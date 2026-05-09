use pyo3::prelude::{Bound, Py, PyAny, PyRef, PyResult, Python, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyBool, PyBytes, PyInt};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use super::error::{js_exception, js_timeout};
use super::host_apis::{self, HostAPIDefinition};
use super::module::Module;
use super::profile::{self, BaseProfile, HostFunctionDefinition};
use super::runtime::SharedIsolate;
use super::script::Script;
use super::templates::HostClassDefinition;
use super::v8value::{
    V8ArrayBuffer, V8CompiledWasmModule, V8External, V8Function, V8Object, V8Private, V8String,
    V8Value, V8WasmModule, Value, WasmModuleCache, compile_wasm_module, copy_bytes_to_array_buffer,
    python_bytes_like_to_vec, python_to_v8, wasm_bytes_from_python,
};

/// A V8 execution context tied to one isolate.
#[gen_stub_pyclass]
#[pyclass(unsendable)]
pub(crate) struct Context {
    pub(crate) context: Option<v8::Global<v8::Context>>,
    pub(crate) isolate: Option<SharedIsolate>,
    pub(crate) isolate_id: u64,
}

/// Builder for configuring a context before it is created.
#[gen_stub_pyclass]
#[pyclass(unsendable)]
pub(crate) struct ContextBuilder {
    isolate: Option<SharedIsolate>,
    isolate_id: u64,
    snapshot_index: Option<usize>,
    globals: Vec<(String, Py<PyAny>)>,
    host_functions: Vec<HostFunctionDefinition>,
    host_classes: Vec<HostClassDefinition>,
    host_apis: Vec<HostAPIDefinition>,
    microtasks_policy: Option<v8::MicrotasksPolicy>,
}

enum ArrayBufferSource {
    ByteLength(usize),
    Bytes(Vec<u8>),
}

fn compile_source_text(source: &Bound<'_, PyAny>, isolate_id: u64) -> PyResult<String> {
    if let Ok(source) = source.extract::<PyRef<'_, V8String>>() {
        source.ensure_isolate(isolate_id)?;
        return Ok(source.text.clone());
    }

    source.extract::<String>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err("compile() source must be str or v8.String.")
    })
}

fn cached_data_bytes(
    py: Python<'_>,
    cached_data: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<Vec<u8>>> {
    let Some(cached_data) = cached_data else {
        return Ok(None);
    };
    let Some(bytes) = python_bytes_like_to_vec(py, cached_data)? else {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "cached_data must be bytes, bytearray, or memoryview.",
        ));
    };

    Ok(Some(bytes))
}

fn compile_function_arguments(arguments: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<String>> {
    let Some(arguments) = arguments else {
        return Ok(Vec::new());
    };

    arguments.extract::<Vec<String>>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err(
            "compile_function() arguments must be a sequence of str.",
        )
    })
}

impl Context {
    pub(crate) fn from_isolate(isolate: SharedIsolate, isolate_id: u64) -> PyResult<Self> {
        let context = {
            let mut isolate_ref = isolate.borrow_mut();
            let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
            let scope = &mut scope.init();
            let context = v8::Context::new(scope, Default::default());
            v8::Global::new(scope, context)
        };

        Ok(Self {
            context: Some(context),
            isolate: Some(isolate),
            isolate_id,
        })
    }

    pub(crate) fn from_snapshot(
        isolate: SharedIsolate,
        isolate_id: u64,
        index: usize,
    ) -> PyResult<Self> {
        let context = {
            let mut isolate_ref = isolate.borrow_mut();
            let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
            let scope = &mut scope.init();
            let context =
                v8::Context::from_snapshot(scope, index, Default::default()).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err(
                        "Failed to create Context from snapshot.",
                    )
                })?;
            v8::Global::new(scope, context)
        };

        Ok(Self {
            context: Some(context),
            isolate: Some(isolate),
            isolate_id,
        })
    }

    pub(crate) fn set_global_value(
        &mut self,
        name: &str,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);
        v8::tc_scope!(let scope, &mut **scope);

        let key = v8::String::new(scope, name)
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Failed to create key."))?;
        let value = python_to_v8(value.py(), scope, value, self.isolate_id, 0)?;
        let global = local_context.global(scope);

        global
            .set(scope, key.into(), value)
            .ok_or_else(|| js_exception(scope, "Failed to set global value."))
    }

    pub(crate) fn parse_microtasks_policy(policy: &str) -> PyResult<v8::MicrotasksPolicy> {
        match policy {
            "auto" => Ok(v8::MicrotasksPolicy::Auto),
            "explicit" => Ok(v8::MicrotasksPolicy::Explicit),
            _ => Err(pyo3::exceptions::PyValueError::new_err(
                "microtasks policy must be 'auto' or 'explicit'.",
            )),
        }
    }
}

impl ContextBuilder {
    pub(crate) fn from_isolate(isolate: SharedIsolate, isolate_id: u64) -> Self {
        Self {
            isolate: Some(isolate),
            isolate_id,
            snapshot_index: None,
            globals: Vec::new(),
            host_functions: Vec::new(),
            host_classes: Vec::new(),
            host_apis: Vec::new(),
            microtasks_policy: None,
        }
    }

    pub(crate) fn add_host_function(
        &mut self,
        py: Python<'_>,
        name: Option<String>,
        function: Py<PyAny>,
    ) -> PyResult<()> {
        self.host_functions
            .push(HostFunctionDefinition::new(py, name, function)?);
        Ok(())
    }

    pub(crate) fn add_host_class(
        &mut self,
        py: Python<'_>,
        name: Option<String>,
        cls: Py<PyAny>,
    ) -> PyResult<()> {
        self.host_classes
            .push(HostClassDefinition::new(py, name, cls)?);
        Ok(())
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl ContextBuilder {
    /// Return whether this builder can still build a context.
    fn is_alive(&self) -> bool {
        self.isolate.is_some()
    }

    /// Queue a global value to install when the context is built.
    fn set_global(
        &mut self,
        name: &str,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) {
        self.globals.push((name.to_owned(), value.clone().unbind()));
    }

    /// Register a Python callable as a JavaScript global function for this context.
    #[gen_stub(override_return_type(type_repr = "_HostCallable | _HostFunctionDecorator", imports = ()))]
    #[pyo3(signature = (function=None, *, name=None))]
    fn host_function(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        #[gen_stub(override_type(type_repr = "_HostCallable | None", imports = ()))]
        function: Option<Py<PyAny>>,
        name: Option<String>,
    ) -> PyResult<Py<PyAny>> {
        profile::builder_host_function(py, slf, function, name)
    }

    /// Register a Python class as a JavaScript constructor template for this context.
    #[gen_stub(override_return_type(type_repr = "_HostClassDecorator", imports = ()))]
    #[pyo3(name = "class_", signature = (cls=None, *, name=None))]
    fn class_(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        #[gen_stub(override_type(type_repr = "_HostClassLike | None", imports = ()))] cls: Option<
            Py<PyAny>,
        >,
        name: Option<String>,
    ) -> PyResult<Py<PyAny>> {
        profile::builder_host_class(py, slf, cls, name)
    }

    /// Copy host functions, host classes, and HostAPI installers from a profile.
    fn use_profile(&mut self, py: Python<'_>, profile: PyRef<'_, BaseProfile>) {
        self.host_functions.extend(
            profile
                .host_functions()
                .iter()
                .map(|definition| definition.clone_ref(py)),
        );
        self.host_classes.extend(
            profile
                .host_classes()
                .iter()
                .map(|definition| definition.clone_ref(py)),
        );
        self.host_apis.extend(
            profile
                .host_apis()
                .iter()
                .map(|definition| definition.clone_ref(py)),
        );
    }

    /// Build the context from a snapshot context index.
    #[pyo3(signature = (index = 0))]
    fn use_snapshot(&mut self, index: usize) {
        self.snapshot_index = Some(index);
    }

    /// Set the microtask policy to "auto" or "explicit".
    fn set_microtasks_policy(&mut self, policy: &str) -> PyResult<()> {
        self.microtasks_policy = Some(Context::parse_microtasks_policy(policy)?);
        Ok(())
    }

    /// Consume this builder and create the configured context.
    fn build(&mut self, py: Python<'_>) -> PyResult<Context> {
        let isolate = self.isolate.take().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "ContextBuilder has already been used to build a Context.",
            )
        })?;

        if let Some(policy) = self.microtasks_policy {
            isolate.borrow_mut().set_microtasks_policy(policy);
        }

        for api in &self.host_apis {
            host_apis::install_on_isolate(&isolate, api)?;
        }

        let mut context = if let Some(index) = self.snapshot_index {
            Context::from_snapshot(isolate, self.isolate_id, index)?
        } else {
            Context::from_isolate(isolate, self.isolate_id)?
        };

        for api in &self.host_apis {
            host_apis::install(py, &mut context, api)?;
        }

        for definition in &self.host_functions {
            profile::install_host_function(py, &mut context, definition)?;
        }

        for definition in &self.host_classes {
            super::templates::install_host_class(py, &mut context, definition)?;
        }

        for (name, value) in &self.globals {
            context.set_global_value(name, value.bind(py))?;
        }

        Ok(context)
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl Context {
    /// Return whether this context still owns a live V8 context and isolate.
    fn is_alive(&self) -> bool {
        self.isolate.is_some() && self.context.is_some()
    }

    /// Return the inspector object installed for this context.
    #[gen_stub(override_return_type(type_repr = "Inspector", imports = ()))]
    fn inspector(&self) -> PyResult<host_apis::inspector::Inspector> {
        host_apis::inspector::inspector_from_context(self)
    }

    /// Return V8 heap statistics for this context's isolate.
    #[gen_stub(override_return_type(type_repr = "dict[str, int | bool]", imports = ()))]
    fn heap_statistics(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        super::heap::heap_statistics(py, isolate)
    }

    /// Return per-space V8 heap statistics for this context's isolate.
    #[gen_stub(override_return_type(type_repr = "list[dict[str, int | str]]", imports = ()))]
    fn heap_space_statistics(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        super::heap::heap_space_statistics(py, isolate)
    }

    /// Return V8 heap statistics for generated code and metadata.
    #[gen_stub(override_return_type(type_repr = "dict[str, int]", imports = ()))]
    fn heap_code_statistics(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        super::heap::heap_code_statistics(py, isolate)
    }

    /// Notify V8 that the host is under low-memory pressure.
    fn low_memory_notification(&self) -> PyResult<()> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        super::heap::low_memory_notification(isolate);
        Ok(())
    }

    /// Set V8's memory pressure level for this context's isolate.
    fn memory_pressure(
        &self,
        #[gen_stub(override_type(
            type_repr = "typing.Literal['none', 'moderate', 'critical']",
            imports = ("typing")
        ))]
        level: &str,
    ) -> PyResult<()> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        super::heap::memory_pressure(isolate, level)
    }

    /// Request a V8 garbage collection cycle for testing.
    #[pyo3(signature = (collection_type="full"))]
    fn request_garbage_collection_for_testing(
        &self,
        #[gen_stub(override_type(
            type_repr = "typing.Literal['full', 'minor']",
            imports = ("typing")
        ))]
        collection_type: &str,
    ) -> PyResult<()> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        super::heap::request_garbage_collection_for_testing(isolate, collection_type)
    }

    /// Create a V8 string in this context.
    fn new_string(&mut self, value: &str) -> PyResult<V8String> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let context_global = context.clone();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);

        let string = v8::String::new(scope, value).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create v8::String.")
        })?;

        Ok(V8String::from_context_local(
            scope,
            string,
            value.to_owned(),
            context_global,
            isolate.clone(),
            self.isolate_id,
        ))
    }

    /// Create a V8 private key, optionally using the API-visible key namespace.
    #[pyo3(signature = (name=None, *, for_api=false))]
    #[gen_stub(override_return_type(type_repr = "Private", imports = ()))]
    fn new_private(
        &mut self,
        #[gen_stub(override_type(type_repr = "str | String | None", imports = ()))] name: Option<
            &Bound<'_, PyAny>,
        >,
        for_api: bool,
    ) -> PyResult<V8Private> {
        let name = name
            .map(|name| compile_source_text(name, self.isolate_id))
            .transpose()?;
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);
        let name = name
            .as_deref()
            .map(|name| {
                v8::String::new(scope, name).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to create Private name.")
                })
            })
            .transpose()?;
        let private = if for_api {
            v8::Private::for_api(scope, name)
        } else {
            v8::Private::new(scope, name)
        };

        Ok(V8Private::from_local(
            scope,
            private,
            context.clone(),
            isolate.clone(),
            self.isolate_id,
        ))
    }

    /// Create or retrieve a V8 API private key.
    #[pyo3(signature = (name=None))]
    #[gen_stub(override_return_type(type_repr = "Private", imports = ()))]
    fn private_for_api(
        &mut self,
        #[gen_stub(override_type(type_repr = "str | String | None", imports = ()))] name: Option<
            &Bound<'_, PyAny>,
        >,
    ) -> PyResult<V8Private> {
        self.new_private(name, true)
    }

    /// Create a new V8 object, optionally with internal fields.
    #[pyo3(signature = (internal_field_count=0))]
    #[gen_stub(override_return_type(type_repr = "Object", imports = ()))]
    fn new_object(&mut self, internal_field_count: usize) -> PyResult<V8Object> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);
        let object = if internal_field_count == 0 {
            v8::Object::new(scope)
        } else {
            let template = v8::ObjectTemplate::new(scope);
            if !template.set_internal_field_count(internal_field_count) {
                return Err(pyo3::exceptions::PyOverflowError::new_err(
                    "internal_field_count is too large for V8.",
                ));
            }

            template.new_instance(scope).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to create object.")
            })?
        };
        let handle = V8Value::from_local(
            scope,
            object.into(),
            context.clone(),
            isolate.clone(),
            self.isolate_id,
        );

        Ok(V8Object::from_local(scope, object, handle))
    }

    /// Create a V8 external value that owns a Python payload.
    #[gen_stub(override_return_type(type_repr = "External", imports = ()))]
    fn new_external(
        &mut self,
        #[gen_stub(override_type(type_repr = "object", imports = ()))] payload: Py<PyAny>,
    ) -> PyResult<V8External> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let token = super::runtime::register_external(isolate, payload);

        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);
        let external = v8::External::new(scope, token);
        let handle = V8Value::from_local(
            scope,
            external.into(),
            context.clone(),
            isolate.clone(),
            self.isolate_id,
        );

        Ok(V8External::from_local(scope, external, handle))
    }

    /// Compile a WebAssembly module from wire bytes.
    #[gen_stub(override_return_type(type_repr = "WasmModule", imports = ()))]
    #[pyo3(signature = (wire_bytes, cache = None))]
    fn compile_wasm_module(
        &mut self,
        py: Python<'_>,
        #[gen_stub(override_type(type_repr = "bytes | bytearray | memoryview", imports = ()))]
        wire_bytes: &Bound<'_, PyAny>,
        cache: Option<PyRef<'_, WasmModuleCache>>,
    ) -> PyResult<V8WasmModule> {
        let wire_bytes = wasm_bytes_from_python(py, wire_bytes)?;
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);
        let module = if let Some(cache) = cache {
            cache.handle().compile_or_get(scope, &wire_bytes)?
        } else {
            compile_wasm_module(scope, &wire_bytes)?
        };
        let handle = V8Value::from_local(
            scope,
            module.into(),
            context.clone(),
            isolate.clone(),
            self.isolate_id,
        );

        Ok(V8WasmModule::from_local(scope, module, handle))
    }

    /// Recreate a WebAssembly module object from a compiled module handle.
    #[gen_stub(override_return_type(type_repr = "WasmModule", imports = ()))]
    fn wasm_module_from_compiled(
        &mut self,
        compiled: PyRef<'_, V8CompiledWasmModule>,
    ) -> PyResult<V8WasmModule> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);
        let module = v8::WasmModuleObject::from_compiled_module(scope, compiled.module())
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "Failed to create WebAssembly module from compiled module.",
                )
            })?;
        let handle = V8Value::from_local(
            scope,
            module.into(),
            context.clone(),
            isolate.clone(),
            self.isolate_id,
        );

        Ok(V8WasmModule::from_local(scope, module, handle))
    }

    /// Create an ArrayBuffer from a byte length or bytes-like object.
    #[gen_stub(override_return_type(type_repr = "ArrayBuffer", imports = ()))]
    fn new_array_buffer(
        &mut self,
        #[gen_stub(override_type(type_repr = "int | bytes | bytearray | memoryview", imports = ()))]
        value: &Bound<'_, PyAny>,
    ) -> PyResult<V8ArrayBuffer> {
        let source = if value.is_exact_instance_of::<PyBool>() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "new_array_buffer() expects a byte length or a bytes-like object.",
            ));
        } else if value.is_instance_of::<PyInt>() {
            ArrayBufferSource::ByteLength(value.extract()?)
        } else if let Some(bytes) = python_bytes_like_to_vec(value.py(), value)? {
            ArrayBufferSource::Bytes(bytes)
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "new_array_buffer() expects a byte length or a bytes-like object.",
            ));
        };

        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);

        let array_buffer = match source {
            ArrayBufferSource::ByteLength(byte_length) => v8::ArrayBuffer::new(scope, byte_length),
            ArrayBufferSource::Bytes(bytes) => {
                let array_buffer = v8::ArrayBuffer::new(scope, bytes.len());
                copy_bytes_to_array_buffer(array_buffer, &bytes)?;
                array_buffer
            }
        };
        let handle = V8Value::from_local(
            scope,
            array_buffer.into(),
            context.clone(),
            isolate.clone(),
            self.isolate_id,
        );

        Ok(V8ArrayBuffer::from_local(scope, array_buffer, handle))
    }

    /// Convert a Python object into a JavaScript value in this context.
    #[pyo3(name = "from_python")]
    fn value_from_python(
        &mut self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<Value> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);
        let value = python_to_v8(value.py(), scope, value, self.isolate_id, 0)?;

        Ok(Value::from_local(
            scope,
            value,
            context.clone(),
            isolate.clone(),
            self.isolate_id,
        ))
    }

    /// Compile and run JavaScript source immediately.
    #[pyo3(signature = (source, timeout_ms=None))]
    fn eval(&mut self, source: &str, timeout_ms: Option<u64>) -> PyResult<Value> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        let mut isolate_ref = isolate.borrow_mut();
        let timeout =
            super::runtime::ExecutionTimeout::arm((**isolate_ref).thread_safe_handle(), timeout_ms);
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);
        v8::tc_scope!(let scope, &mut **scope);

        let source_string = v8::String::new(scope, source).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create v8::String.")
        })?;
        let script = v8::Script::compile(scope, source_string, None)
            .ok_or_else(|| js_exception(scope, "v8::Script::compile returned None."))?;
        let result = script.run(scope).ok_or_else(|| {
            if scope.has_terminated() {
                scope.cancel_terminate_execution();
                js_timeout()
            } else {
                js_exception(scope, "Script execution failed.")
            }
        })?;
        drop(timeout);

        Ok(Value::from_local(
            scope,
            result,
            context.clone(),
            isolate.clone(),
            self.isolate_id,
        ))
    }

    /// Set a global property on this context.
    fn set_global(
        &mut self,
        name: &str,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<bool> {
        self.set_global_value(name, value)
    }

    /// Read a global property from this context.
    fn get_global(&mut self, name: &str) -> PyResult<Value> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);
        v8::tc_scope!(let scope, &mut **scope);

        let key = v8::String::new(scope, name)
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Failed to create key."))?;
        let global = local_context.global(scope);
        let result = global
            .get(scope, key.into())
            .ok_or_else(|| js_exception(scope, "Failed to get global value."))?;

        Ok(Value::from_local(
            scope,
            result,
            context.clone(),
            isolate.clone(),
            self.isolate_id,
        ))
    }

    /// Parse JSON source into a JavaScript value.
    fn parse_json(&mut self, source: &str) -> PyResult<Value> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);
        v8::tc_scope!(let scope, &mut **scope);

        let source = v8::String::new(scope, source).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create v8::String.")
        })?;
        let result =
            v8::json::parse(scope, source).ok_or_else(|| js_exception(scope, "Invalid JSON."))?;

        Ok(Value::from_local(
            scope,
            result,
            context.clone(),
            isolate.clone(),
            self.isolate_id,
        ))
    }

    /// Serialize a JavaScript-compatible value using V8's structured clone format.
    fn serialize<'py>(
        &mut self,
        py: Python<'py>,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        super::structured_clone::serialize(py, isolate, self.isolate_id, context, value)
    }

    /// Deserialize bytes produced by serialize into a JavaScript value.
    fn deserialize(
        &mut self,
        #[gen_stub(override_type(type_repr = "bytes | bytearray | memoryview", imports = ()))]
        data: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        super::structured_clone::deserialize(data, isolate, self.isolate_id, context)
    }

    /// Run V8's microtask checkpoint for this isolate.
    fn perform_microtask_checkpoint(&mut self) -> PyResult<()> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        isolate.borrow_mut().perform_microtask_checkpoint();
        Ok(())
    }

    /// Run at most one queued host task or timer.
    #[pyo3(signature = (timeout_ms=None))]
    fn run_event_loop_once(&mut self, timeout_ms: Option<u64>) -> PyResult<bool> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let timeout = timeout_ms.map(std::time::Duration::from_millis);

        super::event_loop::run_event_loop_once(isolate, self.isolate_id, context, timeout)
    }

    /// Run queued host tasks until the queue is idle or max_tasks is reached.
    #[pyo3(signature = (max_tasks=None))]
    fn run_until_idle(&mut self, max_tasks: Option<usize>) -> PyResult<usize> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        super::event_loop::run_until_idle(isolate, self.isolate_id, context, max_tasks)
    }

    /// Set the microtask policy to "auto" or "explicit" for this context's isolate.
    fn set_microtasks_policy(&mut self, policy: &str) -> PyResult<()> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let policy = Self::parse_microtasks_policy(policy)?;

        isolate.borrow_mut().set_microtasks_policy(policy);
        Ok(())
    }

    /// Request termination of currently running JavaScript execution.
    fn terminate_execution(&mut self) -> PyResult<bool> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        Ok(isolate.borrow().terminate_execution())
    }

    /// Cancel a pending termination request for JavaScript execution.
    fn cancel_terminate_execution(&mut self) -> PyResult<bool> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        Ok(isolate.borrow().cancel_terminate_execution())
    }

    /// Compile an ECMAScript module with the given specifier.
    #[pyo3(signature = (source, specifier="<module>"))]
    fn compile_module(&mut self, source: &str, specifier: &str) -> PyResult<Module> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);
        v8::tc_scope!(let scope, &mut **scope);

        let source_string = v8::String::new(scope, source).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create v8::String.")
        })?;
        let resource_name = v8::String::new(scope, specifier).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create module resource name.")
        })?;
        let origin = v8::ScriptOrigin::new(
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
        );
        let mut source = v8::script_compiler::Source::new(source_string, Some(&origin));
        let module = v8::script_compiler::compile_module(scope, &mut source)
            .ok_or_else(|| js_exception(scope, "Failed to compile module."))?;
        host_apis::module_loader::register_module(scope, local_context, specifier, module);

        Ok(Module {
            module: v8::Global::new(scope, module),
            context: context.clone(),
            isolate: isolate.clone(),
            isolate_id: self.isolate_id,
            specifier: specifier.to_owned(),
            dependencies: Vec::new(),
        })
    }

    /// Compile JavaScript source as a function object.
    #[pyo3(signature = (source, arguments=None, *, filename="<function>", source_map_url=None, cached_data=None))]
    fn compile_function(
        &mut self,
        #[gen_stub(override_type(type_repr = "str | String", imports = ()))] source: &Bound<
            '_,
            PyAny,
        >,
        #[gen_stub(override_type(type_repr = "collections.abc.Sequence[str] | None", imports = ("collections.abc")))]
        arguments: Option<&Bound<'_, PyAny>>,
        filename: &str,
        source_map_url: Option<&str>,
        #[gen_stub(override_type(type_repr = "bytes | bytearray | memoryview | None", imports = ()))]
        cached_data: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<V8Function> {
        let source_text = compile_source_text(source, self.isolate_id)?;
        let arguments = compile_function_arguments(arguments)?;
        let cached_data = cached_data_bytes(source.py(), cached_data)?;
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let script_id = super::runtime::next_script_id();

        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);
        v8::tc_scope!(let scope, &mut **scope);

        let source_code = v8::String::new(scope, &source_text).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create function source.")
        })?;
        let resource_name = v8::String::new(scope, filename).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create function resource name.")
        })?;
        let source_map_url_value = if let Some(source_map_url) = source_map_url {
            Some(
                v8::String::new(scope, source_map_url)
                    .map(|value| value.into())
                    .ok_or_else(|| {
                        pyo3::exceptions::PyRuntimeError::new_err(
                            "Failed to create function source map URL.",
                        )
                    })?,
            )
        } else {
            None
        };
        let origin = v8::ScriptOrigin::new(
            scope,
            resource_name.into(),
            0,
            0,
            false,
            script_id,
            source_map_url_value,
            false,
            false,
            false,
            None,
        );
        let mut source = if let Some(cached_data) = cached_data.as_ref() {
            v8::script_compiler::Source::new_with_cached_data(
                source_code,
                Some(&origin),
                v8::CachedData::new(cached_data),
            )
        } else {
            v8::script_compiler::Source::new(source_code, Some(&origin))
        };
        let compile_options = if cached_data.is_some() {
            v8::script_compiler::CompileOptions::ConsumeCodeCache
        } else {
            v8::script_compiler::CompileOptions::NoCompileOptions
        };
        let argument_names = arguments
            .iter()
            .map(|argument| {
                v8::String::new(scope, argument).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err(
                        "Failed to create function argument name.",
                    )
                })
            })
            .collect::<PyResult<Vec<_>>>()?;
        let function = v8::script_compiler::compile_function(
            scope,
            &mut source,
            &argument_names,
            &[],
            compile_options,
            v8::script_compiler::NoCacheReason::NoReason,
        )
        .ok_or_else(|| js_exception(scope, "Failed to compile function."))?;
        let cached_data_rejected = source
            .get_cached_data()
            .map(|cached_data| cached_data.rejected())
            .unwrap_or(false);
        let handle = V8Value::from_local(
            scope,
            function.into(),
            context.clone(),
            isolate.clone(),
            self.isolate_id,
        );

        Ok(V8Function::from_compiled_local(
            scope,
            function,
            handle,
            cached_data_rejected,
        ))
    }

    /// Compile JavaScript source as a reusable script.
    #[pyo3(signature = (source, *, filename="<script>", source_map_url=None, cached_data=None))]
    fn compile(
        &mut self,
        #[gen_stub(override_type(type_repr = "str | String", imports = ()))] source: &Bound<
            '_,
            PyAny,
        >,
        filename: &str,
        source_map_url: Option<&str>,
        #[gen_stub(override_type(type_repr = "bytes | bytearray | memoryview | None", imports = ()))]
        cached_data: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Script> {
        let source_text = compile_source_text(source, self.isolate_id)?;
        let cached_data = cached_data_bytes(source.py(), cached_data)?;
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;
        let context = self.context.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive.")
        })?;

        let script_id = super::runtime::next_script_id();

        let (script, unbound_script, cached_data_rejected, source_url, source_mapping_url) = {
            let mut isolate_ref = isolate.borrow_mut();
            let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
            let scope = &mut scope.init();
            let local_context = v8::Local::new(scope, context);
            let scope = &mut v8::ContextScope::new(scope, local_context);
            v8::tc_scope!(let scope, &mut **scope);

            let source_code = v8::String::new(scope, &source_text).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to create script source.")
            })?;
            let resource_name = v8::String::new(scope, filename).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to create script resource name.")
            })?;
            let source_map_url_value = if let Some(source_map_url) = source_map_url {
                Some(
                    v8::String::new(scope, source_map_url)
                        .map(|value| value.into())
                        .ok_or_else(|| {
                            pyo3::exceptions::PyRuntimeError::new_err(
                                "Failed to create script source map URL.",
                            )
                        })?,
                )
            } else {
                None
            };
            let origin = v8::ScriptOrigin::new(
                scope,
                resource_name.into(),
                0,
                0,
                false,
                script_id,
                source_map_url_value,
                false,
                false,
                false,
                None,
            );
            let mut source = if let Some(cached_data) = cached_data.as_ref() {
                v8::script_compiler::Source::new_with_cached_data(
                    source_code,
                    Some(&origin),
                    v8::CachedData::new(cached_data),
                )
            } else {
                v8::script_compiler::Source::new(source_code, Some(&origin))
            };
            let compile_options = if cached_data.is_some() {
                v8::script_compiler::CompileOptions::ConsumeCodeCache
            } else {
                v8::script_compiler::CompileOptions::NoCompileOptions
            };
            let unbound_script = v8::script_compiler::compile_unbound_script(
                scope,
                &mut source,
                compile_options,
                v8::script_compiler::NoCacheReason::NoReason,
            )
            .ok_or_else(|| js_exception(scope, "Failed to compile script."))?;
            let cached_data_rejected = source
                .get_cached_data()
                .map(|cached_data| cached_data.rejected())
                .unwrap_or(false);
            let script = unbound_script.bind_to_current_context(scope);
            let source_url = super::script::optional_value_to_string(
                scope,
                unbound_script.get_source_url(scope),
            );
            let source_mapping_url = super::script::optional_value_to_string(
                scope,
                unbound_script.get_source_mapping_url(scope),
            );

            (
                v8::Global::new(scope, script),
                v8::Global::new(scope, unbound_script),
                cached_data_rejected,
                source_url,
                source_mapping_url,
            )
        };

        Ok(Script {
            script,
            unbound_script,
            context: context.clone(),
            isolate: isolate.clone(),
            isolate_id: self.isolate_id,
            source: source_text,
            script_id,
            resource_name: Some(filename.to_owned()),
            source_map_url: source_map_url.map(str::to_owned),
            source_url,
            source_mapping_url,
            cached_data_rejected,
        })
    }
}
