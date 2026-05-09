use pyo3::IntoPyObjectExt;
use pyo3::prelude::{Bound, Py, PyAny, PyRef, PyResult, Python, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyTuple};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use super::PromiseAwaiter;
use super::convert::{
    bigint_to_decimal_string, bigint_to_python, python_args_to_v8, python_to_v8, symbol_to_string,
    value_to_python,
};
use super::embedder::{
    V8External, V8Private, delete_private_on_object, get_internal_field_on_object,
    get_private_on_object, has_private_on_object, set_internal_field_on_object,
    set_private_on_object,
};
use super::handle::V8Value;
use super::kind::ValueKind;
use super::operator::{self, BinaryOperator, UnaryOperator};
use super::property::{
    PropertyAttribute, PropertyDescriptor, define_own_property_on_object,
    define_property_on_object, get_own_property_descriptor_on_object,
    get_property_attributes_on_object, property_attribute_from_python,
    set_integrity_level_on_object,
};
use super::typed::{
    V8Array, V8ArrayBuffer, V8ArrayBufferView, V8BigInt, V8DataView, V8Date, V8Function, V8Map,
    V8Object, V8Promise, V8Proxy, V8RegExp, V8Set, V8String, V8Symbol, V8TypedArray,
};
use super::wasm::V8WasmModule;
use crate::error::js_exception;
use crate::runtime::SharedIsolate;

/// Generic JavaScript value wrapper.
#[gen_stub_pyclass]
#[pyclass(unsendable)]
pub(crate) struct Value {
    pub(crate) handle: V8Value,
}

#[gen_stub_pymethods]
#[pymethods]
impl Value {
    /// Return this value's V8 kind name.
    #[getter]
    fn kind(&self) -> &'static str {
        self.handle.kind.as_str()
    }

    /// Return whether this value is JavaScript undefined.
    fn is_undefined(&self) -> bool {
        matches!(self.handle.kind, ValueKind::Undefined)
    }

    /// Return whether this value is JavaScript null.
    fn is_null(&self) -> bool {
        matches!(self.handle.kind, ValueKind::Null)
    }

    /// Return whether this value is a boolean.
    fn is_boolean(&self) -> bool {
        matches!(self.handle.kind, ValueKind::Boolean)
    }

    /// Return whether this value is an Int32.
    fn is_int32(&self) -> bool {
        matches!(self.handle.kind, ValueKind::Int32)
    }

    /// Return whether this value is a Uint32.
    fn is_uint32(&self) -> bool {
        matches!(self.handle.kind, ValueKind::Uint32)
    }

    /// Return whether this value is a JavaScript number.
    fn is_number(&self) -> bool {
        self.handle.kind.is_number()
    }

    /// Return whether this value is a BigInt.
    fn is_big_int(&self) -> bool {
        matches!(self.handle.kind, ValueKind::BigInt)
    }

    /// Return whether this value is a string.
    fn is_string(&self) -> bool {
        matches!(self.handle.kind, ValueKind::String)
    }

    /// Return whether this value is a symbol.
    fn is_symbol(&self) -> bool {
        matches!(self.handle.kind, ValueKind::Symbol)
    }

    /// Return whether this value is an array.
    fn is_array(&self) -> bool {
        matches!(self.handle.kind, ValueKind::Array)
    }

    /// Return whether this value is a function.
    fn is_function(&self) -> bool {
        matches!(self.handle.kind, ValueKind::Function)
    }

    /// Return whether this value is a Promise.
    fn is_promise(&self) -> bool {
        matches!(self.handle.kind, ValueKind::Promise)
    }

    /// Return whether this value is a Map.
    fn is_map(&self) -> bool {
        matches!(self.handle.kind, ValueKind::Map)
    }

    /// Return whether this value is a Set.
    fn is_set(&self) -> bool {
        matches!(self.handle.kind, ValueKind::Set)
    }

    /// Return whether this value is a Date.
    fn is_date(&self) -> bool {
        matches!(self.handle.kind, ValueKind::Date)
    }

    /// Return whether this value is a RegExp.
    fn is_regexp(&self) -> bool {
        matches!(self.handle.kind, ValueKind::RegExp)
    }

    /// Return whether this value is a Proxy.
    fn is_proxy(&self) -> bool {
        matches!(self.handle.kind, ValueKind::Proxy)
    }

    /// Return whether this value is an External.
    fn is_external(&self) -> bool {
        matches!(self.handle.kind, ValueKind::External)
    }

    /// Return whether this value is a WebAssembly.Module object.
    fn is_wasm_module(&self) -> bool {
        matches!(self.handle.kind, ValueKind::WasmModule)
    }

    /// Return whether this value is an ArrayBuffer.
    fn is_array_buffer(&self) -> bool {
        matches!(self.handle.kind, ValueKind::ArrayBuffer)
    }

    /// Return whether this value is an ArrayBufferView, TypedArray, or DataView.
    fn is_array_buffer_view(&self) -> bool {
        matches!(
            self.handle.kind,
            ValueKind::ArrayBufferView | ValueKind::TypedArray | ValueKind::DataView
        )
    }

    /// Return whether this value is a TypedArray.
    fn is_typed_array(&self) -> bool {
        matches!(self.handle.kind, ValueKind::TypedArray)
    }

    /// Return whether this value is a DataView.
    fn is_data_view(&self) -> bool {
        matches!(self.handle.kind, ValueKind::DataView)
    }

    /// Return whether this value is object-like.
    fn is_object(&self) -> bool {
        self.handle.kind.is_object()
    }

    /// Return whether this value is a native JavaScript Error object.
    fn is_error(&self) -> PyResult<bool> {
        self.handle
            .with_local_value(|_, value| Ok(value.is_native_error()))
    }

    /// Return whether this value can be called as a function.
    fn is_callable(&self) -> bool {
        self.is_function()
    }

    /// Return JavaScript typeof for this value.
    #[pyo3(name = "typeof")]
    fn typeof_(&self) -> PyResult<String> {
        self.handle
            .with_local_value(|scope, value| Ok(value.type_of(scope).to_rust_string_lossy(scope)))
    }

    /// Convert this value using JavaScript's string conversion.
    fn to_string(&self) -> PyResult<String> {
        self.handle.with_local_value(|scope, value| {
            if value.is_symbol() {
                return symbol_to_string(scope, value);
            }

            let value = value.to_string(scope).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to string.")
            })?;

            Ok(value.to_rust_string_lossy(scope))
        })
    }

    /// Compare this value with JavaScript strict equality.
    fn strict_equals(&self, other: PyRef<'_, Value>) -> PyResult<bool> {
        other.handle.ensure_isolate(self.handle.isolate_id)?;

        self.handle.with_local_value(|scope, value| {
            let other = v8::Local::new(scope, &other.handle.value);
            Ok(value.strict_equals(other))
        })
    }

    /// Compare this value with JavaScript SameValue semantics.
    fn same_value(&self, other: PyRef<'_, Value>) -> PyResult<bool> {
        other.handle.ensure_isolate(self.handle.isolate_id)?;

        self.handle.with_local_value(|scope, value| {
            let other = v8::Local::new(scope, &other.handle.value);
            Ok(value.same_value(other))
        })
    }

    /// Return this value as bool if it is a JavaScript boolean.
    fn as_boolean(&self) -> PyResult<Option<bool>> {
        if !self.is_boolean() {
            return Ok(None);
        }

        self.handle
            .with_local_value(|scope, value| Ok(Some(value.boolean_value(scope))))
    }

    /// Return this value as i32 if it is an Int32.
    fn as_int32(&self) -> PyResult<Option<i32>> {
        if !self.is_int32() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            value.int32_value(scope).map(Some).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to int32.")
            })
        })
    }

    /// Return this value as u32 if it is a Uint32.
    fn as_uint32(&self) -> PyResult<Option<u32>> {
        if !self.is_uint32() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            value.uint32_value(scope).map(Some).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to uint32.")
            })
        })
    }

    /// Return this value as f64 if it is a JavaScript number.
    fn as_number(&self) -> PyResult<Option<f64>> {
        if !self.is_number() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            value.number_value(scope).map(Some).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to number.")
            })
        })
    }

    /// Return this value as a Python int if it is a BigInt.
    #[gen_stub(override_return_type(type_repr = "int | None", imports = ()))]
    fn as_big_int(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        if !self.is_big_int() {
            return Ok(None);
        }

        self.handle
            .with_local_value(|scope, value| bigint_to_python(py, scope, value).map(Some))
    }

    /// Return this BigInt as i64 when the conversion is lossless.
    fn as_big_int_i64(&self) -> PyResult<Option<i64>> {
        if !self.is_big_int() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let bigint = value.to_big_int(scope).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to bigint.")
            })?;
            let (value, lossless) = bigint.i64_value();

            Ok(lossless.then_some(value))
        })
    }

    /// Return this BigInt as a decimal string.
    fn as_big_int_string(&self) -> PyResult<Option<String>> {
        if !self.is_big_int() {
            return Ok(None);
        }

        self.handle
            .with_local_value(|scope, value| bigint_to_decimal_string(scope, value).map(Some))
    }

    /// Return JavaScript truthiness for this value.
    fn __bool__(&self) -> PyResult<bool> {
        self.handle
            .with_local_value(|scope, value| Ok(value.boolean_value(scope)))
    }

    /// Convert this numeric or BigInt value to a Python int.
    #[gen_stub(override_return_type(type_repr = "int", imports = ()))]
    fn __int__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if self.is_int32() {
            return self
                .as_int32()?
                .ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to int32.")
                })?
                .into_py_any(py);
        }

        if self.is_uint32() {
            return self
                .as_uint32()?
                .ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to uint32.")
                })?
                .into_py_any(py);
        }

        if self.is_big_int() {
            return self.handle.with_local_value(|scope, value| {
                let decimal = bigint_to_decimal_string(scope, value)?;
                Ok(py
                    .get_type::<pyo3::types::PyInt>()
                    .call1((decimal,))?
                    .unbind())
            });
        }

        Err(pyo3::exceptions::PyTypeError::new_err(
            "Value cannot be converted to int.",
        ))
    }

    /// Convert this numeric value to a Python float.
    fn __float__(&self) -> PyResult<f64> {
        if !self.is_number() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "Value cannot be converted to float.",
            ));
        }

        self.as_number()?.ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to number.")
        })
    }

    /// Convert this array value to a Python object.
    #[gen_stub(override_return_type(type_repr = "list[object] | None", imports = ()))]
    fn as_array(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        if !self.is_array() {
            return Ok(None);
        }

        self.to_python(py).map(Some)
    }

    /// Convert this object value to a Python object.
    #[gen_stub(override_return_type(type_repr = "dict[object, object] | None", imports = ()))]
    fn as_object(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        if !self.is_object() {
            return Ok(None);
        }

        self.to_python(py).map(Some)
    }

    /// Convert this function value to a Python object.
    #[gen_stub(override_return_type(type_repr = "dict[str, str] | None", imports = ()))]
    fn as_function(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        if !self.is_function() {
            return Ok(None);
        }

        self.to_python(py).map(Some)
    }

    /// Return this value as a String wrapper.
    fn as_v8_string(&self) -> PyResult<Option<V8String>> {
        if !self.is_string() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::String>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to String.")
            })?;

            Ok(Some(V8String::from_value_local(
                scope,
                value,
                self.handle.clone(),
            )))
        })
    }

    /// Return this value as an Object wrapper.
    fn as_v8_object(&self) -> PyResult<Option<V8Object>> {
        if !self.is_object() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let object = value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to object.")
            })?;

            Ok(Some(V8Object::from_local(
                scope,
                object,
                self.handle.clone(),
            )))
        })
    }

    /// Return this value as an Array wrapper.
    fn as_v8_array(&self) -> PyResult<Option<V8Array>> {
        if !self.is_array() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::Array>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Array.")
            })?;

            Ok(Some(V8Array::from_local(scope, value, self.handle.clone())))
        })
    }

    /// Return this value as a Function wrapper.
    fn as_v8_function(&self) -> PyResult<Option<V8Function>> {
        if !self.is_function() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::Function>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Function.")
            })?;

            Ok(Some(V8Function::from_local(
                scope,
                value,
                self.handle.clone(),
            )))
        })
    }

    /// Return this value as a Promise wrapper.
    fn as_v8_promise(&self) -> PyResult<Option<V8Promise>> {
        if !self.is_promise() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::Promise>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Promise.")
            })?;

            Ok(Some(V8Promise::from_local(
                scope,
                value,
                self.handle.clone(),
            )))
        })
    }

    /// Return this value as a Map wrapper.
    fn as_v8_map(&self) -> PyResult<Option<V8Map>> {
        if !self.is_map() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::Map>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Map.")
            })?;

            Ok(Some(V8Map::from_local(scope, value, self.handle.clone())))
        })
    }

    /// Return this value as a Map wrapper.
    fn as_map(&self) -> PyResult<Option<V8Map>> {
        self.as_v8_map()
    }

    /// Return this value as a Set wrapper.
    fn as_v8_set(&self) -> PyResult<Option<V8Set>> {
        if !self.is_set() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::Set>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Set.")
            })?;

            Ok(Some(V8Set::from_local(scope, value, self.handle.clone())))
        })
    }

    /// Return this value as a Set wrapper.
    fn as_set(&self) -> PyResult<Option<V8Set>> {
        self.as_v8_set()
    }

    /// Return this value as a Date wrapper.
    fn as_v8_date(&self) -> PyResult<Option<V8Date>> {
        if !self.is_date() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::Date>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Date.")
            })?;

            Ok(Some(V8Date::from_local(scope, value, self.handle.clone())))
        })
    }

    /// Return this value as a Date wrapper.
    fn as_date(&self) -> PyResult<Option<V8Date>> {
        self.as_v8_date()
    }

    /// Return this value as a RegExp wrapper.
    fn as_v8_regexp(&self) -> PyResult<Option<V8RegExp>> {
        if !self.is_regexp() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::RegExp>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to RegExp.")
            })?;

            Ok(Some(V8RegExp::from_local(
                scope,
                value,
                self.handle.clone(),
            )))
        })
    }

    /// Return this value as a RegExp wrapper.
    fn as_regexp(&self) -> PyResult<Option<V8RegExp>> {
        self.as_v8_regexp()
    }

    /// Return this value as a Proxy wrapper.
    fn as_v8_proxy(&self) -> PyResult<Option<V8Proxy>> {
        if !self.is_proxy() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::Proxy>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Proxy.")
            })?;

            Ok(Some(V8Proxy::from_local(scope, value, self.handle.clone())))
        })
    }

    /// Return this value as a Proxy wrapper.
    fn as_proxy(&self) -> PyResult<Option<V8Proxy>> {
        self.as_v8_proxy()
    }

    /// Return this value as an External wrapper.
    fn as_v8_external(&self) -> PyResult<Option<V8External>> {
        if !self.is_external() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::External>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to External.")
            })?;

            Ok(Some(V8External::from_local(
                scope,
                value,
                self.handle.clone(),
            )))
        })
    }

    /// Return this value as an External wrapper.
    fn as_external(&self) -> PyResult<Option<V8External>> {
        self.as_v8_external()
    }

    /// Return this value as a WasmModule wrapper.
    fn as_v8_wasm_module(&self) -> PyResult<Option<V8WasmModule>> {
        if !self.is_wasm_module() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::WasmModuleObject>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to WasmModule.")
            })?;

            Ok(Some(V8WasmModule::from_local(
                scope,
                value,
                self.handle.clone(),
            )))
        })
    }

    /// Return this value as a WasmModule wrapper.
    fn as_wasm_module(&self) -> PyResult<Option<V8WasmModule>> {
        self.as_v8_wasm_module()
    }

    /// Return this value as a BigInt wrapper.
    fn as_v8_big_int(&self) -> PyResult<Option<V8BigInt>> {
        if !self.is_big_int() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::BigInt>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to BigInt.")
            })?;

            Ok(Some(V8BigInt::from_local(
                scope,
                value,
                self.handle.clone(),
            )))
        })
    }

    /// Return this value as a Symbol wrapper.
    fn as_v8_symbol(&self) -> PyResult<Option<V8Symbol>> {
        if !self.is_symbol() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::Symbol>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Symbol.")
            })?;

            Ok(Some(V8Symbol::from_local(
                scope,
                value,
                self.handle.clone(),
            )))
        })
    }

    /// Return this value as an ArrayBuffer wrapper.
    fn as_v8_array_buffer(&self) -> PyResult<Option<V8ArrayBuffer>> {
        if !self.is_array_buffer() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::ArrayBuffer>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to ArrayBuffer.")
            })?;

            Ok(Some(V8ArrayBuffer::from_local(
                scope,
                value,
                self.handle.clone(),
            )))
        })
    }

    /// Return this value as an ArrayBufferView wrapper.
    fn as_v8_array_buffer_view(&self) -> PyResult<Option<V8ArrayBufferView>> {
        if !self.is_array_buffer_view() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::ArrayBufferView>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "Failed to cast value to ArrayBufferView.",
                )
            })?;

            Ok(Some(V8ArrayBufferView::from_local(
                scope,
                value,
                self.handle.clone(),
            )))
        })
    }

    /// Return this value as a TypedArray wrapper.
    fn as_v8_typed_array(&self) -> PyResult<Option<V8TypedArray>> {
        if !self.is_typed_array() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::TypedArray>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to TypedArray.")
            })?;

            Ok(Some(V8TypedArray::from_local(
                scope,
                value,
                self.handle.clone(),
            )))
        })
    }

    /// Return this value as a DataView wrapper.
    fn as_v8_data_view(&self) -> PyResult<Option<V8DataView>> {
        if !self.is_data_view() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let value = v8::Local::<v8::DataView>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to DataView.")
            })?;

            Ok(Some(V8DataView::from_local(
                scope,
                value,
                self.handle.clone(),
            )))
        })
    }

    /// Read an object property after converting this value to an object.
    fn get(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        self.handle.with_local_value(|scope, value| {
            let object = value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;
            let key = python_to_v8(key.py(), scope, key, self.handle.isolate_id, 0)?;
            let result = object.get(scope, key).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to get object property.")
            })?;

            Ok(Self::from_local(
                scope,
                result,
                self.handle.context.clone(),
                self.handle.isolate.clone(),
                self.handle.isolate_id,
            ))
        })
    }

    /// Read an indexed object property.
    fn get_index(&self, index: u32) -> PyResult<Value> {
        self.handle.with_local_value(|scope, value| {
            let object = value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;
            let result = object.get_index(scope, index).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to get array item.")
            })?;

            Ok(Self::from_local(
                scope,
                result,
                self.handle.context.clone(),
                self.handle.isolate.clone(),
                self.handle.isolate_id,
            ))
        })
    }

    /// Set an object property after converting this value to an object.
    fn set(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<bool> {
        self.handle.with_local_value(|scope, local_value| {
            let object = local_value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;
            let key = python_to_v8(key.py(), scope, key, self.handle.isolate_id, 0)?;
            let value = python_to_v8(value.py(), scope, value, self.handle.isolate_id, 0)?;

            object.set(scope, key, value).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to set object property.")
            })
        })
    }

    /// Set an indexed object property.
    fn set_index(
        &self,
        index: u32,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<bool> {
        self.handle.with_local_value(|scope, local_value| {
            let object = local_value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;
            let value = python_to_v8(value.py(), scope, value, self.handle.isolate_id, 0)?;

            object.set_index(scope, index, value).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to set array item.")
            })
        })
    }

    /// Return whether this value's object form has a property.
    fn has(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.handle.with_local_value(|scope, value| {
            let object = value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;
            let key = python_to_v8(key.py(), scope, key, self.handle.isolate_id, 0)?;

            object.has(scope, key).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to test object property.")
            })
        })
    }

    /// Delete an object property after converting this value to an object.
    fn delete(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.handle.with_local_value(|scope, value| {
            let object = value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;
            let key = python_to_v8(key.py(), scope, key, self.handle.isolate_id, 0)?;

            object.delete(scope, key).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to delete object property.")
            })
        })
    }

    /// Read an object property.
    fn __getitem__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        self.get(key)
    }

    /// Set an object property.
    fn __setitem__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<()> {
        self.set(key, value).map(|_| ())
    }

    /// Delete an object property.
    fn __delitem__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.delete(key).map(|_| ())
    }

    /// Return whether this value's object form has a property.
    fn __contains__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.has(key)
    }

    /// Return this object's own property names.
    #[gen_stub(override_return_type(type_repr = "list[object]", imports = ()))]
    fn keys(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.handle.with_local_value(|scope, value| {
            let object = value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;
            let names = object
                .get_own_property_names(scope, Default::default())
                .ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to read object keys.")
                })?;

            value_to_python(py, scope, names.into(), 0)
        })
    }

    #[pyo3(signature = (key, value, attributes=None, *, read_only=false, dont_enum=false, dont_delete=false))]
    /// Define an own data property with optional V8 attributes.
    fn define_own_property(
        &self,
        #[gen_stub(override_type(type_repr = "_JSPropertyNameLike", imports = ()))] key: &Bound<
            '_,
            PyAny,
        >,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
        #[gen_stub(override_type(type_repr = "PropertyAttribute | None", imports = ()))]
        attributes: Option<&Bound<'_, PyAny>>,
        read_only: bool,
        dont_enum: bool,
        dont_delete: bool,
    ) -> PyResult<bool> {
        let attribute =
            property_attribute_from_python(attributes, read_only, dont_enum, dont_delete)?;

        self.handle.with_local_value(|scope, local_value| {
            let object = local_value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;

            define_own_property_on_object(
                key.py(),
                scope,
                object,
                key,
                value,
                attribute,
                self.handle.isolate_id,
            )
        })
    }

    /// Define a property using a full property descriptor.
    fn define_property(
        &self,
        #[gen_stub(override_type(type_repr = "_JSPropertyNameLike", imports = ()))] key: &Bound<
            '_,
            PyAny,
        >,
        descriptor: PyRef<'_, PropertyDescriptor>,
    ) -> PyResult<bool> {
        self.handle.with_local_value(|scope, local_value| {
            let object = local_value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;

            define_property_on_object(
                key.py(),
                scope,
                object,
                key,
                descriptor,
                self.handle.isolate_id,
            )
        })
    }

    /// Return the own property descriptor for a key.
    fn get_own_property_descriptor(
        &self,
        py: Python<'_>,
        #[gen_stub(override_type(type_repr = "_JSPropertyNameLike", imports = ()))] key: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<Option<PropertyDescriptor>> {
        self.handle.with_local_value(|scope, local_value| {
            let object = local_value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;

            get_own_property_descriptor_on_object(py, scope, object, key, &self.handle)
        })
    }

    /// Return V8 property attributes for a key.
    fn get_property_attributes(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<PropertyAttribute> {
        self.handle.with_local_value(|scope, local_value| {
            let object = local_value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;

            get_property_attributes_on_object(key.py(), scope, object, key, self.handle.isolate_id)
        })
    }

    /// Set this object's integrity level to "sealed" or "frozen".
    fn set_integrity_level(&self, level: &str) -> PyResult<bool> {
        self.handle.with_local_value(|scope, local_value| {
            let object = local_value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;

            set_integrity_level_on_object(scope, object, level)
        })
    }

    /// Freeze this object value.
    fn freeze(&self) -> PyResult<bool> {
        self.set_integrity_level("frozen")
    }

    /// Seal this object value.
    fn seal(&self) -> PyResult<bool> {
        self.set_integrity_level("sealed")
    }

    /// Read a private property.
    fn get_private(&self, key: PyRef<'_, V8Private>) -> PyResult<Value> {
        self.handle.with_local_value(|scope, value| {
            let object = value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;

            get_private_on_object(scope, object, &key, &self.handle)
        })
    }

    /// Set a private property.
    fn set_private(
        &self,
        key: PyRef<'_, V8Private>,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<bool> {
        self.handle.with_local_value(|scope, local_value| {
            let object = local_value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;

            set_private_on_object(value.py(), scope, object, &key, value, &self.handle)
        })
    }

    /// Return whether a private property exists.
    fn has_private(&self, key: PyRef<'_, V8Private>) -> PyResult<bool> {
        self.handle.with_local_value(|scope, value| {
            let object = value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;

            has_private_on_object(scope, object, &key, &self.handle)
        })
    }

    /// Delete a private property.
    fn delete_private(&self, key: PyRef<'_, V8Private>) -> PyResult<bool> {
        self.handle.with_local_value(|scope, value| {
            let object = value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;

            delete_private_on_object(scope, object, &key, &self.handle)
        })
    }

    /// Return the number of internal fields on this value's object form.
    fn internal_field_count(&self) -> PyResult<usize> {
        self.handle.with_local_value(|scope, value| {
            let object = value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;

            Ok(object.internal_field_count())
        })
    }

    #[gen_stub(override_return_type(type_repr = "Value | Private | None", imports = ()))]
    /// Return an internal field value.
    fn get_internal_field(&self, py: Python<'_>, index: usize) -> PyResult<Option<Py<PyAny>>> {
        self.handle.with_local_value(|scope, value| {
            let object = value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;

            get_internal_field_on_object(py, scope, object, index, &self.handle)
        })
    }

    /// Set an internal field value.
    fn set_internal_field(
        &self,
        index: usize,
        #[gen_stub(override_type(type_repr = "object", imports = ()))] data: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.handle.with_local_value(|scope, value| {
            let object = value.to_object(scope).ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err("Value cannot be converted to object.")
            })?;

            set_internal_field_on_object(data.py(), scope, object, index, data, &self.handle)
        })
    }

    /// Return this value's natural length when it has one.
    fn length(&self) -> PyResult<Option<u32>> {
        self.handle.with_local_value(|scope, value| {
            if value.is_array() {
                let array = v8::Local::<v8::Array>::try_from(value).map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Array.")
                })?;
                return Ok(Some(array.length()));
            }

            if value.is_string() {
                let string = value.to_string(scope).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to string.")
                })?;
                return u32::try_from(string.length()).map(Some).map_err(|_| {
                    pyo3::exceptions::PyOverflowError::new_err("String length exceeds u32.")
                });
            }

            if value.is_array_buffer() {
                let array_buffer = v8::Local::<v8::ArrayBuffer>::try_from(value).map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err(
                        "Failed to cast value to ArrayBuffer.",
                    )
                })?;

                return u32::try_from(array_buffer.byte_length())
                    .map(Some)
                    .map_err(|_| {
                        pyo3::exceptions::PyOverflowError::new_err(
                            "ArrayBuffer byte_length exceeds u32.",
                        )
                    });
            }

            if value.is_map() {
                let map = v8::Local::<v8::Map>::try_from(value).map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Map.")
                })?;

                return u32::try_from(map.size()).map(Some).map_err(|_| {
                    pyo3::exceptions::PyOverflowError::new_err("Map size exceeds u32.")
                });
            }

            if value.is_set() {
                let set = v8::Local::<v8::Set>::try_from(value).map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Set.")
                })?;

                return u32::try_from(set.size()).map(Some).map_err(|_| {
                    pyo3::exceptions::PyOverflowError::new_err("Set size exceeds u32.")
                });
            }

            if value.is_typed_array() {
                let typed_array = v8::Local::<v8::TypedArray>::try_from(value).map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to TypedArray.")
                })?;

                return u32::try_from(typed_array.length()).map(Some).map_err(|_| {
                    pyo3::exceptions::PyOverflowError::new_err("TypedArray length exceeds u32.")
                });
            }

            if value.is_array_buffer_view() {
                let view = v8::Local::<v8::ArrayBufferView>::try_from(value).map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err(
                        "Failed to cast value to ArrayBufferView.",
                    )
                })?;

                return u32::try_from(view.byte_length()).map(Some).map_err(|_| {
                    pyo3::exceptions::PyOverflowError::new_err(
                        "ArrayBufferView byte_length exceeds u32.",
                    )
                });
            }

            Ok(None)
        })
    }

    /// Return this value's natural length.
    fn __len__(&self) -> PyResult<usize> {
        self.handle.with_local_value(|scope, value| {
            if value.is_array() {
                let array = v8::Local::<v8::Array>::try_from(value).map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Array.")
                })?;

                return usize::try_from(array.length()).map_err(|_| {
                    pyo3::exceptions::PyOverflowError::new_err("Array length exceeds usize.")
                });
            }

            if value.is_string() {
                let string = value.to_string(scope).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to string.")
                })?;

                return Ok(string.length());
            }

            if value.is_array_buffer() {
                let array_buffer = v8::Local::<v8::ArrayBuffer>::try_from(value).map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err(
                        "Failed to cast value to ArrayBuffer.",
                    )
                })?;

                return Ok(array_buffer.byte_length());
            }

            if value.is_map() {
                let map = v8::Local::<v8::Map>::try_from(value).map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Map.")
                })?;

                return Ok(map.size());
            }

            if value.is_set() {
                let set = v8::Local::<v8::Set>::try_from(value).map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Set.")
                })?;

                return Ok(set.size());
            }

            if value.is_typed_array() {
                let typed_array = v8::Local::<v8::TypedArray>::try_from(value).map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to TypedArray.")
                })?;

                return Ok(typed_array.length());
            }

            if value.is_array_buffer_view() {
                let view = v8::Local::<v8::ArrayBufferView>::try_from(value).map_err(|_| {
                    pyo3::exceptions::PyRuntimeError::new_err(
                        "Failed to cast value to ArrayBufferView.",
                    )
                })?;

                return Ok(view.byte_length());
            }

            if value.is_object() {
                let object = value.to_object(scope).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to object.")
                })?;
                let names = object
                    .get_own_property_names(scope, Default::default())
                    .ok_or_else(|| {
                        pyo3::exceptions::PyRuntimeError::new_err("Failed to read object keys.")
                    })?;

                return usize::try_from(names.length()).map_err(|_| {
                    pyo3::exceptions::PyOverflowError::new_err("Object key count exceeds usize.")
                });
            }

            Err(pyo3::exceptions::PyTypeError::new_err(
                "Value has no length.",
            ))
        })
    }

    /// Evaluate JavaScript instanceof against a constructor value.
    fn instance_of(&self, constructor: PyRef<'_, Value>) -> PyResult<bool> {
        constructor.handle.ensure_isolate(self.handle.isolate_id)?;

        self.handle.with_local_value(|scope, value| {
            let constructor = v8::Local::new(scope, &constructor.handle.value);
            let constructor = v8::Local::<v8::Object>::try_from(constructor).map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err("Constructor value is not an object.")
            })?;

            value.instance_of(scope, constructor).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to evaluate instanceof.")
            })
        })
    }

    /// Apply JavaScript unary plus.
    fn __pos__(&self) -> PyResult<Value> {
        operator::apply_unary_value_operator(&self.handle, UnaryOperator::Pos)
    }

    /// Apply JavaScript unary negation.
    fn __neg__(&self) -> PyResult<Value> {
        operator::apply_unary_value_operator(&self.handle, UnaryOperator::Neg)
    }

    /// Apply JavaScript bitwise not.
    fn __invert__(&self) -> PyResult<Value> {
        operator::apply_unary_value_operator(&self.handle, UnaryOperator::Invert)
    }

    /// Apply JavaScript addition.
    fn __add__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Add,
            false,
        )
    }

    /// Apply JavaScript reflected addition.
    fn __radd__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Add,
            true,
        )
    }

    /// Apply JavaScript subtraction.
    fn __sub__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Sub,
            false,
        )
    }

    /// Apply JavaScript reflected subtraction.
    fn __rsub__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Sub,
            true,
        )
    }

    /// Apply JavaScript multiplication.
    fn __mul__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Mul,
            false,
        )
    }

    /// Apply JavaScript reflected multiplication.
    fn __rmul__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Mul,
            true,
        )
    }

    /// Apply JavaScript division.
    fn __truediv__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Div,
            false,
        )
    }

    /// Apply JavaScript reflected division.
    fn __rtruediv__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Div,
            true,
        )
    }

    /// Apply JavaScript remainder.
    fn __mod__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Rem,
            false,
        )
    }

    /// Apply JavaScript reflected remainder.
    fn __rmod__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Rem,
            true,
        )
    }

    /// Apply JavaScript exponentiation.
    fn __pow__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        exponent: &Bound<'_, PyAny>,
        #[gen_stub(override_type(type_repr = "None", imports = ()))] modulo: Option<
            &Bound<'_, PyAny>,
        >,
    ) -> PyResult<Value> {
        if modulo.is_some() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "JavaScript exponentiation does not support a modulo argument.",
            ));
        }

        operator::apply_binary_value_operator(
            &self.handle,
            exponent.py(),
            exponent,
            BinaryOperator::Pow,
            false,
        )
    }

    /// Apply JavaScript reflected exponentiation.
    fn __rpow__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        base: &Bound<'_, PyAny>,
        #[gen_stub(override_type(type_repr = "None", imports = ()))] modulo: Option<
            &Bound<'_, PyAny>,
        >,
    ) -> PyResult<Value> {
        if modulo.is_some() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "JavaScript exponentiation does not support a modulo argument.",
            ));
        }

        operator::apply_binary_value_operator(
            &self.handle,
            base.py(),
            base,
            BinaryOperator::Pow,
            true,
        )
    }

    /// Apply JavaScript bitwise and.
    fn __and__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::BitAnd,
            false,
        )
    }

    /// Apply JavaScript reflected bitwise and.
    fn __rand__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::BitAnd,
            true,
        )
    }

    /// Apply JavaScript bitwise or.
    fn __or__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::BitOr,
            false,
        )
    }

    /// Apply JavaScript reflected bitwise or.
    fn __ror__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::BitOr,
            true,
        )
    }

    /// Apply JavaScript bitwise xor.
    fn __xor__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::BitXor,
            false,
        )
    }

    /// Apply JavaScript reflected bitwise xor.
    fn __rxor__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::BitXor,
            true,
        )
    }

    /// Apply JavaScript left shift.
    fn __lshift__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Shl,
            false,
        )
    }

    /// Apply JavaScript reflected left shift.
    fn __rlshift__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Shl,
            true,
        )
    }

    /// Apply JavaScript right shift.
    fn __rshift__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Shr,
            false,
        )
    }

    /// Apply JavaScript reflected right shift.
    fn __rrshift__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        operator::apply_binary_value_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Shr,
            true,
        )
    }

    /// Apply JavaScript loose equality.
    fn __eq__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        match operator::apply_binary_bool_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Eq,
        ) {
            Ok(value) => Ok(value),
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyTypeError>(other.py()) => {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    /// Apply JavaScript loose inequality.
    fn __ne__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        match operator::apply_binary_bool_operator(
            &self.handle,
            other.py(),
            other,
            BinaryOperator::Ne,
        ) {
            Ok(value) => Ok(value),
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyTypeError>(other.py()) => {
                Ok(true)
            }
            Err(error) => Err(error),
        }
    }

    /// Apply JavaScript less-than comparison.
    fn __lt__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        operator::apply_binary_bool_operator(&self.handle, other.py(), other, BinaryOperator::Lt)
    }

    /// Apply JavaScript less-than-or-equal comparison.
    fn __le__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        operator::apply_binary_bool_operator(&self.handle, other.py(), other, BinaryOperator::Le)
    }

    /// Apply JavaScript greater-than comparison.
    fn __gt__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        operator::apply_binary_bool_operator(&self.handle, other.py(), other, BinaryOperator::Gt)
    }

    /// Apply JavaScript greater-than-or-equal comparison.
    fn __ge__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ("collections.abc")))]
        other: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        operator::apply_binary_bool_operator(&self.handle, other.py(), other, BinaryOperator::Ge)
    }

    /// Return the promise state when this value is a Promise.
    fn promise_state(&self) -> PyResult<Option<&'static str>> {
        if !self.is_promise() {
            return Ok(None);
        }

        self.handle.with_local_value(|_, value| {
            let promise = v8::Local::<v8::Promise>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Promise.")
            })?;

            Ok(Some(super::kind::promise_state_name(&promise.state())))
        })
    }

    /// Return the settled promise result, or None when pending or not a Promise.
    fn promise_result(&self) -> PyResult<Option<Value>> {
        if !self.is_promise() {
            return Ok(None);
        }

        self.handle.with_local_value(|scope, value| {
            let promise = v8::Local::<v8::Promise>::try_from(value).map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Promise.")
            })?;

            if promise.state() == v8::PromiseState::Pending {
                return Ok(None);
            }

            let result = promise.result(scope);

            Ok(Some(Self::from_local(
                scope,
                result,
                self.handle.context.clone(),
                self.handle.isolate.clone(),
                self.handle.isolate_id,
            )))
        })
    }

    #[gen_stub(override_return_type(type_repr = "collections.abc.Generator[object, None, Value]", imports = ("collections.abc")))]
    /// Await this value when it is a JavaScript Promise.
    fn __await__(&self) -> PyResult<PromiseAwaiter> {
        if !self.is_promise() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "Only JavaScript Promise values can be awaited.",
            ));
        }

        Ok(PromiseAwaiter::new(self.handle.clone()))
    }

    #[pyo3(signature = (args=None, this_arg=None))]
    /// Call this value when it is a JavaScript function.
    fn call(
        &self,
        #[gen_stub(override_type(type_repr = "_JSFunctionArgsLike | None", imports = ()))]
        args: Option<&Bound<'_, PyAny>>,
        #[gen_stub(override_type(type_repr = "_JSValueLike | None", imports = ()))]
        this_arg: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Value> {
        if !self.is_function() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "Value is not a function.",
            ));
        }

        let mut isolate = self.handle.isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate));
        let scope = &mut scope.init();
        let context = v8::Local::new(scope, &self.handle.context);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(let scope, &mut **scope);
        let value = v8::Local::new(scope, &self.handle.value);
        let function = v8::Local::<v8::Function>::try_from(value).map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to cast value to Function.")
        })?;
        let recv = if let Some(this_arg) = this_arg {
            python_to_v8(this_arg.py(), scope, this_arg, self.handle.isolate_id, 0)?
        } else {
            context.global(scope).into()
        };
        let args = python_args_to_v8(args, scope, self.handle.isolate_id)?;
        let result = function
            .call(scope, recv, &args)
            .ok_or_else(|| js_exception(scope, "Function execution failed."))?;

        Ok(Self::from_local(
            scope,
            result,
            self.handle.context.clone(),
            self.handle.isolate.clone(),
            self.handle.isolate_id,
        ))
    }

    #[pyo3(signature = (*args))]
    /// Call this value with positional arguments.
    fn __call__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] args: &Bound<
            '_,
            PyTuple,
        >,
    ) -> PyResult<Value> {
        self.call(Some(args.as_any()), None)
    }

    /// Convert this JavaScript value to a Python object.
    #[gen_stub(override_return_type(type_repr = "object", imports = ()))]
    fn to_python(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.handle
            .with_local_value(|scope, value| value_to_python(py, scope, value, 0))
    }

    /// Serialize this value with JSON.stringify.
    fn to_json(&self) -> PyResult<Option<String>> {
        self.handle.with_local_value(|scope, value| {
            let Some(json) = v8::json::stringify(scope, value) else {
                return Ok(None);
            };

            Ok(Some(json.to_rust_string_lossy(scope)))
        })
    }

    /// Return a debug representation.
    fn __repr__(&self) -> PyResult<String> {
        Ok(format!("<v8.Value kind='{}'>", self.kind()))
    }

    /// Convert this value using JavaScript's string conversion.
    fn __str__(&self) -> PyResult<String> {
        self.to_string()
    }
}

impl Value {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Value>,
        context: v8::Global<v8::Context>,
        isolate: SharedIsolate,
        isolate_id: u64,
    ) -> Self {
        Self {
            handle: V8Value::from_local(scope, value, context, isolate, isolate_id),
        }
    }

    pub(crate) fn from_handle(handle: V8Value) -> Self {
        Self { handle }
    }
}
