use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

use pyo3::PyClassInitializer;
use pyo3::prelude::{Bound, Py, PyAny, PyModule, PyResult, Python, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyModuleMethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::context::Context;
use crate::host_apis::{HostAPI, HostAPIDefinition};

const DEFAULT_LABEL: &str = "default";

pub(crate) struct ConsoleDefinition {
    logger: Py<PyAny>,
}

/// Installs console methods that forward to a Python logging.Logger.
#[gen_stub_pyclass]
#[pyclass(extends = HostAPI, module = "v8.api", name = "Console")]
pub(crate) struct ConsoleAPI {
    logger: Py<PyAny>,
}

struct ConsoleRuntime {
    logger: Py<PyAny>,
    counters: RefCell<HashMap<String, u64>>,
    timers: RefCell<HashMap<String, Instant>>,
    group_depth: RefCell<usize>,
}

impl ConsoleDefinition {
    pub(crate) fn clone_ref(&self, py: Python<'_>) -> Self {
        Self {
            logger: self.logger.clone_ref(py),
        }
    }
}

impl ConsoleRuntime {
    fn new(py: Python<'_>, definition: &ConsoleDefinition) -> Self {
        Self {
            logger: definition.logger.clone_ref(py),
            counters: RefCell::new(HashMap::new()),
            timers: RefCell::new(HashMap::new()),
            group_depth: RefCell::new(0),
        }
    }

    fn log(&self, py: Python<'_>, method: &str, message: String) -> PyResult<()> {
        let depth = *self.group_depth.borrow();
        let message = if depth == 0 {
            message
        } else {
            format!("{}{}", "  ".repeat(depth), message)
        };

        self.logger.bind(py).call_method1(method, (message,))?;
        Ok(())
    }

    fn count(&self, py: Python<'_>, label: String) -> PyResult<()> {
        let count = {
            let mut counters = self.counters.borrow_mut();
            let count = counters.entry(label.clone()).or_insert(0);
            *count += 1;
            *count
        };

        self.log(py, "info", format!("{label}: {count}"))
    }

    fn count_reset(&self, py: Python<'_>, label: String) -> PyResult<()> {
        if self.counters.borrow_mut().remove(&label).is_some() {
            return Ok(());
        }

        self.log(
            py,
            "warning",
            format!("Count for '{label}' does not exist."),
        )
    }

    fn time(&self, label: String) {
        self.timers.borrow_mut().insert(label, Instant::now());
    }

    fn time_log(
        &self,
        py: Python<'_>,
        label: String,
        suffix: Option<String>,
        clear: bool,
    ) -> PyResult<()> {
        let started_at = if clear {
            self.timers.borrow_mut().remove(&label)
        } else {
            self.timers.borrow().get(&label).copied()
        };
        let Some(started_at) = started_at else {
            return self.log(py, "warning", format!("Timer '{label}' does not exist."));
        };
        let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
        let mut message = format!("{label}: {elapsed_ms:.3}ms");

        if let Some(suffix) = suffix
            && !suffix.is_empty()
        {
            message.push(' ');
            message.push_str(&suffix);
        }

        self.log(py, "info", message)
    }

    fn group(&self, py: Python<'_>, message: String) -> PyResult<()> {
        if !message.is_empty() {
            self.log(py, "info", message)?;
        }
        *self.group_depth.borrow_mut() += 1;
        Ok(())
    }

    fn group_end(&self) {
        let mut depth = self.group_depth.borrow_mut();

        if *depth > 0 {
            *depth -= 1;
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl ConsoleAPI {
    /// Create a console HostAPI backed by a Python logger.
    #[gen_stub(override_return_type(type_repr = "Console", imports = ()))]
    #[new]
    #[pyo3(signature = (logger = None, *, name = "v8.console"))]
    fn new(
        py: Python<'_>,
        #[gen_stub(override_type(type_repr = "logging.Logger | None", imports = ("logging")))]
        logger: Option<Py<PyAny>>,
        name: &str,
    ) -> PyResult<PyClassInitializer<Self>> {
        let logger = match logger {
            Some(logger) => logger,
            None => py
                .import("logging")?
                .call_method1("getLogger", (name,))?
                .unbind(),
        };
        validate_logger(logger.bind(py))?;

        Ok(PyClassInitializer::from(HostAPI).add_subclass(Self { logger }))
    }

    /// Return the Python logger used by installed console methods.
    #[getter]
    #[gen_stub(override_return_type(type_repr = "logging.Logger", imports = ("logging")))]
    fn logger(&self, py: Python<'_>) -> Py<PyAny> {
        self.logger.clone_ref(py)
    }
}

pub(crate) fn add_class(api_module: &Bound<'_, PyModule>) -> PyResult<()> {
    api_module.add_class::<ConsoleAPI>()
}

pub(crate) fn definition_from_python(
    api: &Bound<'_, PyAny>,
) -> PyResult<Option<HostAPIDefinition>> {
    if !api.is_instance_of::<ConsoleAPI>() {
        return Ok(None);
    }

    let console = api.extract::<pyo3::PyRef<'_, ConsoleAPI>>()?;
    let py = api.py();

    Ok(Some(HostAPIDefinition::Console(ConsoleDefinition {
        logger: console.logger.clone_ref(py),
    })))
}

pub(crate) fn install(
    py: Python<'_>,
    context: &mut Context,
    definition: &ConsoleDefinition,
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
    local_context.set_slot(std::rc::Rc::new(ConsoleRuntime::new(py, definition)));
    let scope = &mut v8::ContextScope::new(scope, local_context);
    let global = local_context.global(scope);
    let console = console_object(scope, global)?;

    install_console_function(scope, console, "log", log_callback)?;
    install_console_function(scope, console, "info", info_callback)?;
    install_console_function(scope, console, "debug", debug_callback)?;
    install_console_function(scope, console, "warn", warn_callback)?;
    install_console_function(scope, console, "warning", warn_callback)?;
    install_console_function(scope, console, "error", error_callback)?;
    install_console_function(scope, console, "trace", trace_callback)?;
    install_console_function(scope, console, "assert", assert_callback)?;
    install_console_function(scope, console, "clear", clear_callback)?;
    install_console_function(scope, console, "count", count_callback)?;
    install_console_function(scope, console, "countReset", count_reset_callback)?;
    install_console_function(scope, console, "time", time_callback)?;
    install_console_function(scope, console, "timeLog", time_log_callback)?;
    install_console_function(scope, console, "timeEnd", time_end_callback)?;
    install_console_function(scope, console, "group", group_callback)?;
    install_console_function(scope, console, "groupCollapsed", group_callback)?;
    install_console_function(scope, console, "groupEnd", group_end_callback)?;
    install_console_function(scope, console, "dir", info_callback)?;
    install_console_function(scope, console, "table", info_callback)?;

    Ok(())
}

fn validate_logger(logger: &Bound<'_, PyAny>) -> PyResult<()> {
    for method in ["debug", "info", "warning", "error"] {
        let value = logger.getattr(method).map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err(format!(
                "Console logger must provide callable '{method}'.",
            ))
        })?;

        if !value.is_callable() {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Console logger must provide callable '{method}'.",
            )));
        }
    }

    Ok(())
}

fn console_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    global: v8::Local<'s, v8::Object>,
) -> PyResult<v8::Local<'s, v8::Object>> {
    let key = v8::String::new(scope, "console").ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create console key.")
    })?;
    let existing = global.get(scope, key.into());
    let console = existing
        .and_then(|value| {
            if value.is_object() && !value.is_null() {
                value.to_object(scope)
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            let console = v8::Object::new(scope);
            global.set(scope, key.into(), console.into());
            console
        });

    Ok(console)
}

fn install_console_function(
    scope: &mut v8::PinScope<'_, '_>,
    console: v8::Local<'_, v8::Object>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) -> PyResult<()> {
    let key = v8::String::new(scope, name).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create console function name.")
    })?;
    let function = v8::Function::builder(callback)
        .build(scope)
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create console function.")
        })?;
    function.set_name(key);

    console
        .set(scope, key.into(), function.into())
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to install console function.")
        })
        .map(|_| ())
}

fn log_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue<'s, v8::Value>,
) {
    log_to_python(scope, args, rv, "info", None);
}

fn info_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue<'s, v8::Value>,
) {
    log_to_python(scope, args, rv, "info", None);
}

fn debug_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue<'s, v8::Value>,
) {
    log_to_python(scope, args, rv, "debug", None);
}

fn warn_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue<'s, v8::Value>,
) {
    log_to_python(scope, args, rv, "warning", None);
}

fn error_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue<'s, v8::Value>,
) {
    log_to_python(scope, args, rv, "error", None);
}

fn trace_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue<'s, v8::Value>,
) {
    log_to_python(scope, args, rv, "error", Some("Trace: "));
}

fn assert_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let passed = args.length() > 0 && args.get(0).boolean_value(scope);

    if passed {
        rv.set_undefined();
        return;
    }

    let message = if args.length() <= 1 {
        "Assertion failed".to_owned()
    } else {
        format!("Assertion failed: {}", joined_arguments(scope, &args, 1))
    };
    finish_console_call(scope, &mut rv, |runtime, py| {
        runtime.log(py, "error", message)
    });
}

fn clear_callback<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i>,
    _args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    rv.set_undefined();
}

fn count_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let label = label_argument(scope, &args);
    finish_console_call(scope, &mut rv, |runtime, py| runtime.count(py, label));
}

fn count_reset_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let label = label_argument(scope, &args);
    finish_console_call(scope, &mut rv, |runtime, py| runtime.count_reset(py, label));
}

fn time_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let label = label_argument(scope, &args);
    finish_console_call(scope, &mut rv, |runtime, _| {
        runtime.time(label);
        Ok(())
    });
}

fn time_log_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let label = label_argument(scope, &args);
    let suffix = (args.length() > 1).then(|| joined_arguments(scope, &args, 1));
    finish_console_call(scope, &mut rv, |runtime, py| {
        runtime.time_log(py, label, suffix, false)
    });
}

fn time_end_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let label = label_argument(scope, &args);
    finish_console_call(scope, &mut rv, |runtime, py| {
        runtime.time_log(py, label, None, true)
    });
}

fn group_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let message = joined_arguments(scope, &args, 0);
    finish_console_call(scope, &mut rv, |runtime, py| runtime.group(py, message));
}

fn group_end_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    _args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    finish_console_call(scope, &mut rv, |runtime, _| {
        runtime.group_end();
        Ok(())
    });
}

fn log_to_python<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
    method: &'static str,
    prefix: Option<&'static str>,
) {
    let mut message = joined_arguments(scope, &args, 0);

    if let Some(prefix) = prefix {
        message.insert_str(0, prefix);
    }

    finish_console_call(scope, &mut rv, |runtime, py| {
        runtime.log(py, method, message)
    });
}

fn finish_console_call(
    scope: &mut v8::PinScope<'_, '_>,
    rv: &mut v8::ReturnValue<'_, v8::Value>,
    call: impl FnOnce(&ConsoleRuntime, Python<'_>) -> PyResult<()>,
) {
    let context = scope.get_current_context();
    let Some(runtime) = context.get_slot::<ConsoleRuntime>() else {
        throw_js_error(scope, "Console runtime is no longer alive.");
        return;
    };
    let result = Python::attach(|py| call(&runtime, py));

    match result {
        Ok(()) => rv.set_undefined(),
        Err(err) => throw_js_error(scope, &err.to_string()),
    }
}

fn label_argument(
    scope: &v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments<'_>,
) -> String {
    if args.length() == 0 {
        return DEFAULT_LABEL.to_owned();
    }

    value_to_console_string(scope, args.get(0))
}

fn joined_arguments(
    scope: &v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments<'_>,
    start: i32,
) -> String {
    (start..args.length())
        .map(|index| value_to_console_string(scope, args.get(index)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn value_to_console_string(
    scope: &v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
) -> String {
    if value.is_undefined() {
        return "undefined".to_owned();
    }

    if value.is_null() {
        return "null".to_owned();
    }

    if value.is_string() {
        return value
            .to_string(scope)
            .map(|value| value.to_rust_string_lossy(scope))
            .unwrap_or_default();
    }

    if value.is_big_int() {
        return value
            .to_string(scope)
            .map(|value| format!("{}n", value.to_rust_string_lossy(scope)))
            .unwrap_or_else(|| "<bigint>".to_owned());
    }

    value
        .to_string(scope)
        .map(|value| value.to_rust_string_lossy(scope))
        .unwrap_or_else(|| "<value>".to_owned())
}

fn throw_js_error(scope: &mut v8::PinScope<'_, '_>, message: &str) {
    let message = v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::error(scope, message);
    scope.throw_exception(exception);
}
