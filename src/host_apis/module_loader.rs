use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::rc::Rc;

use pyo3::IntoPyObjectExt;
use pyo3::PyClassInitializer;
use pyo3::prelude::{Bound, Py, PyAny, PyModule, PyRef, PyResult, Python, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyDict, PyDictMethods, PyModuleMethods, PyTypeMethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::context::Context;
use crate::host_apis::{HostAPI, HostAPIDefinition};
use crate::module::Module;
use crate::v8value::python_to_v8;

#[derive(Clone, Copy)]
enum ImportAttributeLayout {
    Static,
    Dynamic,
}

pub(crate) enum ModuleResolveResult<'s> {
    Resolved(v8::Local<'s, v8::Module>),
    NotHandled,
    Failed,
}

pub(crate) struct ModuleLoaderDefinition {
    resolver: Option<Py<PyAny>>,
    import_meta: Option<Py<PyAny>>,
}

/// Installs host-backed static and dynamic ECMAScript module resolution.
#[gen_stub_pyclass]
#[pyclass(extends = HostAPI, module = "v8.api", name = "ModuleLoader")]
pub(crate) struct ModuleLoaderAPI {
    resolver: Option<Py<PyAny>>,
    import_meta: Option<Py<PyAny>>,
}

struct ModuleLoaderRuntime {
    resolver: Option<Py<PyAny>>,
    import_meta: Option<Py<PyAny>>,
    isolate_id: u64,
    modules: RefCell<HashMap<String, v8::Global<v8::Module>>>,
    module_specifiers: RefCell<HashMap<i32, String>>,
}

enum ModuleLoad {
    Source(String),
    Module {
        module: v8::Global<v8::Module>,
        specifier: String,
    },
    Missing,
}

impl ModuleLoaderDefinition {
    pub(crate) fn clone_ref(&self, py: Python<'_>) -> Self {
        Self {
            resolver: self
                .resolver
                .as_ref()
                .map(|resolver| resolver.clone_ref(py)),
            import_meta: self
                .import_meta
                .as_ref()
                .map(|import_meta| import_meta.clone_ref(py)),
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl ModuleLoaderAPI {
    /// Create a module loader from a mapping or resolver callback.
    #[gen_stub(override_return_type(type_repr = "ModuleLoader", imports = ()))]
    #[new]
    #[pyo3(signature = (resolver=None, *, import_meta=None))]
    fn new(
        py: Python<'_>,
        #[gen_stub(override_type(
            type_repr = "collections.abc.Mapping[str, str | v8.Module] | collections.abc.Callable[[str, str | None, dict[str, str]], str | v8.Module | None] | None",
            imports = ("collections.abc", "v8")
        ))]
        resolver: Option<Py<PyAny>>,
        #[gen_stub(override_type(
            type_repr = "collections.abc.Mapping[str, object] | collections.abc.Callable[[str], collections.abc.Mapping[str, object] | None] | None",
            imports = ("collections.abc",)
        ))]
        import_meta: Option<Py<PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        validate_resolver(py, resolver.as_ref())?;
        validate_import_meta(py, import_meta.as_ref())?;

        Ok(PyClassInitializer::from(HostAPI).add_subclass(Self {
            resolver,
            import_meta,
        }))
    }
}

impl ModuleLoaderRuntime {
    fn new(py: Python<'_>, definition: &ModuleLoaderDefinition, isolate_id: u64) -> Self {
        Self {
            resolver: definition
                .resolver
                .as_ref()
                .map(|resolver| resolver.clone_ref(py)),
            import_meta: definition
                .import_meta
                .as_ref()
                .map(|import_meta| import_meta.clone_ref(py)),
            isolate_id,
            modules: RefCell::new(HashMap::new()),
            module_specifiers: RefCell::new(HashMap::new()),
        }
    }

    fn cached_module<'s>(
        &self,
        scope: &v8::PinScope<'s, '_>,
        specifier: &str,
    ) -> Option<v8::Local<'s, v8::Module>> {
        let module = self.modules.borrow().get(specifier).cloned();
        module.map(|module| v8::Local::new(scope, &module))
    }

    fn remember_module<'s>(
        &self,
        scope: &v8::PinScope<'s, '_>,
        cache_key: &str,
        module_specifier: &str,
        module: v8::Local<'s, v8::Module>,
    ) {
        let global = v8::Global::new(scope, module);
        let mut modules = self.modules.borrow_mut();
        modules.insert(cache_key.to_owned(), global.clone());
        modules.insert(module_specifier.to_owned(), global);
        self.module_specifiers.borrow_mut().insert(
            module.get_identity_hash().get(),
            module_specifier.to_owned(),
        );
    }

    fn module_specifier(&self, module: v8::Local<'_, v8::Module>) -> Option<String> {
        self.module_specifiers
            .borrow()
            .get(&module.get_identity_hash().get())
            .cloned()
    }

    fn resolve(
        &self,
        py: Python<'_>,
        specifier: &str,
        referrer: Option<&str>,
        attributes: &Bound<'_, PyDict>,
    ) -> PyResult<ModuleLoad> {
        let Some(resolver) = &self.resolver else {
            return Ok(ModuleLoad::Missing);
        };
        let resolver = resolver.bind(py);
        let result = if resolver.is_callable() {
            let referrer = match referrer {
                Some(referrer) => referrer.into_py_any(py)?,
                None => py.None(),
            };
            resolver.call1((specifier, referrer, attributes))?
        } else if resolver.hasattr("get")? {
            resolver.call_method1("get", (specifier, py.None()))?
        } else {
            resolver.get_item(specifier)?
        };

        self.coerce_module_load(result)
    }

    fn coerce_module_load(&self, result: Bound<'_, PyAny>) -> PyResult<ModuleLoad> {
        if result.is_none() {
            return Ok(ModuleLoad::Missing);
        }

        if let Ok(module) = result.extract::<PyRef<'_, Module>>() {
            module.ensure_compatible(self.isolate_id)?;
            return Ok(ModuleLoad::Module {
                module: module.module.clone(),
                specifier: module.specifier.clone(),
            });
        }

        if let Ok(source) = result.extract::<String>() {
            return Ok(ModuleLoad::Source(source));
        }

        Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "ModuleLoader resolver must return a source string, v8.Module, or None, got '{}'.",
            result.get_type().name()?
        )))
    }
}

pub(crate) fn add_class(api_module: &Bound<'_, PyModule>) -> PyResult<()> {
    api_module.add_class::<ModuleLoaderAPI>()
}

pub(crate) fn definition_from_python(
    api: &Bound<'_, PyAny>,
) -> PyResult<Option<HostAPIDefinition>> {
    if !api.is_instance_of::<ModuleLoaderAPI>() {
        return Ok(None);
    }

    let loader = api.extract::<PyRef<'_, ModuleLoaderAPI>>()?;
    let py = api.py();

    Ok(Some(HostAPIDefinition::ModuleLoader(
        ModuleLoaderDefinition {
            resolver: loader
                .resolver
                .as_ref()
                .map(|resolver| resolver.clone_ref(py)),
            import_meta: loader
                .import_meta
                .as_ref()
                .map(|import_meta| import_meta.clone_ref(py)),
        },
    )))
}

pub(crate) fn install(
    py: Python<'_>,
    context: &mut Context,
    definition: &ModuleLoaderDefinition,
) -> PyResult<()> {
    let isolate = context
        .isolate
        .as_ref()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive."))?;
    let context_global = context
        .context
        .as_ref()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive."))?;

    let mut isolate_ref = isolate.borrow_mut();
    isolate_ref.set_host_initialize_import_meta_object_callback(initialize_import_meta_callback);
    isolate_ref.set_host_import_module_dynamically_callback(dynamic_import_callback);

    let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
    let scope = &mut scope.init();
    let local_context = v8::Local::new(scope, context_global);
    local_context.set_slot(Rc::new(ModuleLoaderRuntime::new(
        py,
        definition,
        context.isolate_id,
    )));

    Ok(())
}

pub(crate) fn register_module<'s>(
    scope: &v8::PinScope<'s, '_>,
    context: v8::Local<'s, v8::Context>,
    specifier: &str,
    module: v8::Local<'s, v8::Module>,
) {
    if let Some(loader) = context.get_slot::<ModuleLoaderRuntime>() {
        loader.remember_module(scope, specifier, specifier, module);
    }
}

pub(crate) fn resolve_module<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    context: v8::Local<'s, v8::Context>,
    specifier: &str,
    import_attributes: v8::Local<'s, v8::FixedArray>,
    referrer: v8::Local<'s, v8::Module>,
) -> ModuleResolveResult<'s> {
    let Some(loader) = context.get_slot::<ModuleLoaderRuntime>() else {
        return ModuleResolveResult::NotHandled;
    };

    if let Some(module) = loader.cached_module(scope, specifier) {
        return ModuleResolveResult::Resolved(module);
    }

    let referrer = loader.module_specifier(referrer);
    match load_module(
        scope,
        &loader,
        specifier,
        referrer.as_deref(),
        import_attributes,
        ImportAttributeLayout::Static,
    ) {
        Ok(Some(module)) => ModuleResolveResult::Resolved(module),
        Ok(None) => ModuleResolveResult::NotHandled,
        Err(message) => {
            throw_js_error(scope, &message);
            ModuleResolveResult::Failed
        }
    }
}

fn dynamic_import_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    _host_defined_options: v8::Local<'s, v8::Data>,
    resource_name: v8::Local<'s, v8::Value>,
    specifier: v8::Local<'s, v8::String>,
    import_attributes: v8::Local<'s, v8::FixedArray>,
) -> Option<v8::Local<'s, v8::Promise>> {
    let fallback_resolver = v8::PromiseResolver::new(scope)?;
    let fallback_promise = fallback_resolver.get_promise(scope);

    match dynamic_import(scope, resource_name, specifier, import_attributes) {
        Ok(promise) => Some(promise),
        Err(message) => {
            reject_with_message(scope, fallback_resolver, &message);
            Some(fallback_promise)
        }
    }
}

fn dynamic_import<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    resource_name: v8::Local<'s, v8::Value>,
    specifier: v8::Local<'s, v8::String>,
    import_attributes: v8::Local<'s, v8::FixedArray>,
) -> Result<v8::Local<'s, v8::Promise>, String> {
    let context = scope.get_current_context();
    let loader = context
        .get_slot::<ModuleLoaderRuntime>()
        .ok_or_else(|| "No v8.api.ModuleLoader is installed for this context.".to_owned())?;
    let specifier = specifier.to_rust_string_lossy(scope);
    let referrer = resource_name_to_string(scope, resource_name);
    let module = load_module(
        scope,
        &loader,
        &specifier,
        referrer.as_deref(),
        import_attributes,
        ImportAttributeLayout::Dynamic,
    )?
    .ok_or_else(|| format!("Cannot resolve module '{specifier}'."))?;

    module_to_dynamic_import_promise(scope, module)
}

fn module_to_dynamic_import_promise<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    module: v8::Local<'s, v8::Module>,
) -> Result<v8::Local<'s, v8::Promise>, String> {
    if matches!(module.get_status(), v8::ModuleStatus::Uninstantiated) {
        module
            .instantiate_module(scope, crate::module::resolve_module_callback)
            .ok_or_else(|| "Failed to instantiate dynamically imported module.".to_owned())?;
    }

    if matches!(module.get_status(), v8::ModuleStatus::Errored) {
        return Err(v8_value_to_string(scope, module.get_exception()));
    }

    if matches!(module.get_status(), v8::ModuleStatus::Evaluated) {
        return resolved_namespace_promise(scope, module);
    }

    let result = module
        .evaluate(scope)
        .ok_or_else(|| "Failed to evaluate dynamically imported module.".to_owned())?;

    if matches!(module.get_status(), v8::ModuleStatus::Errored) {
        return Err(v8_value_to_string(scope, module.get_exception()));
    }

    if let Ok(evaluation) = v8::Local::<v8::Promise>::try_from(result) {
        let namespace = module.get_module_namespace();
        let handler = v8::Function::builder(return_callback_data)
            .data(namespace)
            .build(scope)
            .ok_or_else(|| "Failed to create dynamic import continuation.".to_owned())?;

        return evaluation
            .then(scope, handler)
            .ok_or_else(|| "Failed to chain dynamic import evaluation.".to_owned());
    }

    resolved_namespace_promise(scope, module)
}

fn resolved_namespace_promise<'s>(
    scope: &v8::PinScope<'s, '_>,
    module: v8::Local<'s, v8::Module>,
) -> Result<v8::Local<'s, v8::Promise>, String> {
    let resolver = v8::PromiseResolver::new(scope)
        .ok_or_else(|| "Failed to create dynamic import Promise.".to_owned())?;
    let namespace = module.get_module_namespace();
    resolver.resolve(scope, namespace);

    Ok(resolver.get_promise(scope))
}

fn load_module<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    loader: &ModuleLoaderRuntime,
    specifier: &str,
    referrer: Option<&str>,
    import_attributes: v8::Local<'s, v8::FixedArray>,
    layout: ImportAttributeLayout,
) -> Result<Option<v8::Local<'s, v8::Module>>, String> {
    if let Some(module) = loader.cached_module(scope, specifier) {
        return Ok(Some(module));
    }

    let loaded = Python::attach(|py| {
        let attributes = import_attributes_to_dict(py, scope, import_attributes, layout)?;
        loader.resolve(py, specifier, referrer, &attributes)
    })
    .map_err(|err| err.to_string())?;

    match loaded {
        ModuleLoad::Source(source) => {
            let Some(module) = compile_module_source(scope, specifier, &source) else {
                return Err(format!("Failed to compile module '{specifier}'."));
            };
            loader.remember_module(scope, specifier, specifier, module);
            Ok(Some(module))
        }
        ModuleLoad::Module {
            module,
            specifier: module_specifier,
        } => {
            let module = v8::Local::new(scope, &module);
            loader.remember_module(scope, specifier, &module_specifier, module);
            Ok(Some(module))
        }
        ModuleLoad::Missing => Ok(None),
    }
}

fn compile_module_source<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    specifier: &str,
    source_text: &str,
) -> Option<v8::Local<'s, v8::Module>> {
    let source = v8::String::new(scope, source_text)?;
    let origin = module_origin(scope, specifier)?;
    let mut source = v8::script_compiler::Source::new(source, Some(&origin));

    v8::script_compiler::compile_module(scope, &mut source)
}

extern "C" fn initialize_import_meta_callback(
    context: v8::Local<'_, v8::Context>,
    module: v8::Local<'_, v8::Module>,
    meta: v8::Local<'_, v8::Object>,
) {
    v8::callback_scope!(unsafe scope, context);
    let Some(loader) = context.get_slot::<ModuleLoaderRuntime>() else {
        return;
    };
    let specifier = loader
        .module_specifier(module)
        .unwrap_or_else(|| "<module>".to_owned());

    let result = Python::attach(|py| install_import_meta(py, scope, &loader, &specifier, meta));
    if let Err(err) = result {
        throw_js_error(scope, &err.to_string());
    }
}

fn install_import_meta(
    py: Python<'_>,
    scope: &mut v8::PinScope<'_, '_>,
    loader: &ModuleLoaderRuntime,
    specifier: &str,
    meta: v8::Local<'_, v8::Object>,
) -> PyResult<()> {
    let Some(import_meta) = &loader.import_meta else {
        return Ok(());
    };
    let import_meta = import_meta.bind(py);
    let properties = if import_meta.is_callable() {
        import_meta.call1((specifier,))?
    } else {
        import_meta.clone()
    };

    if properties.is_none() {
        return Ok(());
    }

    let properties = properties.cast::<PyDict>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err(
            "ModuleLoader import_meta must be a dict or return a dict.",
        )
    })?;

    for (key, value) in properties.iter() {
        let key: String = key.extract()?;
        let key = v8::String::new(scope, &key).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create import.meta key.")
        })?;
        let value = python_to_v8(py, scope, &value, loader.isolate_id, 0)?;
        meta.create_data_property(scope, key.into(), value)
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to create import.meta property.")
            })?;
    }

    Ok(())
}

fn import_attributes_to_dict<'py>(
    py: Python<'py>,
    scope: &v8::PinScope<'_, '_>,
    import_attributes: v8::Local<'_, v8::FixedArray>,
    layout: ImportAttributeLayout,
) -> PyResult<Bound<'py, PyDict>> {
    let attributes = PyDict::new(py);
    let stride = match layout {
        ImportAttributeLayout::Static => 3,
        ImportAttributeLayout::Dynamic => 2,
    };
    let mut index = 0;

    while index + 1 < import_attributes.length() {
        let Some(key) = import_attribute_string(scope, import_attributes, index) else {
            break;
        };
        let Some(value) = import_attribute_string(scope, import_attributes, index + 1) else {
            break;
        };
        attributes.set_item(key, value)?;
        index += stride;
    }

    Ok(attributes)
}

fn import_attribute_string(
    scope: &v8::PinScope<'_, '_>,
    import_attributes: v8::Local<'_, v8::FixedArray>,
    index: usize,
) -> Option<String> {
    let data = import_attributes.get(scope, index)?;
    let value = v8::Local::<v8::Value>::try_from(data).ok()?;
    Some(value.to_rust_string_lossy(scope))
}

fn module_origin<'s>(
    scope: &v8::PinScope<'s, '_>,
    specifier: &str,
) -> Option<v8::ScriptOrigin<'s>> {
    let resource_name = v8::String::new(scope, specifier)?;

    Some(v8::ScriptOrigin::new(
        scope,
        resource_name.into(),
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        true,
        None,
    ))
}

fn validate_resolver(py: Python<'_>, resolver: Option<&Py<PyAny>>) -> PyResult<()> {
    let Some(resolver) = resolver else {
        return Ok(());
    };
    let resolver = resolver.bind(py);

    if resolver.is_callable() || resolver.hasattr("__getitem__")? {
        return Ok(());
    }

    Err(pyo3::exceptions::PyTypeError::new_err(
        "ModuleLoader resolver must be callable or mapping-like.",
    ))
}

fn validate_import_meta(py: Python<'_>, import_meta: Option<&Py<PyAny>>) -> PyResult<()> {
    let Some(import_meta) = import_meta else {
        return Ok(());
    };
    let import_meta = import_meta.bind(py);

    if import_meta.is_callable() || import_meta.is_instance_of::<PyDict>() {
        return Ok(());
    }

    Err(pyo3::exceptions::PyTypeError::new_err(
        "ModuleLoader import_meta must be callable or dict.",
    ))
}

fn resource_name_to_string(
    scope: &v8::PinScope<'_, '_>,
    resource_name: v8::Local<'_, v8::Value>,
) -> Option<String> {
    if resource_name.is_undefined() || resource_name.is_null() {
        return None;
    }

    resource_name
        .to_string(scope)
        .map(|resource_name| resource_name.to_rust_string_lossy(scope))
}

fn v8_value_to_string(scope: &v8::PinScope<'_, '_>, value: v8::Local<'_, v8::Value>) -> String {
    if value.is_undefined() {
        return "undefined".to_owned();
    }

    if value.is_null() {
        return "null".to_owned();
    }

    value
        .to_string(scope)
        .map(|value| value.to_rust_string_lossy(scope))
        .unwrap_or_else(|| "<exception>".to_owned())
}

fn reject_with_message(
    scope: &mut v8::PinScope<'_, '_>,
    resolver: v8::Local<'_, v8::PromiseResolver>,
    message: &str,
) {
    let message = v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::error(scope, message);
    resolver.reject(scope, exception);
}

fn return_callback_data<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    rv.set(args.data());
}

fn throw_js_error(scope: &mut v8::PinScope<'_, '_>, message: &str) {
    let message = v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::error(scope, message);
    scope.throw_exception(exception);
}
