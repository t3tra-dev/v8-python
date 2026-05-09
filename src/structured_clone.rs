use pyo3::prelude::{Bound, PyAny, PyResult, Python};
use pyo3::types::PyBytes;
use v8::{ValueDeserializerHelper, ValueSerializerHelper};

use crate::error::js_exception;
use crate::runtime::SharedIsolate;
use crate::v8value::{V8Value, Value, python_bytes_like_to_vec, python_to_v8};

struct StructuredCloneSerializer;

impl v8::ValueSerializerImpl for StructuredCloneSerializer {
    fn throw_data_clone_error<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        message: v8::Local<'s, v8::String>,
    ) {
        let error = v8::Exception::error(scope, message);
        scope.throw_exception(error);
    }
}

struct StructuredCloneDeserializer;

impl v8::ValueDeserializerImpl for StructuredCloneDeserializer {}

pub(crate) fn serialize<'py>(
    py: Python<'py>,
    isolate: &SharedIsolate,
    isolate_id: u64,
    context: &v8::Global<v8::Context>,
    value: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let bytes = {
        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);
        v8::tc_scope!(let scope, &mut **scope);

        let value = python_to_v8(py, scope, value, isolate_id, 0)?;
        let serializer = v8::ValueSerializer::new(scope, Box::new(StructuredCloneSerializer));

        serializer.write_header();
        match serializer.write_value(local_context, value) {
            Some(true) => serializer.release(),
            Some(false) => {
                return Err(js_exception(
                    scope,
                    "Structured clone serialization was rejected.",
                ));
            }
            None => {
                return Err(js_exception(
                    scope,
                    "Structured clone serialization failed.",
                ));
            }
        }
    };

    Ok(PyBytes::new(py, &bytes))
}

pub(crate) fn deserialize(
    data: &Bound<'_, PyAny>,
    isolate: &SharedIsolate,
    isolate_id: u64,
    context: &v8::Global<v8::Context>,
) -> PyResult<Value> {
    let bytes = python_bytes_like_to_vec(data.py(), data)?.ok_or_else(|| {
        pyo3::exceptions::PyTypeError::new_err(
            "deserialize() expects bytes, bytearray, or memoryview.",
        )
    })?;

    let mut isolate_ref = isolate.borrow_mut();
    let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
    let scope = &mut scope.init();
    let local_context = v8::Local::new(scope, context);
    let scope = &mut v8::ContextScope::new(scope, local_context);
    v8::tc_scope!(let scope, &mut **scope);

    let deserializer =
        v8::ValueDeserializer::new(scope, Box::new(StructuredCloneDeserializer), &bytes);

    match deserializer.read_header(local_context) {
        Some(true) => {}
        Some(false) => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Invalid structured clone header.",
            ));
        }
        None => {
            return Err(js_exception(
                scope,
                "Structured clone deserialization failed.",
            ));
        }
    }

    let value = deserializer.read_value(local_context).ok_or_else(|| {
        js_exception(
            scope,
            "Structured clone deserialization did not produce a value.",
        )
    })?;

    Ok(Value::from_handle(V8Value::from_local(
        scope,
        value,
        context.clone(),
        isolate.clone(),
        isolate_id,
    )))
}
