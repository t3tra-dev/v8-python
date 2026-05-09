use std::rc::Rc;

use pyo3::PyClassInitializer;
use pyo3::prelude::{Bound, Py, PyAny, PyModule, PyResult, Python, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyModuleMethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::context::Context;
use crate::host_apis::{HostAPI, HostAPIDefinition};
use crate::runtime::SharedIsolate;
use crate::v8value::{WasmModuleCache, WasmModuleCacheHandle, compile_wasm_module};

pub(crate) struct WebAssemblyDefinition {
    loader: Option<Py<PyAny>>,
    allow_code_generation: bool,
    cache: Option<Py<WasmModuleCache>>,
}

/// Installs WebAssembly streaming helpers and configures Wasm code generation.
#[gen_stub_pyclass]
#[pyclass(extends = HostAPI, module = "v8.api", name = "WebAssembly")]
pub(crate) struct WebAssemblyAPI {
    loader: Option<Py<PyAny>>,
    allow_code_generation: bool,
    cache: Option<Py<WasmModuleCache>>,
}

struct WebAssemblyRuntime {
    loader: Option<Py<PyAny>>,
    allow_code_generation: bool,
    cache: Option<WasmModuleCacheHandle>,
}

impl WebAssemblyDefinition {
    pub(crate) fn clone_ref(&self, py: Python<'_>) -> Self {
        Self {
            loader: self.loader.as_ref().map(|loader| loader.clone_ref(py)),
            allow_code_generation: self.allow_code_generation,
            cache: self.cache.as_ref().map(|cache| cache.clone_ref(py)),
        }
    }
}

impl WebAssemblyRuntime {
    fn new(py: Python<'_>, definition: &WebAssemblyDefinition) -> Self {
        Self {
            loader: definition
                .loader
                .as_ref()
                .map(|loader| loader.clone_ref(py)),
            allow_code_generation: definition.allow_code_generation,
            cache: definition
                .cache
                .as_ref()
                .map(|cache| cache.bind(py).borrow().handle()),
        }
    }

    fn load_streaming_source(&self, py: Python<'_>, source: &str) -> PyResult<Option<Vec<u8>>> {
        let Some(loader) = &self.loader else {
            return Ok(None);
        };
        let loader = loader.bind(py);
        let loaded = if loader.is_callable() {
            loader.call1((source,))?
        } else {
            loader.call_method1("get", (source,))?
        };

        if loaded.is_none() {
            return Ok(None);
        }

        loaded.extract::<Vec<u8>>().map(Some).map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err(
                "WebAssembly loader must return bytes or a sequence of integers.",
            )
        })
    }

    fn compile_module<'s>(
        &self,
        scope: &v8::PinScope<'s, '_>,
        wire_bytes: &[u8],
    ) -> Result<v8::Local<'s, v8::WasmModuleObject>, String> {
        if !self.allow_code_generation {
            return Err("Wasm code generation is disallowed by v8.api.WebAssembly.".to_owned());
        }

        if let Some(cache) = &self.cache {
            return cache
                .compile_or_get(scope, wire_bytes)
                .map_err(|err| err.to_string());
        }

        compile_wasm_module(scope, wire_bytes).map_err(|err| err.to_string())
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl WebAssemblyAPI {
    /// Create a WebAssembly HostAPI configuration.
    #[gen_stub(override_return_type(type_repr = "WebAssembly", imports = ()))]
    #[new]
    #[pyo3(signature = (loader = None, *, allow_code_generation = true, cache = None))]
    fn new(
        #[gen_stub(override_type(
            type_repr = "_WebAssemblyLoaderLike | None",
            imports = ()
        ))]
        loader: Option<Py<PyAny>>,
        allow_code_generation: bool,
        #[gen_stub(override_type(type_repr = "v8.WasmModuleCache | None", imports = ("v8",)))]
        cache: Option<Py<WasmModuleCache>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        validate_loader(loader.as_ref())?;

        Ok(PyClassInitializer::from(HostAPI).add_subclass(Self {
            loader,
            allow_code_generation,
            cache,
        }))
    }

    /// Return whether WebAssembly code generation is allowed.
    #[getter]
    fn allow_code_generation(&self) -> bool {
        self.allow_code_generation
    }

    /// Return the Python streaming loader, if one was configured.
    #[getter]
    #[gen_stub(override_return_type(type_repr = "_WebAssemblyLoaderLike | None", imports = ()))]
    fn loader(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.loader.as_ref().map(|loader| loader.clone_ref(py))
    }

    /// Return the optional compiled Wasm module cache.
    #[getter]
    #[gen_stub(override_return_type(type_repr = "v8.WasmModuleCache | None", imports = ("v8",)))]
    fn cache(&self, py: Python<'_>) -> Option<Py<WasmModuleCache>> {
        self.cache.as_ref().map(|cache| cache.clone_ref(py))
    }
}

pub(crate) fn add_class(api_module: &Bound<'_, PyModule>) -> PyResult<()> {
    api_module.add_class::<WebAssemblyAPI>()
}

pub(crate) fn definition_from_python(
    api: &Bound<'_, PyAny>,
) -> PyResult<Option<HostAPIDefinition>> {
    if !api.is_instance_of::<WebAssemblyAPI>() {
        return Ok(None);
    }

    let web_assembly = api.extract::<pyo3::PyRef<'_, WebAssemblyAPI>>()?;
    let py = api.py();

    Ok(Some(HostAPIDefinition::WebAssembly(
        WebAssemblyDefinition {
            loader: web_assembly
                .loader
                .as_ref()
                .map(|loader| loader.clone_ref(py)),
            allow_code_generation: web_assembly.allow_code_generation,
            cache: web_assembly.cache.as_ref().map(|cache| cache.clone_ref(py)),
        },
    )))
}

pub(crate) fn install(
    py: Python<'_>,
    context: &mut Context,
    definition: &WebAssemblyDefinition,
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
    let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
    let scope = &mut scope.init();
    let local_context = v8::Local::new(scope, context_global);
    local_context.set_slot(Rc::new(WebAssemblyRuntime::new(py, definition)));
    let scope = &mut v8::ContextScope::new(scope, local_context);
    install_streaming_wrappers(scope, local_context)?;

    Ok(())
}

pub(crate) fn install_on_isolate(isolate: &SharedIsolate) -> PyResult<()> {
    let mut isolate_ref = isolate.borrow_mut();

    isolate_ref.set_allow_wasm_code_generation_callback(allow_wasm_code_generation_callback);
    isolate_ref.set_wasm_streaming_callback(wasm_streaming_callback);

    Ok(())
}

fn install_streaming_wrappers(
    scope: &mut v8::PinScope<'_, '_>,
    context: v8::Local<'_, v8::Context>,
) -> PyResult<()> {
    let global = context.global(scope);
    let key = v8::String::new(scope, "WebAssembly").ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create WebAssembly key.")
    })?;
    let web_assembly = global
        .get(scope, key.into())
        .and_then(|value| value.to_object(scope))
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("WebAssembly object is not available.")
        })?;

    install_web_assembly_function(scope, web_assembly, "compile", compile_callback)?;
    install_web_assembly_function(
        scope,
        web_assembly,
        "compileStreaming",
        compile_streaming_callback,
    )?;
    install_web_assembly_function(
        scope,
        web_assembly,
        "instantiateStreaming",
        instantiate_streaming_callback,
    )
}

fn install_web_assembly_function(
    scope: &mut v8::PinScope<'_, '_>,
    web_assembly: v8::Local<'_, v8::Object>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) -> PyResult<()> {
    let key = v8::String::new(scope, name).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create WebAssembly function name.")
    })?;
    let function = v8::Function::builder(callback)
        .build(scope)
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create WebAssembly function.")
        })?;
    function.set_name(key);

    web_assembly
        .set(scope, key.into(), function.into())
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to install WebAssembly function.")
        })
        .map(|_| ())
}

fn validate_loader(loader: Option<&Py<PyAny>>) -> PyResult<()> {
    let Some(loader) = loader else {
        return Ok(());
    };

    Python::attach(|py| {
        let loader = loader.bind(py);

        if loader.is_callable() {
            return Ok(());
        }

        if loader
            .getattr("get")
            .map(|get| get.is_callable())
            .unwrap_or(false)
        {
            return Ok(());
        }

        Err(pyo3::exceptions::PyTypeError::new_err(
            "WebAssembly loader must be a mapping or callable.",
        ))
    })
}

fn compile_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let result = (|| {
        let source = args.get(0);
        let bytes = buffer_source_bytes(scope, source)?;
        let runtime = current_runtime(scope)?;
        let module = runtime.compile_module(scope, &bytes)?;

        Ok(module.into())
    })();
    let promise = promise_from_result(scope, result);

    rv.set(promise);
}

fn compile_streaming_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let result = (|| {
        let source = args.get(0);
        let source = resolve_streaming_source(scope, source)?;
        let runtime = current_runtime(scope)?;
        let module = runtime.compile_module(scope, &source.bytes)?;

        Ok(module.into())
    })();
    let promise = promise_from_result(scope, result);

    rv.set(promise);
}

fn instantiate_streaming_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let result = (|| {
        let source = args.get(0);
        let source = resolve_streaming_source(scope, source)?;
        let runtime = current_runtime(scope)?;
        let module = runtime.compile_module(scope, &source.bytes)?;
        let imports = (args.length() > 1).then(|| args.get(1));

        instantiate_module(scope, module, imports)
    })();
    let promise = promise_from_result(scope, result);

    rv.set(promise);
}

unsafe extern "C" fn allow_wasm_code_generation_callback(
    context: v8::Local<'_, v8::Context>,
    _source: v8::Local<'_, v8::String>,
) -> bool {
    let scope = std::pin::pin!(unsafe { v8::CallbackScope::new(context) });
    let scope = &mut scope.init();
    let context = scope.get_current_context();

    context
        .get_slot::<WebAssemblyRuntime>()
        .map(|runtime| runtime.allow_code_generation)
        .unwrap_or(true)
}

fn wasm_streaming_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    source: v8::Local<'s, v8::Value>,
    mut streaming: v8::WasmStreaming<false>,
) {
    match streaming_source_bytes(scope, source) {
        Ok(Some(StreamingSource { bytes, url })) => {
            if let Some(url) = url {
                streaming.set_url(&url);
            }
            streaming.on_bytes_received(&bytes);
            streaming.finish();
        }
        Ok(None) => {
            let source_name = value_to_string(scope, source);
            let result = load_source_with_runtime(scope, &source_name);

            match result {
                Ok(bytes) => {
                    streaming.set_url(&source_name);
                    streaming.on_bytes_received(&bytes);
                    streaming.finish();
                }
                Err(message) => {
                    let exception = js_error(scope, &message);
                    streaming.abort(Some(exception));
                }
            }
        }
        Err(message) => {
            let exception = js_error(scope, &message);
            streaming.abort(Some(exception));
        }
    }
}

struct StreamingSource {
    bytes: Vec<u8>,
    url: Option<String>,
}

fn current_runtime(scope: &v8::PinScope<'_, '_>) -> Result<Rc<WebAssemblyRuntime>, String> {
    scope
        .get_current_context()
        .get_slot::<WebAssemblyRuntime>()
        .ok_or_else(|| "WebAssembly HostAPI is not installed for this context.".to_owned())
}

fn resolve_streaming_source(
    scope: &v8::PinScope<'_, '_>,
    source: v8::Local<'_, v8::Value>,
) -> Result<StreamingSource, String> {
    match streaming_source_bytes(scope, source)? {
        Some(source) => Ok(source),
        None => {
            let source_name = value_to_string(scope, source);
            let bytes = load_source_with_runtime(scope, &source_name)?;

            Ok(StreamingSource {
                bytes,
                url: Some(source_name),
            })
        }
    }
}

fn buffer_source_bytes(
    scope: &v8::PinScope<'_, '_>,
    source: v8::Local<'_, v8::Value>,
) -> Result<Vec<u8>, String> {
    streaming_source_bytes(scope, source)?
        .map(|source| source.bytes)
        .ok_or_else(|| {
            "WebAssembly.compile source must be an ArrayBuffer or ArrayBufferView.".to_owned()
        })
}

fn instantiate_module<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    module: v8::Local<'s, v8::WasmModuleObject>,
    imports: Option<v8::Local<'s, v8::Value>>,
) -> Result<v8::Local<'s, v8::Value>, String> {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let web_assembly_key = v8::String::new(scope, "WebAssembly")
        .ok_or_else(|| "Failed to create WebAssembly key.".to_owned())?;
    let instance_key = v8::String::new(scope, "Instance")
        .ok_or_else(|| "Failed to create WebAssembly.Instance key.".to_owned())?;
    let web_assembly = global
        .get(scope, web_assembly_key.into())
        .and_then(|value| value.to_object(scope))
        .ok_or_else(|| "WebAssembly object is not available.".to_owned())?;
    let instance_constructor = web_assembly
        .get(scope, instance_key.into())
        .ok_or_else(|| "WebAssembly.Instance is not available.".to_owned())?;
    let instance_constructor = v8::Local::<v8::Function>::try_from(instance_constructor)
        .map_err(|_| "WebAssembly.Instance is not callable.".to_owned())?;
    let mut args = vec![module.into()];

    if let Some(imports) = imports {
        args.push(imports);
    }

    let instance = instance_constructor
        .new_instance(scope, &args)
        .ok_or_else(|| "Failed to instantiate WebAssembly module.".to_owned())?;
    let result = v8::Object::new(scope);
    let module_key = v8::String::new(scope, "module")
        .ok_or_else(|| "Failed to create instantiateStreaming result key.".to_owned())?;
    let instance_key = v8::String::new(scope, "instance")
        .ok_or_else(|| "Failed to create instantiateStreaming result key.".to_owned())?;

    result
        .set(scope, module_key.into(), module.into())
        .ok_or_else(|| "Failed to set instantiateStreaming module result.".to_owned())?;
    result
        .set(scope, instance_key.into(), instance.into())
        .ok_or_else(|| "Failed to set instantiateStreaming instance result.".to_owned())?;

    Ok(result.into())
}

fn promise_from_result<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    result: Result<v8::Local<'s, v8::Value>, String>,
) -> v8::Local<'s, v8::Value> {
    let Some(resolver) = v8::PromiseResolver::new(scope) else {
        let exception = js_error(scope, "Failed to create WebAssembly Promise.");
        scope.throw_exception(exception);
        return v8::undefined(scope).into();
    };
    let promise = resolver.get_promise(scope);

    match result {
        Ok(value) => {
            resolver.resolve(scope, value);
        }
        Err(message) => {
            let exception = js_error(scope, &message);
            resolver.reject(scope, exception);
        }
    }

    promise.into()
}

fn streaming_source_bytes(
    scope: &v8::PinScope<'_, '_>,
    source: v8::Local<'_, v8::Value>,
) -> Result<Option<StreamingSource>, String> {
    if source.is_array_buffer_view() {
        let view = v8::Local::<v8::ArrayBufferView>::try_from(source)
            .map_err(|_| "Failed to read WebAssembly streaming source.".to_owned())?;
        let mut bytes = vec![0; view.byte_length()];
        let copied = view.copy_contents(&mut bytes);
        bytes.truncate(copied);

        return Ok(Some(StreamingSource { bytes, url: None }));
    }

    if source.is_array_buffer() {
        let buffer = v8::Local::<v8::ArrayBuffer>::try_from(source)
            .map_err(|_| "Failed to read WebAssembly streaming source.".to_owned())?;
        let backing_store = buffer.get_backing_store();
        let length = buffer.byte_length();
        let bytes = backing_store
            .data()
            .map(|data| unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), length) })
            .unwrap_or(&[])
            .to_vec();

        return Ok(Some(StreamingSource { bytes, url: None }));
    }

    if source.is_string() {
        return Ok(None);
    }

    if let Some(value) = source.to_string(scope) {
        let value = value.to_rust_string_lossy(scope);

        if !value.is_empty() {
            return Ok(None);
        }
    }

    Err(
        "WebAssembly streaming source must be an ArrayBuffer, ArrayBufferView, or loader key."
            .to_owned(),
    )
}

fn load_source_with_runtime(
    scope: &v8::PinScope<'_, '_>,
    source_name: &str,
) -> Result<Vec<u8>, String> {
    let context = scope.get_current_context();
    let Some(runtime) = context.get_slot::<WebAssemblyRuntime>() else {
        return Err("WebAssembly HostAPI is not installed for this context.".to_owned());
    };

    Python::attach(|py| runtime.load_streaming_source(py, source_name))
        .map_err(|err| err.to_string())?
        .ok_or_else(|| format!("WebAssembly loader did not return bytes for '{source_name}'."))
}

fn value_to_string(scope: &v8::PinScope<'_, '_>, value: v8::Local<'_, v8::Value>) -> String {
    if value.is_undefined() {
        return "undefined".to_owned();
    }

    if value.is_null() {
        return "null".to_owned();
    }

    value
        .to_string(scope)
        .map(|value| value.to_rust_string_lossy(scope))
        .unwrap_or_else(|| "<source>".to_owned())
}

fn js_error<'s>(scope: &mut v8::PinScope<'s, '_>, message: &str) -> v8::Local<'s, v8::Value> {
    let message = v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
    v8::Exception::error(scope, message)
}
