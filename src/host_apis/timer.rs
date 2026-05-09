use std::convert::TryFrom;
use std::ffi::c_void;
use std::time::Duration;

use pyo3::PyClassInitializer;
use pyo3::prelude::{Bound, PyAny, PyModule, PyResult, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyModuleMethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::context::Context;
use crate::event_loop::{self, SharedTaskQueue};
use crate::host_apis::{HostAPI, HostAPIDefinition};
use crate::runtime::SharedIsolate;

/// Installs browser-style timer globals backed by the Rust host task queue.
#[gen_stub_pyclass]
#[pyclass(extends = HostAPI, module = "v8.api", name = "Timer")]
pub(crate) struct TimerAPI;

#[gen_stub_pymethods]
#[pymethods]
impl TimerAPI {
    /// Create a timer HostAPI that installs setTimeout, clearTimeout, setInterval, and clearInterval.
    #[gen_stub(override_return_type(type_repr = "Timer", imports = ()))]
    #[new]
    fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(HostAPI).add_subclass(Self)
    }
}

pub(crate) fn add_class(api_module: &Bound<'_, PyModule>) -> PyResult<()> {
    api_module.add_class::<TimerAPI>()
}

pub(crate) fn definition_from_python(api: &Bound<'_, PyAny>) -> Option<HostAPIDefinition> {
    api.is_instance_of::<TimerAPI>()
        .then_some(HostAPIDefinition::Timer)
}

pub(crate) fn install(context: &mut Context) -> PyResult<()> {
    let isolate = context
        .isolate
        .as_ref()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive."))?;
    let context_global = context
        .context
        .as_ref()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive."))?;

    install_timer_api(isolate, context.isolate_id, context_global)
}

fn install_timer_api(
    isolate: &SharedIsolate,
    isolate_id: u64,
    context: &v8::Global<v8::Context>,
) -> PyResult<()> {
    event_loop::register_task_queue(isolate_id);

    let mut isolate_ref = isolate.borrow_mut();
    let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
    let scope = &mut scope.init();
    let local_context = v8::Local::new(scope, context);
    let scope = &mut v8::ContextScope::new(scope, local_context);
    let global = local_context.global(scope);
    let data = v8::External::new(scope, isolate_id as usize as *mut c_void);

    install_global_function(
        scope,
        global,
        "setTimeout",
        set_timeout_callback,
        data.into(),
    )?;
    install_global_function(
        scope,
        global,
        "clearTimeout",
        clear_timer_callback,
        data.into(),
    )?;
    install_global_function(
        scope,
        global,
        "setInterval",
        set_interval_callback,
        data.into(),
    )?;
    install_global_function(
        scope,
        global,
        "clearInterval",
        clear_timer_callback,
        data.into(),
    )?;

    Ok(())
}

fn install_global_function(
    scope: &mut v8::PinScope<'_, '_>,
    global: v8::Local<'_, v8::Object>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    data: v8::Local<'_, v8::Value>,
) -> PyResult<()> {
    let key = v8::String::new(scope, name).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create timer function name.")
    })?;
    let function = v8::Function::builder(callback)
        .data(data)
        .build(scope)
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create timer function.")
        })?;
    function.set_name(key);

    global
        .set(scope, key.into(), function.into())
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Failed to install timer API."))
        .map(|_| ())
}

fn set_timeout_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    set_timer_callback(scope, args, &mut rv, false);
}

fn set_interval_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    set_timer_callback(scope, args, &mut rv, true);
}

fn clear_timer_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let Some(queue) = callback_task_queue(&args) else {
        rv.set_undefined();
        return;
    };
    let id = timer_id_argument(scope, &args);

    queue.borrow_mut().clear_timer(id);
    rv.set_undefined();
}

fn set_timer_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: &mut v8::ReturnValue<'s, v8::Value>,
    repeat: bool,
) {
    let Some(queue) = callback_task_queue(&args) else {
        throw_js_error(scope, "Timer runtime is no longer alive.");
        return;
    };

    if args.length() < 1 || !args.get(0).is_function() {
        throw_js_type_error(scope, "Timer callback must be a function.");
        return;
    }

    let callback = match v8::Local::<v8::Function>::try_from(args.get(0)) {
        Ok(callback) => callback,
        Err(_) => {
            throw_js_type_error(scope, "Timer callback must be a function.");
            return;
        }
    };
    let delay = timer_delay_argument(scope, &args);
    let timer_args = (2..args.length())
        .map(|index| args.get(index))
        .collect::<Vec<_>>();
    let repeat = repeat.then_some(delay);
    let id = queue
        .borrow_mut()
        .set_timer(scope, callback, timer_args, delay, repeat);

    rv.set_uint32(id);
}

fn callback_task_queue(args: &v8::FunctionCallbackArguments<'_>) -> Option<SharedTaskQueue> {
    let data = args.data();
    let data = v8::Local::<v8::External>::try_from(data).ok()?;
    let isolate_id = data.value() as usize as u64;

    event_loop::task_queue(isolate_id)
}

fn timer_id_argument(
    scope: &v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments<'_>,
) -> u32 {
    if args.length() == 0 {
        return 0;
    }

    args.get(0)
        .to_uint32(scope)
        .map(|value| value.value())
        .unwrap_or(0)
}

fn timer_delay_argument(
    scope: &v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments<'_>,
) -> Duration {
    if args.length() < 2 {
        return Duration::ZERO;
    }

    let delay = args
        .get(1)
        .to_number(scope)
        .map(|number| number.value())
        .unwrap_or(0.0);

    if !delay.is_finite() || delay <= 0.0 {
        return Duration::ZERO;
    }

    Duration::from_millis(delay.min(u64::MAX as f64) as u64)
}

fn throw_js_error(scope: &mut v8::PinScope<'_, '_>, message: &str) {
    let message = v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::error(scope, message);
    scope.throw_exception(exception);
}

fn throw_js_type_error(scope: &mut v8::PinScope<'_, '_>, message: &str) {
    let message = v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::type_error(scope, message);
    scope.throw_exception(exception);
}
