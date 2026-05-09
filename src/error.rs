use pyo3::PyErr;
use pyo3::exceptions::{PyIndexError, PyRuntimeError};
use pyo3::prelude::{Py, PyResult, Python, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyTuple};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Python exception carrying V8 exception, message, and stack metadata.
#[gen_stub_pyclass]
#[pyclass(skip_from_py_object, extends=PyRuntimeError)]
#[derive(Clone)]
pub(crate) struct JavaScriptError {
    display: String,
    message: String,
    message_info: Option<JavaScriptMessage>,
    details: Option<String>,
    source_line: Option<String>,
    script_resource_name: Option<String>,
    line_number: Option<usize>,
    start_column: usize,
    end_column: usize,
    start_position: Option<i32>,
    end_position: Option<i32>,
    wasm_function_index: Option<i32>,
    error_level: i32,
    is_shared_cross_origin: bool,
    is_opaque: bool,
    stack: Option<String>,
    stack_trace: Option<StackTrace>,
}

#[gen_stub_pymethods]
#[pymethods]
impl JavaScriptError {
    /// Create a JavaScriptError from a message string.
    #[new]
    fn new(message: String) -> Self {
        Self {
            display: message.clone(),
            message,
            message_info: None,
            details: None,
            source_line: None,
            script_resource_name: None,
            line_number: None,
            start_column: 0,
            end_column: 0,
            start_position: None,
            end_position: None,
            wasm_function_index: None,
            error_level: 0,
            is_shared_cross_origin: false,
            is_opaque: false,
            stack: None,
            stack_trace: None,
        }
    }

    /// Return the exception message.
    #[getter]
    fn message(&self) -> String {
        self.message.clone()
    }

    /// Return structured V8 message metadata when available.
    #[getter]
    fn message_info(&self) -> Option<JavaScriptMessage> {
        self.message_info.clone()
    }

    /// Return V8's formatted message details when available.
    #[getter]
    fn details(&self) -> Option<String> {
        self.details.clone()
    }

    /// Return the source line associated with the exception.
    #[getter]
    fn source_line(&self) -> Option<String> {
        self.source_line.clone()
    }

    /// Return the script resource name associated with the exception.
    #[getter]
    fn script_resource_name(&self) -> Option<String> {
        self.script_resource_name.clone()
    }

    /// Return the one-based source line number when V8 reports one.
    #[getter]
    fn line_number(&self) -> Option<usize> {
        self.line_number
    }

    /// Return the zero-based start column.
    #[getter]
    fn start_column(&self) -> usize {
        self.start_column
    }

    /// Return the zero-based end column.
    #[getter]
    fn end_column(&self) -> usize {
        self.end_column
    }

    /// Return the start source position when V8 reports one.
    #[getter]
    fn start_position(&self) -> Option<i32> {
        self.start_position
    }

    /// Return the end source position when V8 reports one.
    #[getter]
    fn end_position(&self) -> Option<i32> {
        self.end_position
    }

    /// Return the WebAssembly function index when the error came from Wasm.
    #[getter]
    fn wasm_function_index(&self) -> Option<i32> {
        self.wasm_function_index
    }

    /// Return V8's message error level.
    #[getter]
    fn error_level(&self) -> i32 {
        self.error_level
    }

    /// Return whether the message is shared cross-origin.
    #[getter]
    fn is_shared_cross_origin(&self) -> bool {
        self.is_shared_cross_origin
    }

    /// Return whether the message is opaque.
    #[getter]
    fn is_opaque(&self) -> bool {
        self.is_opaque
    }

    /// Return the formatted JavaScript stack string.
    #[getter]
    fn stack(&self) -> Option<String> {
        self.stack.clone()
    }

    /// Return structured stack trace frames when available.
    #[getter]
    fn stack_trace(&self) -> Option<StackTrace> {
        self.stack_trace.clone()
    }

    /// Return stack frames, or an empty list when no stack trace is available.
    #[getter]
    fn frames(&self) -> Vec<StackFrame> {
        self.stack_trace
            .as_ref()
            .map(|stack_trace| stack_trace.frames.clone())
            .unwrap_or_default()
    }

    /// Return the formatted exception string.
    fn __str__(&self) -> String {
        self.display.clone()
    }
}

/// V8 message metadata associated with a JavaScript exception.
#[gen_stub_pyclass]
#[pyclass(skip_from_py_object, name = "Message")]
#[derive(Clone)]
pub(crate) struct JavaScriptMessage {
    text: String,
    source_line: Option<String>,
    script_resource_name: Option<String>,
    line_number: Option<usize>,
    start_column: usize,
    end_column: usize,
    start_position: Option<i32>,
    end_position: Option<i32>,
    wasm_function_index: Option<i32>,
    error_level: i32,
    is_shared_cross_origin: bool,
    is_opaque: bool,
}

#[gen_stub_pymethods]
#[pymethods]
impl JavaScriptMessage {
    /// Return V8's message text.
    #[getter]
    fn text(&self) -> String {
        self.text.clone()
    }

    /// Return the source line associated with the message.
    #[getter]
    fn source_line(&self) -> Option<String> {
        self.source_line.clone()
    }

    /// Return the script resource name associated with the message.
    #[getter]
    fn script_resource_name(&self) -> Option<String> {
        self.script_resource_name.clone()
    }

    /// Return the one-based source line number when V8 reports one.
    #[getter]
    fn line_number(&self) -> Option<usize> {
        self.line_number
    }

    /// Return the zero-based start column.
    #[getter]
    fn start_column(&self) -> usize {
        self.start_column
    }

    /// Return the zero-based end column.
    #[getter]
    fn end_column(&self) -> usize {
        self.end_column
    }

    /// Return the start source position when V8 reports one.
    #[getter]
    fn start_position(&self) -> Option<i32> {
        self.start_position
    }

    /// Return the end source position when V8 reports one.
    #[getter]
    fn end_position(&self) -> Option<i32> {
        self.end_position
    }

    /// Return the WebAssembly function index when the message came from Wasm.
    #[getter]
    fn wasm_function_index(&self) -> Option<i32> {
        self.wasm_function_index
    }

    /// Return V8's message error level.
    #[getter]
    fn error_level(&self) -> i32 {
        self.error_level
    }

    /// Return whether the message is shared cross-origin.
    #[getter]
    fn is_shared_cross_origin(&self) -> bool {
        self.is_shared_cross_origin
    }

    /// Return whether the message is opaque.
    #[getter]
    fn is_opaque(&self) -> bool {
        self.is_opaque
    }

    /// Return V8's message text.
    fn __str__(&self) -> String {
        self.text.clone()
    }

    /// Return a debug representation.
    fn __repr__(&self) -> String {
        format!("Message({:?})", self.text)
    }
}

/// Structured JavaScript stack trace.
#[gen_stub_pyclass]
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct StackTrace {
    text: Option<String>,
    frames: Vec<StackFrame>,
}

#[gen_stub_pymethods]
#[pymethods]
impl StackTrace {
    /// Return the formatted stack text when V8 provides one.
    #[getter]
    fn text(&self) -> Option<String> {
        self.text.clone()
    }

    /// Return stack frames.
    #[getter]
    fn frames(&self) -> Vec<StackFrame> {
        self.frames.clone()
    }

    /// Return the number of stack frames.
    fn __len__(&self) -> usize {
        self.frames.len()
    }

    /// Return a stack frame by index.
    fn __getitem__(&self, index: isize) -> PyResult<StackFrame> {
        let index = normalize_index(index, self.frames.len())?;
        Ok(self.frames[index].clone())
    }

    /// Return the formatted stack text, or an empty string.
    fn __str__(&self) -> String {
        self.text.clone().unwrap_or_default()
    }

    /// Return a debug representation.
    fn __repr__(&self) -> String {
        format!("StackTrace(frame_count={})", self.frames.len())
    }
}

/// One frame in a JavaScript stack trace.
#[gen_stub_pyclass]
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct StackFrame {
    line: Option<usize>,
    column: Option<usize>,
    script_id: Option<usize>,
    script_name: Option<String>,
    script_name_or_source_url: Option<String>,
    script_source: Option<String>,
    script_source_mapping_url: Option<String>,
    function_name: Option<String>,
    is_eval: bool,
    is_constructor: bool,
    is_wasm: bool,
    is_user_javascript: bool,
}

#[gen_stub_pymethods]
#[pymethods]
impl StackFrame {
    /// Return the one-based source line.
    #[getter]
    fn line(&self) -> Option<usize> {
        self.line
    }

    /// Return the one-based source line.
    #[getter]
    fn line_number(&self) -> Option<usize> {
        self.line
    }

    /// Return the zero-based source column.
    #[getter]
    fn column(&self) -> Option<usize> {
        self.column
    }

    /// Return V8's script id.
    #[getter]
    fn script_id(&self) -> Option<usize> {
        self.script_id
    }

    /// Return the script name.
    #[getter]
    fn script_name(&self) -> Option<String> {
        self.script_name.clone()
    }

    /// Return the script name or source URL.
    #[getter]
    fn script_name_or_source_url(&self) -> Option<String> {
        self.script_name_or_source_url.clone()
    }

    /// Return the script source when V8 provides it.
    #[getter]
    fn script_source(&self) -> Option<String> {
        self.script_source.clone()
    }

    /// Return the script source mapping URL.
    #[getter]
    fn script_source_mapping_url(&self) -> Option<String> {
        self.script_source_mapping_url.clone()
    }

    /// Return the JavaScript function name.
    #[getter]
    fn function_name(&self) -> Option<String> {
        self.function_name.clone()
    }

    /// Return whether this frame came from eval.
    #[getter]
    fn is_eval(&self) -> bool {
        self.is_eval
    }

    /// Return whether this frame is a constructor call.
    #[getter]
    fn is_constructor(&self) -> bool {
        self.is_constructor
    }

    /// Return whether this frame is WebAssembly.
    #[getter]
    fn is_wasm(&self) -> bool {
        self.is_wasm
    }

    /// Return whether V8 considers this user JavaScript.
    #[getter]
    fn is_user_javascript(&self) -> bool {
        self.is_user_javascript
    }

    /// Return a debug representation.
    fn __repr__(&self) -> String {
        let function_name = self.function_name.as_deref().unwrap_or("<anonymous>");
        let location = self
            .script_name_or_source_url
            .as_deref()
            .or(self.script_name.as_deref())
            .unwrap_or("<unknown>");

        match (self.line, self.column) {
            (Some(line), Some(column)) => {
                format!(
                    "StackFrame(function={function_name:?}, location={location}:{line}:{column})"
                )
            }
            (Some(line), None) => {
                format!("StackFrame(function={function_name:?}, location={location}:{line})")
            }
            _ => format!("StackFrame(function={function_name:?}, location={location})"),
        }
    }
}

pub(crate) fn js_exception(
    try_catch: &mut v8::PinnedRef<'_, v8::TryCatch<v8::HandleScope>>,
    fallback: &str,
) -> PyErr {
    if try_catch.has_terminated() {
        return PyRuntimeError::new_err("JavaScript execution was terminated.");
    }

    let error = JavaScriptError::from_try_catch(try_catch, fallback);
    Python::attach(|py| error.into_py_err(py)).unwrap_or_else(|error| error)
}

pub(crate) fn js_timeout() -> PyErr {
    pyo3::exceptions::PyTimeoutError::new_err("JavaScript execution timed out.")
}

impl JavaScriptError {
    fn from_try_catch(
        try_catch: &mut v8::PinnedRef<'_, v8::TryCatch<v8::HandleScope>>,
        fallback: &str,
    ) -> Self {
        let exception_value = try_catch.exception();
        let exception = exception_value
            .and_then(|exception| exception.to_string(try_catch))
            .map(|exception| exception.to_rust_string_lossy(try_catch))
            .unwrap_or_else(|| fallback.to_owned());
        let message = try_catch.message().or_else(|| {
            exception_value.map(|exception| v8::Exception::create_message(try_catch, exception))
        });
        let details = message.map(|message| message.get(try_catch).to_rust_string_lossy(try_catch));
        let diagnostic = message.map(|message| JavaScriptMessage::from_v8(try_catch, message));
        let stack = try_catch.stack_trace().and_then(|stack| {
            stack
                .to_string(try_catch)
                .map(|stack| stack.to_rust_string_lossy(try_catch))
        });
        let stack_trace = message
            .and_then(|message| message.get_stack_trace(try_catch))
            .or_else(|| {
                exception_value
                    .and_then(|exception| v8::Exception::get_stack_trace(try_catch, exception))
            })
            .map(|stack_trace| StackTrace::from_v8(try_catch, stack_trace, stack.clone()));
        let display = format_js_exception(&exception, diagnostic.as_ref(), stack.as_deref());

        Self {
            display,
            message: exception,
            message_info: diagnostic.clone(),
            details,
            source_line: diagnostic
                .as_ref()
                .and_then(|message| message.source_line.clone()),
            script_resource_name: diagnostic
                .as_ref()
                .and_then(|message| message.script_resource_name.clone()),
            line_number: diagnostic.as_ref().and_then(|message| message.line_number),
            start_column: diagnostic
                .as_ref()
                .map(|message| message.start_column)
                .unwrap_or_default(),
            end_column: diagnostic
                .as_ref()
                .map(|message| message.end_column)
                .unwrap_or_default(),
            start_position: diagnostic
                .as_ref()
                .and_then(|message| message.start_position),
            end_position: diagnostic.as_ref().and_then(|message| message.end_position),
            wasm_function_index: diagnostic
                .as_ref()
                .and_then(|message| message.wasm_function_index),
            error_level: diagnostic
                .as_ref()
                .map(|message| message.error_level)
                .unwrap_or_default(),
            is_shared_cross_origin: diagnostic
                .as_ref()
                .map(|message| message.is_shared_cross_origin)
                .unwrap_or_default(),
            is_opaque: diagnostic
                .as_ref()
                .map(|message| message.is_opaque)
                .unwrap_or_default(),
            stack,
            stack_trace,
        }
    }

    fn into_py_err(self, py: Python<'_>) -> PyResult<PyErr> {
        let display = self.display.clone();
        let error = Py::new(py, self)?;
        let error = error.bind(py).clone();
        let args = PyTuple::new(py, [display])?;

        error.setattr("args", args)?;
        Ok(PyErr::from_value(error.into_any()))
    }
}

impl JavaScriptMessage {
    fn from_v8(
        scope: &mut v8::PinnedRef<'_, v8::TryCatch<v8::HandleScope>>,
        message: v8::Local<'_, v8::Message>,
    ) -> Self {
        Self {
            text: message.get(scope).to_rust_string_lossy(scope),
            source_line: message
                .get_source_line(scope)
                .map(|line| line.to_rust_string_lossy(scope)),
            script_resource_name: value_to_string(scope, message.get_script_resource_name(scope)),
            line_number: message.get_line_number(scope),
            start_column: message.get_start_column(),
            end_column: message.get_end_column(),
            start_position: non_negative_i32(message.get_start_position()),
            end_position: non_negative_i32(message.get_end_position()),
            wasm_function_index: non_negative_i32(message.get_wasm_function_index()),
            error_level: message.error_level(),
            is_shared_cross_origin: message.is_shared_cross_origin(),
            is_opaque: message.is_opaque(),
        }
    }

    fn location(&self) -> Option<String> {
        match (&self.script_resource_name, self.line_number) {
            (Some(resource), Some(line)) => Some(format!("{resource}:{line}")),
            (Some(resource), None) => Some(resource.clone()),
            (None, Some(line)) => Some(format!("<unknown>:{line}")),
            (None, None) => None,
        }
    }
}

impl StackTrace {
    fn from_v8(
        scope: &mut v8::PinnedRef<'_, v8::TryCatch<v8::HandleScope>>,
        stack_trace: v8::Local<'_, v8::StackTrace>,
        text: Option<String>,
    ) -> Self {
        let mut frames = Vec::new();

        for index in 0..stack_trace.get_frame_count() {
            if let Some(frame) = stack_trace.get_frame(scope, index) {
                frames.push(StackFrame::from_v8(scope, frame));
            }
        }

        Self { text, frames }
    }
}

impl StackFrame {
    fn from_v8(
        scope: &mut v8::PinnedRef<'_, v8::TryCatch<v8::HandleScope>>,
        frame: v8::Local<'_, v8::StackFrame>,
    ) -> Self {
        Self {
            line: v8_usize_info(frame.get_line_number()),
            column: v8_usize_info(frame.get_column()),
            script_id: v8_usize_info(frame.get_script_id()),
            script_name: local_string(scope, frame.get_script_name(scope)),
            script_name_or_source_url: local_string(
                scope,
                frame.get_script_name_or_source_url(scope),
            ),
            script_source: local_string(scope, frame.get_script_source(scope)),
            script_source_mapping_url: local_string(
                scope,
                frame.get_script_source_mapping_url(scope),
            ),
            function_name: local_string(scope, frame.get_function_name(scope)),
            is_eval: frame.is_eval(),
            is_constructor: frame.is_constructor(),
            is_wasm: frame.is_wasm(),
            is_user_javascript: frame.is_user_javascript(),
        }
    }
}

fn format_js_exception(
    exception: &str,
    message: Option<&JavaScriptMessage>,
    stack: Option<&str>,
) -> String {
    let mut parts = Vec::new();

    match message.and_then(JavaScriptMessage::location) {
        Some(location) => parts.push(format!("{location}: {exception}")),
        None => parts.push(exception.to_owned()),
    }

    if let Some(source_line) = message.and_then(|message| message.source_line.clone()) {
        parts.push(source_line);
    }

    if let Some(stack) = stack
        && !parts.iter().any(|part| part == stack)
    {
        parts.push(stack.to_owned());
    }

    parts.join("\n")
}

fn value_to_string(
    scope: &mut v8::PinnedRef<'_, v8::TryCatch<v8::HandleScope>>,
    value: Option<v8::Local<'_, v8::Value>>,
) -> Option<String> {
    value
        .and_then(|value| value.to_string(scope))
        .map(|value| value.to_rust_string_lossy(scope))
}

fn local_string(
    scope: &mut v8::PinnedRef<'_, v8::TryCatch<v8::HandleScope>>,
    value: Option<v8::Local<'_, v8::String>>,
) -> Option<String> {
    value.map(|value| value.to_rust_string_lossy(scope))
}

fn non_negative_i32(value: i32) -> Option<i32> {
    if value < 0 { None } else { Some(value) }
}

fn v8_usize_info(value: usize) -> Option<usize> {
    if value == usize::MAX {
        None
    } else {
        Some(value)
    }
}

fn normalize_index(index: isize, length: usize) -> PyResult<usize> {
    let normalized = if index < 0 {
        length.checked_sub(index.unsigned_abs())
    } else {
        usize::try_from(index)
            .ok()
            .and_then(|index| (index < length).then_some(index))
    };

    normalized.ok_or_else(|| PyIndexError::new_err("StackTrace index out of range."))
}
