use std::convert::TryFrom;

use pyo3::PyClassInitializer;
use pyo3::prelude::{Bound, PyAny, PyModule, PyResult, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyModuleMethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::context::Context;
use crate::host_apis::{HostAPI, HostAPIDefinition};

/// Enables SharedArrayBuffer construction in contexts that install this HostAPI.
#[gen_stub_pyclass]
#[pyclass(extends = HostAPI, module = "v8.api", name = "SharedArrayBuffer")]
pub(crate) struct SharedArrayBufferAPI;

#[gen_stub_pymethods]
#[pymethods]
impl SharedArrayBufferAPI {
    /// Create a HostAPI that installs the SharedArrayBuffer constructor.
    #[gen_stub(override_return_type(type_repr = "SharedArrayBuffer", imports = ()))]
    #[new]
    fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(HostAPI).add_subclass(Self)
    }
}

pub(crate) fn add_class(api_module: &Bound<'_, PyModule>) -> PyResult<()> {
    api_module.add_class::<SharedArrayBufferAPI>()
}

pub(crate) fn definition_from_python(api: &Bound<'_, PyAny>) -> Option<HostAPIDefinition> {
    api.is_instance_of::<SharedArrayBufferAPI>()
        .then_some(HostAPIDefinition::SharedArrayBuffer)
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
    let constructor_key = v8::String::new(scope, "SharedArrayBuffer").ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err(
            "Failed to create SharedArrayBuffer constructor name.",
        )
    })?;
    let constructor = global.get(scope, constructor_key.into()).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("SharedArrayBuffer is not available.")
    })?;
    let constructor = v8::Local::<v8::Object>::try_from(constructor).map_err(|_| {
        pyo3::exceptions::PyRuntimeError::new_err("SharedArrayBuffer constructor is invalid.")
    })?;

    install_constructor_function(
        scope,
        constructor,
        "fromHost",
        shared_array_buffer_from_host,
    )
}

fn install_constructor_function(
    scope: &mut v8::PinScope<'_, '_>,
    constructor: v8::Local<'_, v8::Object>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) -> PyResult<()> {
    let key = v8::String::new(scope, name).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err(
            "Failed to create SharedArrayBuffer function name.",
        )
    })?;
    let function = v8::Function::builder(callback)
        .build(scope)
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "Failed to create SharedArrayBuffer function.",
            )
        })?;
    function.set_name(key);

    constructor
        .set(scope, key.into(), function.into())
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to install SharedArrayBuffer API.")
        })
        .map(|_| ())
}

fn shared_array_buffer_from_host<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let Some(byte_length) = byte_length_argument(scope, &args) else {
        return;
    };

    let backing_store = v8::SharedArrayBuffer::new_backing_store(scope, byte_length).make_shared();
    let buffer = v8::SharedArrayBuffer::with_backing_store(scope, &backing_store);
    rv.set(buffer.into());
}

fn byte_length_argument(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments<'_>,
) -> Option<usize> {
    if args.length() == 0 {
        throw_js_type_error(scope, "SharedArrayBuffer.fromHost requires a byteLength.");
        return None;
    }

    let Some(number) = args.get(0).number_value(scope) else {
        throw_js_type_error(
            scope,
            "SharedArrayBuffer.fromHost byteLength must be a number.",
        );
        return None;
    };

    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    let max_byte_length = (usize::MAX as f64).min(MAX_SAFE_INTEGER);

    if !number.is_finite() || number < 0.0 || number.fract() != 0.0 || number > max_byte_length {
        throw_js_range_error(
            scope,
            "SharedArrayBuffer.fromHost byteLength must be a non-negative safe integer.",
        );
        return None;
    }

    Some(number as usize)
}

fn throw_js_type_error(scope: &mut v8::PinScope<'_, '_>, message: &str) {
    let message = v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::type_error(scope, message);
    scope.throw_exception(exception);
}

fn throw_js_range_error(scope: &mut v8::PinScope<'_, '_>, message: &str) {
    let message = v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::range_error(scope, message);
    scope.throw_exception(exception);
}
