use std::rc::Rc;

use pyo3::IntoPyObjectExt;
use pyo3::PyClassInitializer;
use pyo3::exceptions::PyRuntimeWarning;
use pyo3::prelude::{Bound, Py, PyAny, PyModule, PyResult, Python, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyModuleMethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::context::Context;
use crate::host_apis::{HostAPI, HostAPIDefinition};

#[derive(Clone, Copy)]
enum RejectionPolicy {
    Ignore,
    Warn,
}

pub(crate) struct PromiseRejectionTrackerDefinition {
    policy: RejectionPolicy,
    callback: Option<Py<PyAny>>,
}

/// Installs V8 promise rejection tracking with an optional Python callback.
#[gen_stub_pyclass]
#[pyclass(
    extends = HostAPI,
    module = "v8.api",
    name = "PromiseRejectionTracker"
)]
pub(crate) struct PromiseRejectionTrackerAPI {
    policy: RejectionPolicy,
    callback: Option<Py<PyAny>>,
}

struct PromiseRejectionTrackerRuntime {
    policy: RejectionPolicy,
    callback: Option<Py<PyAny>>,
}

impl PromiseRejectionTrackerDefinition {
    pub(crate) fn clone_ref(&self, py: Python<'_>) -> Self {
        Self {
            policy: self.policy,
            callback: self
                .callback
                .as_ref()
                .map(|callback| callback.clone_ref(py)),
        }
    }
}

impl PromiseRejectionTrackerRuntime {
    fn new(py: Python<'_>, definition: &PromiseRejectionTrackerDefinition) -> Self {
        Self {
            policy: definition.policy,
            callback: definition
                .callback
                .as_ref()
                .map(|callback| callback.clone_ref(py)),
        }
    }

    fn handle_event(&self, py: Python<'_>, event: &str, reason: Option<&str>) -> PyResult<()> {
        if let Some(callback) = &self.callback {
            let callback = callback.bind(py);
            let reason = match reason {
                Some(reason) => reason.into_py_any(py)?,
                None => py.None(),
            };
            callback.call1((event, reason))?;
        }

        if matches!(self.policy, RejectionPolicy::Warn) && event == "reject_with_no_handler" {
            let message = match reason {
                Some(reason) => format!("Unhandled JavaScript Promise rejection: {reason}"),
                None => "Unhandled JavaScript Promise rejection.".to_owned(),
            };
            let warnings = py.import("warnings")?;
            let category = py.get_type::<PyRuntimeWarning>();
            warnings.call_method1("warn", (message, category, 2))?;
        }

        Ok(())
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PromiseRejectionTrackerAPI {
    /// Create a promise rejection tracker HostAPI.
    #[gen_stub(override_return_type(type_repr = "PromiseRejectionTracker", imports = ()))]
    #[new]
    #[pyo3(signature = (policy = "warn", *, callback = None))]
    fn new(
        #[gen_stub(override_type(type_repr = "typing.Literal['ignore', 'warn']", imports = ("typing")))]
        policy: &str,
        #[gen_stub(override_type(
            type_repr = "collections.abc.Callable[[str, str | None], object] | None",
            imports = ("collections.abc",)
        ))]
        callback: Option<Py<PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let policy = RejectionPolicy::parse(policy)?;
        validate_callback(callback.as_ref())?;

        Ok(PyClassInitializer::from(HostAPI).add_subclass(Self { policy, callback }))
    }
}

impl RejectionPolicy {
    fn parse(policy: &str) -> PyResult<Self> {
        match policy {
            "ignore" => Ok(Self::Ignore),
            "warn" => Ok(Self::Warn),
            _ => Err(pyo3::exceptions::PyValueError::new_err(
                "PromiseRejectionTracker policy must be 'ignore' or 'warn'.",
            )),
        }
    }
}

fn validate_callback(callback: Option<&Py<PyAny>>) -> PyResult<()> {
    let Some(callback) = callback else {
        return Ok(());
    };

    Python::attach(|py| {
        if callback.bind(py).is_callable() {
            Ok(())
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err(
                "PromiseRejectionTracker callback must be callable.",
            ))
        }
    })
}

pub(crate) fn add_class(api_module: &Bound<'_, PyModule>) -> PyResult<()> {
    api_module.add_class::<PromiseRejectionTrackerAPI>()
}

pub(crate) fn definition_from_python(
    api: &Bound<'_, PyAny>,
) -> PyResult<Option<HostAPIDefinition>> {
    if !api.is_instance_of::<PromiseRejectionTrackerAPI>() {
        return Ok(None);
    }

    let tracker = api.extract::<pyo3::PyRef<'_, PromiseRejectionTrackerAPI>>()?;
    let py = api.py();

    Ok(Some(HostAPIDefinition::PromiseRejectionTracker(
        PromiseRejectionTrackerDefinition {
            policy: tracker.policy,
            callback: tracker
                .callback
                .as_ref()
                .map(|callback| callback.clone_ref(py)),
        },
    )))
}

pub(crate) fn install(
    py: Python<'_>,
    context: &mut Context,
    definition: &PromiseRejectionTrackerDefinition,
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
    isolate_ref.set_promise_reject_callback(promise_reject_callback);

    let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
    let scope = &mut scope.init();
    let local_context = v8::Local::new(scope, context_global);
    local_context.set_slot(Rc::new(PromiseRejectionTrackerRuntime::new(py, definition)));

    Ok(())
}

extern "C" fn promise_reject_callback(message: v8::PromiseRejectMessage) {
    v8::callback_scope!(unsafe scope, &message);
    let context = scope.get_current_context();
    let Some(runtime) = context.get_slot::<PromiseRejectionTrackerRuntime>() else {
        return;
    };

    let event = event_name(message.get_event());
    let reason = message
        .get_value()
        .map(|value| value_to_string(scope, value));
    let result = Python::attach(|py| runtime.handle_event(py, event, reason.as_deref()));

    if let Err(err) = result {
        Python::attach(|py| err.write_unraisable(py, None));
    }
}

fn event_name(event: v8::PromiseRejectEvent) -> &'static str {
    match event {
        v8::PromiseRejectEvent::PromiseRejectWithNoHandler => "reject_with_no_handler",
        v8::PromiseRejectEvent::PromiseHandlerAddedAfterReject => "handler_added_after_reject",
        v8::PromiseRejectEvent::PromiseRejectAfterResolved => "reject_after_resolved",
        v8::PromiseRejectEvent::PromiseResolveAfterResolved => "resolve_after_resolved",
    }
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
        .unwrap_or_else(|| "<exception>".to_owned())
}
