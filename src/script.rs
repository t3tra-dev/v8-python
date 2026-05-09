use pyo3::prelude::{Bound, PyResult, Python, pyclass, pymethods};
use pyo3::types::PyBytes;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use super::error::{js_exception, js_timeout};
use super::runtime::ExecutionTimeout;
use super::runtime::SharedIsolate;
use super::v8value::Value;

/// Compiled JavaScript script bound to a context.
#[gen_stub_pyclass]
#[pyclass(unsendable)]
pub(super) struct Script {
    pub(crate) script: v8::Global<v8::Script>,
    pub(crate) unbound_script: v8::Global<v8::UnboundScript>,
    pub(crate) context: v8::Global<v8::Context>,
    pub(crate) isolate: SharedIsolate,
    pub(crate) isolate_id: u64,
    pub(crate) source: String,
    pub(crate) script_id: i32,
    pub(crate) resource_name: Option<String>,
    pub(crate) source_map_url: Option<String>,
    pub(crate) source_url: Option<String>,
    pub(crate) source_mapping_url: Option<String>,
    pub(crate) cached_data_rejected: bool,
}

#[gen_stub_pymethods]
#[pymethods]
impl Script {
    /// Run the compiled script and return its result.
    #[pyo3(signature = (timeout_ms=None))]
    fn run(&mut self, timeout_ms: Option<u64>) -> PyResult<Value> {
        let mut isolate = self.isolate.borrow_mut();
        let timeout = ExecutionTimeout::arm((**isolate).thread_safe_handle(), timeout_ms);
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate));
        let scope = &mut scope.init();
        let context = v8::Local::new(scope, &self.context);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(let scope, &mut **scope);
        let script = v8::Local::new(scope, &self.script);

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
            self.context.clone(),
            self.isolate.clone(),
            self.isolate_id,
        ))
    }

    /// Return the source text used to compile this script.
    #[getter]
    fn source(&self) -> String {
        self.source.clone()
    }

    /// Return V8's script id for this script.
    #[getter]
    fn script_id(&self) -> i32 {
        self.script_id
    }

    /// Return the resource name supplied at compile time.
    #[getter]
    fn resource_name(&self) -> Option<String> {
        self.resource_name.clone()
    }

    /// Return the source map URL supplied at compile time.
    #[getter]
    fn source_map_url(&self) -> Option<String> {
        self.source_map_url.clone()
    }

    /// Return the source URL discovered by V8.
    #[getter]
    fn source_url(&self) -> Option<String> {
        self.source_url.clone()
    }

    /// Return the source mapping URL discovered by V8.
    #[getter]
    fn source_mapping_url(&self) -> Option<String> {
        self.source_mapping_url.clone()
    }

    /// Return whether supplied cached data was rejected by V8.
    #[getter]
    fn cached_data_rejected(&self) -> bool {
        self.cached_data_rejected
    }

    /// Create V8 code-cache bytes for this script.
    fn create_code_cache<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let mut isolate = self.isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate));
        let scope = &mut scope.init();
        let unbound_script = v8::Local::new(scope, &self.unbound_script);
        let cached_data = unbound_script.create_code_cache().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "V8 could not create code cache for this script.",
            )
        })?;

        Ok(PyBytes::new(py, &cached_data))
    }
}

pub(crate) fn optional_value_to_string(
    scope: &v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
) -> Option<String> {
    if value.is_null_or_undefined() {
        return None;
    }

    value
        .to_string(scope)
        .map(|value| value.to_rust_string_lossy(scope))
}
