use std::convert::TryFrom;

use pyo3::PyClassInitializer;
use pyo3::prelude::{Bound, PyAny, PyModule, PyResult, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyModuleMethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::context::Context;
use crate::host_apis::{HostAPI, HostAPIDefinition};

/// Installs queueMicrotask into the JavaScript global object.
#[gen_stub_pyclass]
#[pyclass(extends = HostAPI, module = "v8.api", name = "MicrotaskQueue")]
pub(crate) struct MicrotaskQueueAPI;

#[gen_stub_pymethods]
#[pymethods]
impl MicrotaskQueueAPI {
    /// Create a HostAPI that installs queueMicrotask.
    #[gen_stub(override_return_type(type_repr = "MicrotaskQueue", imports = ()))]
    #[new]
    fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(HostAPI).add_subclass(Self)
    }
}

pub(crate) fn add_class(api_module: &Bound<'_, PyModule>) -> PyResult<()> {
    api_module.add_class::<MicrotaskQueueAPI>()
}

pub(crate) fn definition_from_python(api: &Bound<'_, PyAny>) -> Option<HostAPIDefinition> {
    api.is_instance_of::<MicrotaskQueueAPI>()
        .then_some(HostAPIDefinition::MicrotaskQueue)
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

    let mut isolate_ref = isolate.borrow_mut();
    let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
    let scope = &mut scope.init();
    let local_context = v8::Local::new(scope, context_global);
    let scope = &mut v8::ContextScope::new(scope, local_context);
    let global = local_context.global(scope);

    install_global_function(scope, global, "queueMicrotask", queue_microtask_callback)
}

fn install_global_function(
    scope: &mut v8::PinScope<'_, '_>,
    global: v8::Local<'_, v8::Object>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) -> PyResult<()> {
    let key = v8::String::new(scope, name).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create microtask function name.")
    })?;
    let function = v8::Function::builder(callback)
        .build(scope)
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create microtask function.")
        })?;
    function.set_name(key);

    global
        .set(scope, key.into(), function.into())
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to install microtask API.")
        })
        .map(|_| ())
}

fn queue_microtask_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    if args.length() < 1 || !args.get(0).is_function() {
        throw_js_type_error(scope, "queueMicrotask callback must be a function.");
        return;
    }

    let callback = match v8::Local::<v8::Function>::try_from(args.get(0)) {
        Ok(callback) => callback,
        Err(_) => {
            throw_js_type_error(scope, "queueMicrotask callback must be a function.");
            return;
        }
    };

    scope.enqueue_microtask(callback);
    rv.set_undefined();
}

fn throw_js_type_error(scope: &mut v8::PinScope<'_, '_>, message: &str) {
    let message = v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::type_error(scope, message);
    scope.throw_exception(exception);
}
