use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use pyo3::prelude::{Bound, PyAny, PyRef, PyResult, Python, pyclass, pymethods};
use pyo3::types::PyBytes;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use super::convert::python_bytes_like_to_vec;
use super::handle::V8Value;
use super::value::Value;

#[derive(Clone)]
pub(crate) struct WasmModuleCacheHandle {
    inner: Rc<RefCell<WasmModuleCacheInner>>,
}

#[derive(Default)]
struct WasmModuleCacheInner {
    modules: HashMap<Vec<u8>, v8::CompiledWasmModule>,
    hits: u64,
    misses: u64,
    stores: u64,
}

/// Cache of compiled WebAssembly modules keyed by their wire bytes.
#[gen_stub_pyclass]
#[pyclass(name = "WasmModuleCache", unsendable)]
pub(crate) struct WasmModuleCache {
    handle: WasmModuleCacheHandle,
}

/// JavaScript WebAssembly.Module object wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "WasmModule", unsendable)]
pub(crate) struct V8WasmModule {
    module: v8::Global<v8::WasmModuleObject>,
    handle: V8Value,
}

/// Reusable compiled WebAssembly module handle.
#[gen_stub_pyclass]
#[pyclass(name = "CompiledWasmModule", unsendable)]
pub(crate) struct V8CompiledWasmModule {
    module: v8::CompiledWasmModule,
}

impl WasmModuleCacheHandle {
    fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(WasmModuleCacheInner::default())),
        }
    }

    pub(crate) fn compile_or_get<'s>(
        &self,
        scope: &v8::PinScope<'s, '_>,
        wire_bytes: &[u8],
    ) -> PyResult<v8::Local<'s, v8::WasmModuleObject>> {
        {
            let mut inner = self.inner.borrow_mut();

            if let Some(compiled) = inner.modules.get(wire_bytes)
                && let Some(module) = v8::WasmModuleObject::from_compiled_module(scope, compiled)
            {
                inner.hits += 1;
                return Ok(module);
            }

            inner.misses += 1;
        }

        let module = compile_wasm_module(scope, wire_bytes)?;
        self.insert_local_module(module);
        Ok(module)
    }

    pub(crate) fn insert_local_module(&self, module: v8::Local<'_, v8::WasmModuleObject>) {
        let compiled = module.get_compiled_module();
        let key = compiled.get_wire_bytes_ref().to_vec();
        let mut inner = self.inner.borrow_mut();

        inner.modules.insert(key, compiled);
        inner.stores += 1;
    }

    fn len(&self) -> usize {
        self.inner.borrow().modules.len()
    }

    fn is_empty(&self) -> bool {
        self.inner.borrow().modules.is_empty()
    }

    fn clear(&self) {
        self.inner.borrow_mut().modules.clear();
    }

    fn contains(&self, wire_bytes: &[u8]) -> bool {
        self.inner.borrow().modules.contains_key(wire_bytes)
    }

    fn hits(&self) -> u64 {
        self.inner.borrow().hits
    }

    fn misses(&self) -> u64 {
        self.inner.borrow().misses
    }

    fn stores(&self) -> u64 {
        self.inner.borrow().stores
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl WasmModuleCache {
    /// Create an empty WebAssembly module cache.
    #[new]
    fn new() -> Self {
        Self {
            handle: WasmModuleCacheHandle::new(),
        }
    }

    /// Remove all cached modules while keeping hit and miss counters.
    fn clear(&self) {
        self.handle.clear();
    }

    /// Return whether the cache contains a module for the given wire bytes.
    fn contains(
        &self,
        py: Python<'_>,
        #[gen_stub(override_type(type_repr = "bytes | bytearray | memoryview", imports = ()))]
        wire_bytes: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        Ok(self
            .handle
            .contains(&wasm_bytes_from_python(py, wire_bytes)?))
    }

    /// Insert a compiled module from an existing WasmModule object.
    fn insert(&self, module: PyRef<'_, V8WasmModule>) -> PyResult<()> {
        module.with_local(|_, module| {
            self.handle.insert_local_module(module);
            Ok(())
        })
    }

    /// Return the number of cache hits.
    #[getter]
    fn hits(&self) -> u64 {
        self.handle.hits()
    }

    /// Return the number of cache misses.
    #[getter]
    fn misses(&self) -> u64 {
        self.handle.misses()
    }

    /// Return the number of modules stored in the cache.
    #[getter]
    fn stores(&self) -> u64 {
        self.handle.stores()
    }

    /// Return the number of cached modules.
    fn __len__(&self) -> usize {
        self.handle.len()
    }

    /// Return whether this cache contains any modules.
    fn __bool__(&self) -> bool {
        !self.handle.is_empty()
    }

    /// Return whether the cache contains a module for the given bytes-like key.
    fn __contains__(
        &self,
        py: Python<'_>,
        #[gen_stub(override_type(type_repr = "object", imports = ()))] wire_bytes: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<bool> {
        let Some(wire_bytes) = python_bytes_like_to_vec(py, wire_bytes)? else {
            return Ok(false);
        };

        Ok(self.handle.contains(&wire_bytes))
    }

    /// Return a debug representation.
    fn __repr__(&self) -> String {
        format!(
            "v8.WasmModuleCache(size={}, hits={}, misses={}, stores={})",
            self.handle.len(),
            self.handle.hits(),
            self.handle.misses(),
            self.handle.stores()
        )
    }
}

impl WasmModuleCache {
    pub(crate) fn handle(&self) -> WasmModuleCacheHandle {
        self.handle.clone()
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8WasmModule {
    /// Return this module as a generic V8 Value.
    fn to_value(&self) -> Value {
        Value::from_handle(self.handle.clone())
    }

    /// Return the reusable compiled module handle.
    fn get_compiled_module(&self) -> PyResult<V8CompiledWasmModule> {
        self.with_local(|_, module| {
            Ok(V8CompiledWasmModule {
                module: module.get_compiled_module(),
            })
        })
    }

    /// Return this module's wire bytes.
    #[getter]
    fn wire_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_local(|_, module| {
            let compiled = module.get_compiled_module();
            Ok(PyBytes::new(py, compiled.get_wire_bytes_ref()))
        })
    }

    /// Return this module's source URL metadata.
    #[getter]
    fn source_url(&self) -> PyResult<String> {
        self.with_local(|_, module| Ok(module.get_compiled_module().source_url().to_owned()))
    }

    /// Return a debug representation.
    fn __repr__(&self) -> PyResult<String> {
        self.with_local(|_, module| {
            let compiled = module.get_compiled_module();
            Ok(format!(
                "<v8.WasmModule byte_length={}>",
                compiled.get_wire_bytes_ref().len()
            ))
        })
    }
}

impl V8WasmModule {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        module: v8::Local<'s, v8::WasmModuleObject>,
        handle: V8Value,
    ) -> Self {
        Self {
            module: v8::Global::new(scope, module),
            handle,
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.handle.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        v8::Local::new(scope, &self.handle.value)
    }

    fn with_local<R>(
        &self,
        f: impl for<'s> FnOnce(
            &v8::PinScope<'s, '_>,
            v8::Local<'s, v8::WasmModuleObject>,
        ) -> PyResult<R>,
    ) -> PyResult<R> {
        self.handle.with_local_value(|scope, _| {
            let module = v8::Local::new(scope, &self.module);

            f(scope, module)
        })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8CompiledWasmModule {
    /// Return this compiled module's wire bytes.
    #[getter]
    fn wire_bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, self.module.get_wire_bytes_ref())
    }

    /// Return this compiled module's wire byte length.
    #[getter]
    fn byte_length(&self) -> usize {
        self.module.get_wire_bytes_ref().len()
    }

    /// Return this compiled module's source URL metadata.
    #[getter]
    fn source_url(&self) -> String {
        self.module.source_url().to_owned()
    }

    /// Return a debug representation.
    fn __repr__(&self) -> String {
        format!("<v8.CompiledWasmModule byte_length={}>", self.byte_length())
    }
}

impl V8CompiledWasmModule {
    pub(crate) fn module(&self) -> &v8::CompiledWasmModule {
        &self.module
    }
}

pub(crate) fn wasm_bytes_from_python(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
) -> PyResult<Vec<u8>> {
    python_bytes_like_to_vec(py, value)?.ok_or_else(|| {
        pyo3::exceptions::PyTypeError::new_err(
            "Wasm module bytes must be bytes, bytearray, or memoryview.",
        )
    })
}

pub(crate) fn compile_wasm_module<'s>(
    scope: &v8::PinScope<'s, '_>,
    wire_bytes: &[u8],
) -> PyResult<v8::Local<'s, v8::WasmModuleObject>> {
    v8::WasmModuleObject::compile(scope, wire_bytes).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to compile WebAssembly module.")
    })
}
