use std::cell::RefCell;
use std::collections::HashMap;

use pyo3::prelude::{Bound, PyAny, PyRef, PyResult, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyDict, PyDictMethods, PyTypeMethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use super::error::{js_exception, js_timeout};
use super::host_apis::module_loader::{self, ModuleResolveResult};
use super::runtime::{ExecutionTimeout, SharedIsolate};
use super::v8value::Value;

/// Compiled ECMAScript module bound to a context.
#[gen_stub_pyclass]
#[pyclass(unsendable)]
pub(super) struct Module {
    pub(crate) module: v8::Global<v8::Module>,
    pub(crate) context: v8::Global<v8::Context>,
    pub(crate) isolate: SharedIsolate,
    pub(crate) isolate_id: u64,
    pub(crate) specifier: String,
    pub(crate) dependencies: Vec<v8::Global<v8::Module>>,
}

#[derive(Default)]
struct ModuleResolver {
    sources: HashMap<String, String>,
    modules: HashMap<String, v8::Global<v8::Module>>,
}

thread_local! {
    static MODULE_RESOLVER: RefCell<Option<ModuleResolver>> = const { RefCell::new(None) };
}

struct ModuleResolverGuard(Option<ModuleResolver>);

impl ModuleResolverGuard {
    fn install(resolver: ModuleResolver) -> Self {
        MODULE_RESOLVER.with(|cell| Self(cell.replace(Some(resolver))))
    }
}

impl Drop for ModuleResolverGuard {
    fn drop(&mut self) {
        MODULE_RESOLVER.with(|cell| {
            cell.replace(self.0.take());
        });
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl Module {
    /// Return this module's specifier.
    #[getter]
    fn specifier(&self) -> &str {
        &self.specifier
    }

    /// Return V8's current module status name.
    #[getter]
    fn status(&self) -> &'static str {
        let mut isolate = self.isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate));
        let scope = &mut scope.init();
        let module = v8::Local::new(scope, &self.module);

        module_status_name(module.get_status())
    }

    /// Instantiate this module, optionally resolving imports from a dict.
    #[pyo3(signature = (imports=None))]
    fn instantiate(
        &mut self,
        #[gen_stub(override_type(type_repr = "_ModuleImportsLike | None", imports = ()))]
        imports: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<bool> {
        let resolver = self.import_resolver(imports)?;
        let mut isolate = self.isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate));
        let scope = &mut scope.init();
        let context = v8::Local::new(scope, &self.context);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(let scope, &mut **scope);
        let module = v8::Local::new(scope, &self.module);
        let _resolver_guard = ModuleResolverGuard::install(resolver);

        let instantiated = module
            .instantiate_module(scope, resolve_module_callback)
            .ok_or_else(|| js_exception(scope, "Failed to instantiate module."))?;

        self.dependencies = MODULE_RESOLVER.with(|cell| {
            cell.borrow()
                .as_ref()
                .map(|resolver| {
                    resolver
                        .modules
                        .iter()
                        .filter(|(specifier, _)| *specifier != &self.specifier)
                        .map(|(_, module)| module.clone())
                        .collect()
                })
                .unwrap_or_default()
        });

        Ok(instantiated)
    }

    /// Evaluate this instantiated module and return the completion value.
    #[pyo3(signature = (timeout_ms=None))]
    fn evaluate(&mut self, timeout_ms: Option<u64>) -> PyResult<Value> {
        let mut isolate = self.isolate.borrow_mut();
        let timeout = ExecutionTimeout::arm((**isolate).thread_safe_handle(), timeout_ms);
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate));
        let scope = &mut scope.init();
        let context = v8::Local::new(scope, &self.context);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(let scope, &mut **scope);
        let module = v8::Local::new(scope, &self.module);

        let result = module.evaluate(scope).ok_or_else(|| {
            if scope.has_terminated() {
                scope.cancel_terminate_execution();
                js_timeout()
            } else {
                js_exception(scope, "Failed to evaluate module.")
            }
        })?;
        drop(timeout);

        Ok(Value::from_local(
            scope,
            result,
            self.context.clone(),
            self.isolate.clone(),
            self.isolate_id,
        ))
    }

    /// Return the module namespace object.
    fn namespace(&self) -> PyResult<Value> {
        let mut isolate = self.isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate));
        let scope = &mut scope.init();
        let context = v8::Local::new(scope, &self.context);
        let _scope = &mut v8::ContextScope::new(scope, context);
        let module = v8::Local::new(_scope, &self.module);

        if matches!(
            module.get_status(),
            v8::ModuleStatus::Uninstantiated | v8::ModuleStatus::Instantiating
        ) {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Module must be instantiated before reading its namespace.",
            ));
        }

        let namespace = module.get_module_namespace();

        Ok(Value::from_local(
            _scope,
            namespace,
            self.context.clone(),
            self.isolate.clone(),
            self.isolate_id,
        ))
    }
}

impl Module {
    fn import_resolver(&self, imports: Option<&Bound<'_, PyAny>>) -> PyResult<ModuleResolver> {
        let mut resolver = ModuleResolver::default();
        resolver
            .modules
            .insert(self.specifier.clone(), self.module.clone());

        let Some(imports) = imports else {
            return Ok(resolver);
        };

        if imports.is_none() {
            return Ok(resolver);
        }

        let imports = imports.cast::<PyDict>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("Module imports must be a dict.")
        })?;

        for (key, value) in imports.iter() {
            let specifier: String = key.extract()?;

            if let Ok(module) = value.extract::<PyRef<'_, Module>>() {
                module.ensure_compatible(self.isolate_id)?;
                resolver.modules.insert(specifier, module.module.clone());
                continue;
            }

            if let Ok(source) = value.extract::<String>() {
                resolver.sources.insert(specifier, source);
                continue;
            }

            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Import '{}' must be a source string or v8.Module, got '{}'.",
                specifier,
                value.get_type().name()?
            )));
        }

        Ok(resolver)
    }

    pub(crate) fn ensure_compatible(&self, isolate_id: u64) -> PyResult<()> {
        if self.isolate_id == isolate_id {
            return Ok(());
        }

        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "Module belongs to a different Isolate.",
        ))
    }
}

pub(crate) fn resolve_module_callback<'s>(
    context: v8::Local<'s, v8::Context>,
    specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
    referrer: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Module>> {
    v8::callback_scope!(unsafe scope, context);
    let specifier = specifier.to_rust_string_lossy(scope);

    if let Some(module) = MODULE_RESOLVER.with(|cell| {
        cell.borrow().as_ref().and_then(|resolver| {
            resolver
                .modules
                .get(&specifier)
                .map(|module| v8::Local::new(scope, module))
        })
    }) {
        return Some(module);
    }

    let source_text = MODULE_RESOLVER.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|resolver| resolver.sources.get(&specifier).cloned())
    });
    let Some(source_text) = source_text else {
        return match module_loader::resolve_module(
            scope,
            context,
            &specifier,
            _import_attributes,
            referrer,
        ) {
            ModuleResolveResult::Resolved(module) => Some(module),
            ModuleResolveResult::NotHandled => {
                throw_resolve_error(scope, &format!("Cannot resolve module '{specifier}'."));
                None
            }
            ModuleResolveResult::Failed => None,
        };
    };

    let source = match v8::String::new(scope, &source_text) {
        Some(source) => source,
        None => {
            throw_resolve_error(scope, "Failed to create module source string.");
            return None;
        }
    };
    let Some(origin) = module_origin(scope, &specifier) else {
        throw_resolve_error(scope, "Failed to create module origin.");
        return None;
    };
    let mut source = v8::script_compiler::Source::new(source, Some(&origin));
    let module = v8::script_compiler::compile_module(scope, &mut source)?;
    module_loader::register_module(scope, context, &specifier, module);

    MODULE_RESOLVER.with(|cell| {
        if let Some(resolver) = cell.borrow_mut().as_mut() {
            resolver
                .modules
                .insert(specifier, v8::Global::new(scope, module));
        }
    });

    Some(module)
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

fn throw_resolve_error(scope: &mut v8::PinScope<'_, '_>, message: &str) {
    if let Some(message) = v8::String::new(scope, message) {
        let error = v8::Exception::error(scope, message);
        scope.throw_exception(error);
    }
}

fn module_status_name(status: v8::ModuleStatus) -> &'static str {
    match status {
        v8::ModuleStatus::Uninstantiated => "uninstantiated",
        v8::ModuleStatus::Instantiating => "instantiating",
        v8::ModuleStatus::Instantiated => "instantiated",
        v8::ModuleStatus::Evaluating => "evaluating",
        v8::ModuleStatus::Evaluated => "evaluated",
        v8::ModuleStatus::Errored => "errored",
    }
}
