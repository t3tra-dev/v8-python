use std::cell::RefCell;
use std::rc::Rc;

use pyo3::prelude::{Bound, Py, PyAny, PyModule, PyResult, Python, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyModuleMethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::context::Context;
use crate::host_apis::{HostAPI, HostAPIDefinition};
use crate::runtime::SharedIsolate;

const DEFAULT_CONTEXT_GROUP_ID: i32 = 1;
const DEFAULT_AUX_DATA: &str = "{\"isDefault\":true}";

pub(crate) struct InspectorDefinition {
    name: String,
    context_group_id: i32,
    aux_data: Option<String>,
}

/// Installs V8 Inspector support for a context.
#[gen_stub_pyclass]
#[pyclass(extends = HostAPI, module = "v8.api", name = "Inspector")]
pub(crate) struct InspectorAPI {
    name: String,
    context_group_id: i32,
    aux_data: Option<String>,
}

/// V8 Inspector endpoint for a context.
#[gen_stub_pyclass]
#[pyclass(name = "Inspector", unsendable)]
pub(crate) struct Inspector {
    runtime: Rc<InspectorRuntime>,
}

/// Connected V8 Inspector protocol session.
#[gen_stub_pyclass]
#[pyclass(name = "InspectorSession", unsendable)]
pub(crate) struct InspectorSession {
    session: v8::inspector::V8InspectorSession,
    messages: InspectorMessages,
    runtime: Rc<InspectorRuntime>,
}

struct InspectorRuntime {
    inspector: v8::inspector::V8Inspector,
    isolate: SharedIsolate,
    context: v8::Global<v8::Context>,
    context_group_id: i32,
    name: String,
}

#[derive(Clone)]
struct InspectorMessages {
    queue: Rc<RefCell<Vec<String>>>,
    callback: Rc<Option<Py<PyAny>>>,
}

struct PythonInspectorClient;

struct PythonInspectorChannel {
    messages: InspectorMessages,
}

impl InspectorDefinition {
    pub(crate) fn clone_ref(&self) -> Self {
        Self {
            name: self.name.clone(),
            context_group_id: self.context_group_id,
            aux_data: self.aux_data.clone(),
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl InspectorAPI {
    /// Create an Inspector HostAPI configuration.
    #[gen_stub(override_return_type(type_repr = "Inspector", imports = ()))]
    #[new]
    #[pyo3(signature = (name = "v8-python", *, context_group_id = DEFAULT_CONTEXT_GROUP_ID, aux_data = None))]
    fn new(
        name: &str,
        context_group_id: i32,
        aux_data: Option<String>,
    ) -> PyResult<pyo3::PyClassInitializer<Self>> {
        if name.trim().is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Inspector context name cannot be empty.",
            ));
        }

        Ok(pyo3::PyClassInitializer::from(HostAPI).add_subclass(Self {
            name: name.to_owned(),
            context_group_id,
            aux_data,
        }))
    }

    /// Return the inspector context name.
    #[getter]
    fn name(&self) -> &str {
        &self.name
    }

    /// Return the inspector context group id.
    #[getter]
    fn context_group_id(&self) -> i32 {
        self.context_group_id
    }

    /// Return auxiliary inspector context data.
    #[getter]
    fn aux_data(&self) -> Option<&str> {
        self.aux_data.as_deref()
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl Inspector {
    /// Return the inspector context name.
    #[getter]
    fn name(&self) -> &str {
        &self.runtime.name
    }

    /// Return the inspector context group id.
    #[getter]
    fn context_group_id(&self) -> i32 {
        self.runtime.context_group_id
    }

    /// Connect a protocol session to this inspector.
    #[pyo3(signature = (state = "", *, trusted = true, on_message = None))]
    fn connect(
        &self,
        state: &str,
        trusted: bool,
        #[gen_stub(override_type(type_repr = "collections.abc.Callable[[str], object] | None", imports = ("collections.abc",)))]
        on_message: Option<Py<PyAny>>,
    ) -> InspectorSession {
        let messages = InspectorMessages::new(on_message);
        let channel = v8::inspector::Channel::new(Box::new(PythonInspectorChannel {
            messages: messages.clone(),
        }));
        let trust_level = if trusted {
            v8::inspector::V8InspectorClientTrustLevel::FullyTrusted
        } else {
            v8::inspector::V8InspectorClientTrustLevel::Untrusted
        };
        let session = self.runtime.inspector.connect(
            self.runtime.context_group_id,
            channel,
            v8::inspector::StringView::from(state.as_bytes()),
            trust_level,
        );

        InspectorSession {
            session,
            messages,
            runtime: self.runtime.clone(),
        }
    }

    /// Return whether this inspector still owns an active V8 inspector.
    fn is_alive(&self) -> bool {
        let _context = &self.runtime.context;

        Rc::strong_count(&self.runtime.isolate) > 0
    }

    /// Return a debug representation.
    fn __repr__(&self) -> String {
        format!(
            "v8.Inspector(name={:?}, context_group_id={})",
            self.runtime.name, self.runtime.context_group_id
        )
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl InspectorSession {
    /// Return whether an inspector protocol method can be dispatched.
    #[staticmethod]
    fn can_dispatch_method(method: &str) -> bool {
        v8::inspector::V8InspectorSession::can_dispatch_method(v8::inspector::StringView::from(
            method.as_bytes(),
        ))
    }

    /// Send an inspector protocol request or notification.
    #[pyo3(name = "send")]
    #[gen_stub(override_return_type(type_repr = "None", imports = ()))]
    fn send_py(
        &self,
        py: Python<'_>,
        #[gen_stub(override_type(type_repr = "str | dict[str, object]", imports = ()))]
        message: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.dispatch(&protocol_message_string(py, message)?);
        Ok(())
    }

    /// Dispatch a raw inspector protocol message.
    fn dispatch(&self, message: &str) {
        self.session
            .dispatch_protocol_message(v8::inspector::StringView::from(message.as_bytes()));
    }

    /// Schedule a debugger pause on the next JavaScript statement.
    fn schedule_pause_on_next_statement(&self, reason: &str, detail: &str) {
        self.session.schedule_pause_on_next_statement(
            v8::inspector::StringView::from(reason.as_bytes()),
            v8::inspector::StringView::from(detail.as_bytes()),
        );
    }

    /// Return queued inspector protocol messages.
    #[getter]
    fn messages(&self) -> Vec<String> {
        self.messages.queue.borrow().clone()
    }

    /// Drain and return queued inspector protocol messages.
    fn take_messages(&self) -> Vec<String> {
        std::mem::take(&mut *self.messages.queue.borrow_mut())
    }

    /// Clear queued inspector protocol messages.
    fn clear_messages(&self) {
        self.messages.queue.borrow_mut().clear();
    }

    /// Return the number of queued inspector messages.
    fn __len__(&self) -> usize {
        self.messages.queue.borrow().len()
    }

    /// Return whether the session has queued messages.
    fn __bool__(&self) -> bool {
        !self.messages.queue.borrow().is_empty()
    }

    /// Return a debug representation.
    fn __repr__(&self) -> String {
        format!(
            "v8.InspectorSession(context_group_id={}, queued_messages={})",
            self.runtime.context_group_id,
            self.messages.queue.borrow().len()
        )
    }
}

impl InspectorRuntime {
    fn new(
        inspector: v8::inspector::V8Inspector,
        isolate: SharedIsolate,
        context: v8::Global<v8::Context>,
        definition: &InspectorDefinition,
    ) -> Self {
        Self {
            inspector,
            isolate,
            context,
            context_group_id: definition.context_group_id,
            name: definition.name.clone(),
        }
    }
}

impl InspectorMessages {
    fn new(callback: Option<Py<PyAny>>) -> Self {
        Self {
            queue: Rc::new(RefCell::new(Vec::new())),
            callback: Rc::new(callback),
        }
    }

    fn push(&self, message: String) {
        self.queue.borrow_mut().push(message.clone());

        if let Some(callback) = &*self.callback {
            Python::attach(|py| {
                let _ = callback.bind(py).call1((message,));
            });
        }
    }
}

impl v8::inspector::ChannelImpl for PythonInspectorChannel {
    fn send_response(&self, _call_id: i32, message: v8::UniquePtr<v8::inspector::StringBuffer>) {
        self.messages.push(string_buffer_to_string(message));
    }

    fn send_notification(&self, message: v8::UniquePtr<v8::inspector::StringBuffer>) {
        self.messages.push(string_buffer_to_string(message));
    }

    fn flush_protocol_notifications(&self) {}
}

impl v8::inspector::V8InspectorClientImpl for PythonInspectorClient {}

pub(crate) fn add_class(api_module: &Bound<'_, PyModule>) -> PyResult<()> {
    api_module.add_class::<InspectorAPI>()
}

pub(crate) fn definition_from_python(
    api: &Bound<'_, PyAny>,
) -> PyResult<Option<HostAPIDefinition>> {
    if !api.is_instance_of::<InspectorAPI>() {
        return Ok(None);
    }

    let inspector = api.extract::<pyo3::PyRef<'_, InspectorAPI>>()?;

    Ok(Some(HostAPIDefinition::Inspector(InspectorDefinition {
        name: inspector.name.clone(),
        context_group_id: inspector.context_group_id,
        aux_data: inspector.aux_data.clone(),
    })))
}

pub(crate) fn install(context: &mut Context, definition: &InspectorDefinition) -> PyResult<()> {
    let isolate = context
        .isolate
        .as_ref()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive."))?;
    let context_global = context
        .context
        .as_ref()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive."))?;

    let mut isolate_ref = isolate.borrow_mut();
    let inspector = v8::inspector::V8Inspector::create(
        &mut isolate_ref,
        v8::inspector::V8InspectorClient::new(Box::new(PythonInspectorClient)),
    );
    let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
    let scope = &mut scope.init();
    let local_context = v8::Local::new(scope, context_global);
    let name = definition.name.as_bytes();
    let aux_data = definition
        .aux_data
        .as_deref()
        .unwrap_or(DEFAULT_AUX_DATA)
        .as_bytes();

    inspector.context_created(
        local_context,
        definition.context_group_id,
        v8::inspector::StringView::from(name),
        v8::inspector::StringView::from(aux_data),
    );
    let runtime = Rc::new(InspectorRuntime::new(
        inspector,
        isolate.clone(),
        context_global.clone(),
        definition,
    ));
    local_context.set_slot(runtime);

    Ok(())
}

pub(crate) fn inspector_from_context(context: &Context) -> PyResult<Inspector> {
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
    let runtime = local_context
        .get_slot::<InspectorRuntime>()
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "v8.api.Inspector is not installed for this Context.",
            )
        })?;

    Ok(Inspector { runtime })
}

fn protocol_message_string(py: Python<'_>, message: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(message) = message.extract::<String>() {
        return Ok(message);
    }

    py.import("json")?
        .call_method1("dumps", (message,))?
        .extract()
}

fn string_buffer_to_string(mut message: v8::UniquePtr<v8::inspector::StringBuffer>) -> String {
    message
        .as_mut()
        .map(|message| message.string().to_string())
        .unwrap_or_default()
}
