use pyo3::prelude::{Bound, Py, PyAny, PyResult, Python, pyclass, pymethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use super::context::{Context, ContextBuilder};
use super::runtime::{SharedIsolate, new_isolate_with_startup_data};
use super::scope::Scope;

/// Owns one V8 isolate until it is consumed to create a context, builder, or scope.
#[gen_stub_pyclass]
#[pyclass(unsendable)]
pub(super) struct Isolate {
    isolate: Option<SharedIsolate>,
    isolate_id: u64,
}

impl Isolate {
    fn live_isolate(&self) -> PyResult<&SharedIsolate> {
        self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "Isolate has already been used to create a Scope, Context, or ContextBuilder, and cannot be reused.",
            )
        })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl Isolate {
    /// Create a new V8 isolate, optionally initialized from snapshot startup data.
    #[new]
    #[pyo3(signature = (snapshot = None))]
    fn new(
        #[gen_stub(override_type(type_repr = "StartupData | bytes | bytearray | memoryview | None", imports = ()))]
        snapshot: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let snapshot = snapshot
            .map(super::snapshot::startup_data_from_python)
            .transpose()?;
        let (isolate_id, isolate) = new_isolate_with_startup_data(snapshot)?;

        Ok(Self {
            isolate: Some(isolate),
            isolate_id,
        })
    }

    /// Consume this isolate and create a fresh execution context.
    fn create_context(&mut self) -> PyResult<Context> {
        let isolate = self.isolate.take().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "Isolate has already been used to create a Scope, Context, or ContextBuilder, and cannot be reused.",
            )
        })?;

        Context::from_isolate(isolate, self.isolate_id)
    }

    /// Consume this isolate and create a context from a snapshot context index.
    #[pyo3(signature = (index = 0))]
    fn create_context_from_snapshot(&mut self, index: usize) -> PyResult<Context> {
        let isolate = self.isolate.take().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "Isolate has already been used to create a Scope, Context, or ContextBuilder, and cannot be reused.",
            )
        })?;

        Context::from_snapshot(isolate, self.isolate_id, index)
    }

    /// Consume this isolate and return a ContextBuilder for configuring globals and HostAPI.
    fn create_context_builder(&mut self) -> PyResult<ContextBuilder> {
        let isolate = self.isolate.take().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "Isolate has already been used to create a Scope, Context, or ContextBuilder, and cannot be reused.",
            )
        })?;

        Ok(ContextBuilder::from_isolate(isolate, self.isolate_id))
    }

    /// Consume this isolate and create a low-level scope for compiling a standalone script.
    fn create_scope(&mut self) -> PyResult<Scope> {
        let isolate = self.isolate.take().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "Isolate has already been used to create a Scope, Context, or ContextBuilder, and cannot be reused.",
            )
        })?;

        Ok(Scope {
            isolate: Some(isolate),
            isolate_id: self.isolate_id,
        })
    }

    /// Return V8 heap statistics for this isolate.
    #[gen_stub(override_return_type(type_repr = "dict[str, int | bool]", imports = ()))]
    fn heap_statistics(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        super::heap::heap_statistics(py, self.live_isolate()?)
    }

    /// Return per-space V8 heap statistics for this isolate.
    #[gen_stub(override_return_type(type_repr = "list[dict[str, int | str]]", imports = ()))]
    fn heap_space_statistics(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        super::heap::heap_space_statistics(py, self.live_isolate()?)
    }

    /// Return V8 heap statistics for generated code and metadata.
    #[gen_stub(override_return_type(type_repr = "dict[str, int]", imports = ()))]
    fn heap_code_statistics(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        super::heap::heap_code_statistics(py, self.live_isolate()?)
    }

    /// Notify V8 that the host is under low-memory pressure.
    fn low_memory_notification(&self) -> PyResult<()> {
        super::heap::low_memory_notification(self.live_isolate()?);
        Ok(())
    }

    /// Set V8's memory pressure level for this isolate.
    fn memory_pressure(
        &self,
        #[gen_stub(override_type(
            type_repr = "typing.Literal['none', 'moderate', 'critical']",
            imports = ("typing")
        ))]
        level: &str,
    ) -> PyResult<()> {
        super::heap::memory_pressure(self.live_isolate()?, level)
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
        super::heap::request_garbage_collection_for_testing(self.live_isolate()?, collection_type)
    }
}
