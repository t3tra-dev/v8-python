use pyo3::IntoPyObjectExt;
use pyo3::buffer::PyBuffer;
use pyo3::prelude::{Bound, Py, PyAny, PyRef, PyResult, Python};
use pyo3::types::{
    PyAnyMethods, PyBool, PyBytes, PyDict, PyDictMethods, PyFloat, PyInt, PyList, PyListMethods,
    PyString, PyTuple, PyTupleMethods, PyTypeMethods,
};

use super::embedder::V8External;
use super::kind::promise_state_name;
use super::typed::{
    V8Array, V8ArrayBuffer, V8ArrayBufferView, V8BigInt, V8DataView, V8Date, V8Function, V8Map,
    V8Object, V8Promise, V8Proxy, V8RegExp, V8Set, V8String, V8Symbol, V8TypedArray,
    array_buffer_to_vec, array_buffer_view_to_vec, copy_bytes_to_array_buffer,
};
use super::value::Value;
use super::wasm::V8WasmModule;
use crate::runtime;

const MAX_TO_PYTHON_DEPTH: usize = 64;

pub(crate) fn python_to_v8<'s>(
    py: Python<'_>,
    scope: &v8::PinScope<'s, '_>,
    value: &Bound<'_, PyAny>,
    isolate_id: u64,
    depth: usize,
) -> PyResult<v8::Local<'s, v8::Value>> {
    if depth > MAX_TO_PYTHON_DEPTH {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "Maximum Python value conversion depth exceeded.",
        ));
    }

    if value.is_none() {
        return Ok(v8::null(scope).into());
    }

    if value.is_exact_instance_of::<PyBool>() {
        return Ok(v8::Boolean::new(scope, value.extract::<bool>()?).into());
    }

    if let Ok(value) = value.extract::<PyRef<'_, Value>>() {
        value.handle.ensure_isolate(isolate_id)?;
        return Ok(v8::Local::new(scope, &value.handle.value));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8String>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8Object>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8Array>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8Function>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8Promise>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8BigInt>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8Symbol>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8ArrayBuffer>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8ArrayBufferView>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8TypedArray>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8DataView>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8Map>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8Set>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8Date>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8RegExp>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8Proxy>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8External>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if let Ok(value) = value.extract::<PyRef<'_, V8WasmModule>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_value(scope));
    }

    if value.is_instance_of::<PyInt>() {
        if let Ok(value) = value.extract::<i32>() {
            return Ok(v8::Integer::new(scope, value).into());
        }

        if let Ok(value) = value.extract::<u32>() {
            return Ok(v8::Integer::new_from_unsigned(scope, value).into());
        }

        if let Ok(value) = value.extract::<i64>() {
            return Ok(v8::BigInt::new_from_i64(scope, value).into());
        }

        if let Ok(value) = value.extract::<u64>() {
            return Ok(v8::BigInt::new_from_u64(scope, value).into());
        }

        return Err(pyo3::exceptions::PyOverflowError::new_err(
            "Python int is too large to convert to V8 BigInt.",
        ));
    }

    if value.is_instance_of::<PyFloat>() {
        return Ok(v8::Number::new(scope, value.extract::<f64>()?).into());
    }

    if value.is_instance_of::<PyString>() {
        let value = value.extract::<String>()?;
        let value = v8::String::new(scope, &value).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create v8::String.")
        })?;

        return Ok(value.into());
    }

    if let Some(bytes) = python_bytes_like_to_vec(py, value)? {
        let array_buffer = v8::ArrayBuffer::new(scope, bytes.len());
        copy_bytes_to_array_buffer(array_buffer, &bytes)?;

        return Ok(array_buffer.into());
    }

    if let Ok(list) = value.cast::<PyList>() {
        return python_sequence_to_v8(py, scope, list.iter(), list.len(), isolate_id, depth + 1);
    }

    if let Ok(tuple) = value.cast::<PyTuple>() {
        return python_sequence_to_v8(py, scope, tuple.iter(), tuple.len(), isolate_id, depth + 1);
    }

    if let Ok(dict) = value.cast::<PyDict>() {
        let object = v8::Object::new(scope);

        for (key, item) in dict.iter() {
            let key = python_to_v8(py, scope, &key, isolate_id, depth + 1)?;
            let item = python_to_v8(py, scope, &item, isolate_id, depth + 1)?;

            object.set(scope, key, item).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to set object property.")
            })?;
        }

        return Ok(object.into());
    }

    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "Cannot convert Python value of type '{}' to a V8 value.",
        value.get_type().name()?
    )))
}

fn python_sequence_to_v8<'s, 'py, I>(
    py: Python<'_>,
    scope: &v8::PinScope<'s, '_>,
    items: I,
    len: usize,
    isolate_id: u64,
    depth: usize,
) -> PyResult<v8::Local<'s, v8::Value>>
where
    I: IntoIterator<Item = Bound<'py, PyAny>>,
{
    let length = i32::try_from(len).map_err(|_| {
        pyo3::exceptions::PyOverflowError::new_err("Python sequence is too long for V8 Array.")
    })?;
    let array = v8::Array::new(scope, length);

    for (index, item) in items.into_iter().enumerate() {
        let item = python_to_v8(py, scope, &item, isolate_id, depth)?;
        array.set_index(scope, index as u32, item).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to set array item.")
        })?;
    }

    Ok(array.into())
}

pub(super) fn python_args_to_v8<'s>(
    args: Option<&Bound<'_, PyAny>>,
    scope: &v8::PinScope<'s, '_>,
    isolate_id: u64,
) -> PyResult<Vec<v8::Local<'s, v8::Value>>> {
    let Some(args) = args else {
        return Ok(Vec::new());
    };

    if args.is_none() {
        return Ok(Vec::new());
    }

    if let Ok(list) = args.cast::<PyList>() {
        return list
            .iter()
            .map(|item| python_to_v8(args.py(), scope, &item, isolate_id, 0))
            .collect();
    }

    if let Ok(tuple) = args.cast::<PyTuple>() {
        return tuple
            .iter()
            .map(|item| python_to_v8(args.py(), scope, &item, isolate_id, 0))
            .collect();
    }

    Err(pyo3::exceptions::PyTypeError::new_err(
        "Function arguments must be a list or tuple.",
    ))
}

pub(crate) fn value_to_python(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
    depth: usize,
) -> PyResult<Py<PyAny>> {
    if depth > MAX_TO_PYTHON_DEPTH {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "Maximum V8 value conversion depth exceeded.",
        ));
    }

    if value.is_undefined() || value.is_null() {
        return Ok(py.None());
    }

    if value.is_boolean() {
        return value.boolean_value(scope).into_py_any(py);
    }

    if value.is_int32() {
        let value = value.int32_value(scope).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to int32.")
        })?;
        return value.into_py_any(py);
    }

    if value.is_uint32() {
        let value = value.uint32_value(scope).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to uint32.")
        })?;
        return value.into_py_any(py);
    }

    if value.is_number() {
        let value = value.number_value(scope).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to number.")
        })?;
        return value.into_py_any(py);
    }

    if value.is_big_int() {
        return bigint_to_python(py, scope, value);
    }

    if value.is_symbol() {
        return symbol_to_string(scope, value)?.into_py_any(py);
    }

    if value.is_string() {
        let value = value.to_string(scope).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to string.")
        })?;
        return value.to_rust_string_lossy(scope).into_py_any(py);
    }

    if value.is_external() {
        let external = v8::Local::<v8::External>::try_from(value).map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to External.")
        })?;

        return runtime::external_payload(py, external.value()).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "External payload is not managed by this runtime.",
            )
        });
    }

    if value.is_array_buffer() {
        let array_buffer = v8::Local::<v8::ArrayBuffer>::try_from(value).map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to ArrayBuffer.")
        })?;
        let bytes = array_buffer_to_vec(array_buffer)?;

        return Ok(PyBytes::new(py, &bytes).into_any().unbind());
    }

    if value.is_array_buffer_view() {
        let view = v8::Local::<v8::ArrayBufferView>::try_from(value).map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to ArrayBufferView.")
        })?;
        let bytes = array_buffer_view_to_vec(view)?;

        return Ok(PyBytes::new(py, &bytes).into_any().unbind());
    }

    if value.is_array() {
        return array_to_python(py, scope, value, depth + 1);
    }

    if value.is_map() {
        return map_to_python(py, scope, value, depth + 1);
    }

    if value.is_set() {
        return set_to_python(py, scope, value, depth + 1);
    }

    if value.is_date() {
        let date = v8::Local::<v8::Date>::try_from(value).map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Date.")
        })?;

        return date_to_python(py, date);
    }

    if value.is_function() {
        return function_to_python(py, scope, value);
    }

    if value.is_promise() {
        return promise_to_python(py, scope, value, depth + 1);
    }

    object_to_python(py, scope, value, depth + 1)
}

pub(crate) fn python_bytes_like_to_vec(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
) -> PyResult<Option<Vec<u8>>> {
    let Ok(buffer) = PyBuffer::<u8>::get(value) else {
        return Ok(None);
    };

    Ok(Some(buffer.to_vec(py)?))
}

fn array_to_python(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
    depth: usize,
) -> PyResult<Py<PyAny>> {
    let array = v8::Local::<v8::Array>::try_from(value)
        .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Array."))?;
    let list = PyList::empty(py);

    for index in 0..array.length() {
        let item = array.get_index(scope, index).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to read array item.")
        })?;
        list.append(value_to_python(py, scope, item, depth)?)?;
    }

    Ok(list.into_any().unbind())
}

fn object_to_python(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
    depth: usize,
) -> PyResult<Py<PyAny>> {
    let object = value.to_object(scope).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to object.")
    })?;
    let names = object
        .get_own_property_names(scope, Default::default())
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to read object property names.")
        })?;
    let dict = PyDict::new(py);

    for index in 0..names.length() {
        let key = names.get_index(scope, index).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to read object property key.")
        })?;
        let item = object.get(scope, key).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to read object property value.")
        })?;

        dict.set_item(
            value_to_python(py, scope, key, depth)?,
            value_to_python(py, scope, item, depth)?,
        )?;
    }

    Ok(dict.into_any().unbind())
}

fn map_to_python(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
    depth: usize,
) -> PyResult<Py<PyAny>> {
    let map = v8::Local::<v8::Map>::try_from(value)
        .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Map."))?;
    let entries = map.as_array(scope);
    let list = PyList::empty(py);

    for index in (0..entries.length()).step_by(2) {
        let key = entries
            .get_index(scope, index)
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Failed to read Map key."))?;
        let value = entries.get_index(scope, index + 1).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to read Map value.")
        })?;
        let item = PyTuple::new(
            py,
            [
                value_to_python(py, scope, key, depth)?,
                value_to_python(py, scope, value, depth)?,
            ],
        )?;

        list.append(item)?;
    }

    Ok(list.into_any().unbind())
}

fn set_to_python(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
    depth: usize,
) -> PyResult<Py<PyAny>> {
    let set = v8::Local::<v8::Set>::try_from(value)
        .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Set."))?;
    let entries = set.as_array(scope);
    let step = if entries.length() as usize == set.size() {
        1
    } else {
        2
    };
    let list = PyList::empty(py);

    for index in (0..entries.length()).step_by(step) {
        let value = entries.get_index(scope, index).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to read Set value.")
        })?;

        list.append(value_to_python(py, scope, value, depth)?)?;
    }

    Ok(list.into_any().unbind())
}

fn function_to_python(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
) -> PyResult<Py<PyAny>> {
    let function = v8::Local::<v8::Function>::try_from(value).map_err(|_| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Function.")
    })?;
    let dict = PyDict::new(py);
    let name = function.get_name(scope).to_rust_string_lossy(scope);
    let source = value
        .to_string(scope)
        .map(|value| value.to_rust_string_lossy(scope))
        .unwrap_or_default();

    dict.set_item("type", "function")?;
    dict.set_item("name", name)?;
    dict.set_item("source", source)?;

    Ok(dict.into_any().unbind())
}

fn promise_to_python(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
    depth: usize,
) -> PyResult<Py<PyAny>> {
    let promise = v8::Local::<v8::Promise>::try_from(value).map_err(|_| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Promise.")
    })?;
    let dict = PyDict::new(py);
    let state = promise.state();

    dict.set_item("type", "promise")?;
    dict.set_item("state", promise_state_name(&state))?;

    if state != v8::PromiseState::Pending {
        dict.set_item(
            "result",
            value_to_python(py, scope, promise.result(scope), depth)?,
        )?;
    }

    Ok(dict.into_any().unbind())
}

pub(super) fn date_to_python(py: Python<'_>, date: v8::Local<'_, v8::Date>) -> PyResult<Py<PyAny>> {
    let datetime = py.import("datetime")?;
    let timezone_utc = datetime.getattr("timezone")?.getattr("utc")?;
    let seconds = date.value_of() / 1000.0;

    Ok(datetime
        .getattr("datetime")?
        .call_method1("fromtimestamp", (seconds, timezone_utc))?
        .unbind())
}

pub(super) fn symbol_to_string(
    scope: &v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
) -> PyResult<String> {
    let symbol = v8::Local::<v8::Symbol>::try_from(value).map_err(|_| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Symbol.")
    })?;
    let description = symbol.description(scope);

    if description.is_undefined() {
        return Ok("Symbol()".to_owned());
    }

    let description = description.to_string(scope).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to convert symbol description to string.")
    })?;

    Ok(format!(
        "Symbol({})",
        description.to_rust_string_lossy(scope)
    ))
}

pub(super) fn bigint_to_python(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
) -> PyResult<Py<PyAny>> {
    let decimal = bigint_to_decimal_string(scope, value)?;
    Ok(py.get_type::<PyInt>().call1((decimal,))?.unbind())
}

pub(super) fn bigint_to_decimal_string(
    scope: &v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
) -> PyResult<String> {
    let bigint = value.to_big_int(scope).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to bigint.")
    })?;
    let value = bigint.to_string(scope).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to convert bigint to string.")
    })?;

    Ok(value.to_rust_string_lossy(scope))
}
