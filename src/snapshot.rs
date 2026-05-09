use pyo3::prelude::{Bound, PyAny, PyResult, Python, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyBytes};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::error::{js_exception, js_timeout};
use crate::runtime;
use crate::v8value::python_bytes_like_to_vec;

/// Raw V8 startup snapshot data.
#[gen_stub_pyclass]
#[pyclass(name = "StartupData")]
pub(crate) struct StartupData {
    data: Vec<u8>,
}

/// Builder for creating V8 startup snapshots.
#[gen_stub_pyclass]
#[pyclass(name = "SnapshotCreator", unsendable)]
pub(crate) struct SnapshotCreator {
    isolate: Option<v8::OwnedIsolate>,
    default_context: Option<v8::Global<v8::Context>>,
    context_snapshots: Vec<v8::Global<v8::Context>>,
}

#[gen_stub_pymethods]
#[pymethods]
impl StartupData {
    /// Wrap snapshot bytes.
    #[new]
    fn new(
        py: Python<'_>,
        #[gen_stub(override_type(type_repr = "bytes | bytearray | memoryview", imports = ()))]
        data: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            data: bytes_like_to_vec(py, data, "StartupData()")?,
        })
    }

    /// Return the number of bytes in this snapshot.
    #[getter]
    fn byte_length(&self) -> usize {
        self.data.len()
    }

    /// Return whether this snapshot is valid for the linked V8.
    fn is_valid(&self) -> bool {
        runtime::init_v8_once();
        self.startup_data().is_valid()
    }

    /// Return whether V8 can rehash this snapshot for the current build.
    fn can_be_rehashed(&self) -> bool {
        runtime::init_v8_once();
        self.startup_data().can_be_rehashed()
    }

    /// Return this snapshot as bytes.
    fn to_bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.data)
    }

    /// Return this snapshot as bytes.
    fn __bytes__<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        self.to_bytes(py)
    }

    /// Return the snapshot byte length.
    fn __len__(&self) -> usize {
        self.data.len()
    }

    /// Return a debug representation.
    fn __repr__(&self) -> String {
        format!("v8.StartupData(byte_length={})", self.data.len())
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl SnapshotCreator {
    /// Create a snapshot creator, optionally seeded from existing startup data.
    #[new]
    #[pyo3(signature = (existing_snapshot = None))]
    fn new(
        #[gen_stub(override_type(type_repr = "StartupData | bytes | bytearray | memoryview | None", imports = ()))]
        existing_snapshot: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        runtime::init_v8_once();
        let existing_snapshot = existing_snapshot
            .map(startup_data_from_python)
            .transpose()?;
        let mut isolate = if let Some(snapshot) = existing_snapshot {
            if !snapshot.is_valid() {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "existing_snapshot is not valid for this V8 instance.",
                ));
            }

            v8::Isolate::snapshot_creator_from_existing_snapshot(snapshot, None, None)
        } else {
            v8::Isolate::snapshot_creator(None, None)
        };
        isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 50);
        let default_context = create_context(&mut isolate);

        Ok(Self {
            isolate: Some(isolate),
            default_context: Some(default_context),
            context_snapshots: Vec::new(),
        })
    }

    /// Return whether this creator can still produce a snapshot.
    fn is_alive(&self) -> bool {
        self.isolate.is_some()
    }

    /// Execute JavaScript in the default snapshot context.
    #[pyo3(signature = (source, timeout_ms = None))]
    fn eval(&mut self, source: &str, timeout_ms: Option<u64>) -> PyResult<()> {
        let context = self
            .default_context
            .as_ref()
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("SnapshotCreator is no longer alive.")
            })?
            .clone();
        let isolate = self.live_isolate_mut()?;

        eval_in_context(isolate, &context, source, timeout_ms)
    }

    /// Add a context snapshot and return its index.
    #[pyo3(signature = (source = None, *, timeout_ms = None))]
    fn add_context(&mut self, source: Option<&str>, timeout_ms: Option<u64>) -> PyResult<usize> {
        let (index, context_global) = {
            let isolate = self.live_isolate_mut()?;
            let timeout = source.and_then(|_| {
                runtime::ExecutionTimeout::arm(isolate.thread_safe_handle(), timeout_ms)
            });
            let scope = std::pin::pin!(v8::HandleScope::new(isolate));
            let scope = &mut scope.init();
            let context = v8::Context::new(scope, Default::default());
            let scope = &mut v8::ContextScope::new(scope, context);

            if let Some(source) = source {
                run_script(scope, source)?;
            }

            drop(timeout);
            (scope.add_context(context), v8::Global::new(scope, context))
        };
        self.context_snapshots.push(context_global);

        Ok(index)
    }

    /// Finish the snapshot and return startup data.
    #[pyo3(signature = (function_code_handling = "clear"))]
    fn create_blob(
        &mut self,
        #[gen_stub(override_type(
            type_repr = "typing.Literal['clear', 'keep']",
            imports = ("typing")
        ))]
        function_code_handling: &str,
    ) -> PyResult<StartupData> {
        let function_code_handling = parse_function_code_handling(function_code_handling)?;
        let startup_data = self
            .finish(function_code_handling)
            .map_err(|message| pyo3::exceptions::PyRuntimeError::new_err(message.to_owned()))?;

        Ok(StartupData::from_v8(startup_data))
    }

    /// Return a debug representation.
    fn __repr__(&self) -> String {
        format!("v8.SnapshotCreator(alive={})", self.is_alive())
    }
}

impl StartupData {
    fn from_v8(data: v8::StartupData) -> Self {
        Self {
            data: data.to_vec(),
        }
    }

    pub(crate) fn startup_data(&self) -> v8::StartupData {
        v8::StartupData::from(self.data.clone())
    }
}

impl SnapshotCreator {
    fn live_isolate_mut(&mut self) -> PyResult<&mut v8::OwnedIsolate> {
        self.isolate.as_mut().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("SnapshotCreator is no longer alive.")
        })
    }

    fn finish(
        &mut self,
        function_code_handling: v8::FunctionCodeHandling,
    ) -> Result<v8::StartupData, &'static str> {
        let Some(mut isolate) = self.isolate.take() else {
            return Err("SnapshotCreator is no longer alive.");
        };

        if let Some(default_context) = self.default_context.take() {
            {
                let scope = std::pin::pin!(v8::HandleScope::new(&mut isolate));
                let scope = &mut scope.init();
                let context = v8::Local::new(scope, &default_context);
                scope.set_default_context(context);
            }
            drop(default_context);
        }
        self.context_snapshots.clear();

        isolate
            .create_blob(function_code_handling)
            .ok_or("SnapshotCreator failed to create a startup blob.")
    }
}

impl Drop for SnapshotCreator {
    fn drop(&mut self) {
        if self.isolate.is_some() {
            let _ = self.finish(v8::FunctionCodeHandling::Clear);
        }
    }
}

pub(crate) fn startup_data_from_python(value: &Bound<'_, PyAny>) -> PyResult<v8::StartupData> {
    if let Ok(data) = value.extract::<pyo3::PyRef<'_, StartupData>>() {
        return Ok(data.startup_data());
    }

    Ok(v8::StartupData::from(bytes_like_to_vec(
        value.py(),
        value,
        "snapshot",
    )?))
}

fn bytes_like_to_vec(py: Python<'_>, value: &Bound<'_, PyAny>, label: &str) -> PyResult<Vec<u8>> {
    python_bytes_like_to_vec(py, value)?.ok_or_else(|| {
        pyo3::exceptions::PyTypeError::new_err(format!(
            "{label} must be bytes, bytearray, or memoryview."
        ))
    })
}

fn create_context(isolate: &mut v8::OwnedIsolate) -> v8::Global<v8::Context> {
    let scope = std::pin::pin!(v8::HandleScope::new(isolate));
    let scope = &mut scope.init();
    let context = v8::Context::new(scope, Default::default());

    v8::Global::new(scope, context)
}

fn eval_in_context(
    isolate: &mut v8::OwnedIsolate,
    context: &v8::Global<v8::Context>,
    source: &str,
    timeout_ms: Option<u64>,
) -> PyResult<()> {
    let timeout = runtime::ExecutionTimeout::arm(isolate.thread_safe_handle(), timeout_ms);
    let scope = std::pin::pin!(v8::HandleScope::new(isolate));
    let scope = &mut scope.init();
    let context = v8::Local::new(scope, context);
    let scope = &mut v8::ContextScope::new(scope, context);

    run_script(scope, source)?;
    drop(timeout);

    Ok(())
}

fn run_script(
    scope: &mut v8::ContextScope<'_, '_, v8::HandleScope<'_, v8::Context>>,
    source: &str,
) -> PyResult<()> {
    v8::tc_scope!(let scope, &mut **scope);

    let source = v8::String::new(scope, source).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create snapshot source.")
    })?;
    let script = v8::Script::compile(scope, source, None)
        .ok_or_else(|| js_exception(scope, "v8::Script::compile returned None."))?;
    script.run(scope).ok_or_else(|| {
        if scope.has_terminated() {
            scope.cancel_terminate_execution();
            js_timeout()
        } else {
            js_exception(scope, "Snapshot script execution failed.")
        }
    })?;

    Ok(())
}

fn parse_function_code_handling(value: &str) -> PyResult<v8::FunctionCodeHandling> {
    match value {
        "clear" => Ok(v8::FunctionCodeHandling::Clear),
        "keep" => Ok(v8::FunctionCodeHandling::Keep),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "function_code_handling must be 'clear' or 'keep'.",
        )),
    }
}
