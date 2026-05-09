use pyo3::prelude::{Bound, Py, PyAny, PyRef, PyResult, Python, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyBytes, PyList, PyMemoryView, PyTuple};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use super::PromiseAwaiter;
use super::convert::{
    bigint_to_decimal_string, date_to_python, python_args_to_v8, python_to_v8, symbol_to_string,
    value_to_python,
};
use super::embedder::{
    V8Private, delete_private_on_object, get_internal_field_on_object, get_private_on_object,
    has_private_on_object, set_internal_field_on_object, set_private_on_object,
};
use super::handle::V8Value;
use super::kind::promise_state_name;
use super::property::{
    PropertyAttribute, PropertyDescriptor, define_own_property_on_object,
    define_property_on_object, get_own_property_descriptor_on_object,
    get_property_attributes_on_object, property_attribute_from_python,
    set_integrity_level_on_object,
};
use super::value::Value;
use crate::error::js_exception;
use crate::runtime::SharedIsolate;

#[derive(Clone)]
struct TypedValue<T> {
    value: v8::Global<T>,
    handle: V8Value,
}

impl<T> TypedValue<T> {
    fn new(value: v8::Global<T>, handle: V8Value) -> Self {
        Self { value, handle }
    }

    fn to_value(&self) -> Value {
        Value::from_handle(self.handle.clone())
    }

    fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.handle.ensure_isolate(isolate_id)
    }

    fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        v8::Local::new(scope, &self.handle.value)
    }

    fn with_local<R>(
        &self,
        f: impl for<'s> FnOnce(&v8::PinScope<'s, '_>, v8::Local<'s, T>) -> PyResult<R>,
    ) -> PyResult<R> {
        self.handle.with_local_value(|scope, _| {
            let value = v8::Local::new(scope, &self.value);

            f(scope, value)
        })
    }
}

fn typed_value_to_string(
    scope: &v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
) -> PyResult<String> {
    let string = value.to_string(scope).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to string.")
    })?;

    Ok(string.to_rust_string_lossy(scope))
}

fn value_from_typed_local<'s>(
    scope: &v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
    handle: &V8Value,
) -> Value {
    Value::from_local(
        scope,
        value,
        handle.context.clone(),
        handle.isolate.clone(),
        handle.isolate_id,
    )
}

fn py_iterator(py: Python<'_>, values: Vec<Value>) -> PyResult<Py<PyAny>> {
    let values = PyList::new(py, values)?;

    Ok(values.call_method0("__iter__")?.unbind())
}

fn string_property(
    scope: &v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
    name: &str,
) -> PyResult<String> {
    let object = value.to_object(scope).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to convert value to object.")
    })?;
    let key = v8::String::new(scope, name).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create property name.")
    })?;
    let value = object.get(scope, key.into()).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to read object property.")
    })?;

    typed_value_to_string(scope, value)
}

/// JavaScript String wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "String", unsendable)]
pub(crate) struct V8String {
    pub(crate) value: v8::Global<v8::String>,
    pub(crate) text: String,
    pub(crate) isolate: SharedIsolate,
    pub(crate) isolate_id: u64,
    pub(crate) handle: Option<V8Value>,
}

#[gen_stub_pymethods]
#[pymethods]
impl V8String {
    /// Return the string contents.
    #[getter]
    fn value(&self) -> String {
        let mut isolate = self.isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate));
        let scope = &mut scope.init();
        let value = v8::Local::new(scope, &self.value);

        value.to_rust_string_lossy(scope)
    }

    /// Return this string as a generic V8 Value.
    fn to_value(&self) -> PyResult<Value> {
        let Some(handle) = &self.handle else {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "String was created without a Context and cannot be converted to Value.",
            ));
        };

        Ok(Value::from_handle(handle.clone()))
    }

    /// Return the string contents.
    fn __str__(&self) -> String {
        self.value()
    }

    /// Return the number of Unicode scalar values in this string.
    fn __len__(&self) -> usize {
        self.value().chars().count()
    }
}

impl V8String {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::String>,
        text: String,
        isolate: SharedIsolate,
        isolate_id: u64,
        handle: Option<V8Value>,
    ) -> Self {
        Self {
            value: v8::Global::new(scope, value),
            text,
            isolate,
            isolate_id,
            handle,
        }
    }

    pub(crate) fn from_value_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::String>,
        handle: V8Value,
    ) -> Self {
        Self::from_local(
            scope,
            value,
            value.to_rust_string_lossy(scope),
            handle.isolate.clone(),
            handle.isolate_id,
            Some(handle),
        )
    }

    pub(crate) fn from_context_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::String>,
        text: String,
        context: v8::Global<v8::Context>,
        isolate: SharedIsolate,
        isolate_id: u64,
    ) -> Self {
        let handle = V8Value::from_local(scope, value.into(), context, isolate.clone(), isolate_id);

        Self::from_local(scope, value, text, isolate, isolate_id, Some(handle))
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        if self.isolate_id == isolate_id {
            return Ok(());
        }

        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "V8 String belongs to a different Isolate.",
        ))
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        if let Some(handle) = &self.handle {
            return v8::Local::new(scope, &handle.value);
        }

        let value = v8::Local::new(scope, &self.value);
        value.into()
    }
}

/// JavaScript Object wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "Object", unsendable)]
pub(crate) struct V8Object {
    object: TypedValue<v8::Object>,
}

/// JavaScript Array wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "Array", unsendable)]
pub(crate) struct V8Array {
    array: TypedValue<v8::Array>,
}

/// JavaScript Function wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "Function", unsendable)]
pub(crate) struct V8Function {
    function: TypedValue<v8::Function>,
    can_create_code_cache: bool,
    cached_data_rejected: bool,
}

/// JavaScript Promise wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "Promise", unsendable)]
pub(crate) struct V8Promise {
    promise: TypedValue<v8::Promise>,
}

/// JavaScript BigInt wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "BigInt", unsendable)]
pub(crate) struct V8BigInt {
    bigint: TypedValue<v8::BigInt>,
}

/// JavaScript Symbol wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "Symbol", unsendable)]
pub(crate) struct V8Symbol {
    symbol: TypedValue<v8::Symbol>,
}

/// JavaScript ArrayBuffer wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "ArrayBuffer", unsendable)]
pub(crate) struct V8ArrayBuffer {
    array_buffer: TypedValue<v8::ArrayBuffer>,
}

/// JavaScript ArrayBufferView wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "ArrayBufferView", unsendable)]
pub(crate) struct V8ArrayBufferView {
    view: TypedValue<v8::ArrayBufferView>,
}

/// JavaScript TypedArray wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "TypedArray", unsendable)]
pub(crate) struct V8TypedArray {
    typed_array: TypedValue<v8::TypedArray>,
}

/// JavaScript DataView wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "DataView", unsendable)]
pub(crate) struct V8DataView {
    data_view: TypedValue<v8::DataView>,
}

/// JavaScript Map wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "Map", unsendable)]
pub(crate) struct V8Map {
    map: TypedValue<v8::Map>,
}

/// JavaScript Set wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "Set", unsendable)]
pub(crate) struct V8Set {
    set: TypedValue<v8::Set>,
}

/// JavaScript Date wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "Date", unsendable)]
pub(crate) struct V8Date {
    date: TypedValue<v8::Date>,
}

/// JavaScript RegExp wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "RegExp", unsendable)]
pub(crate) struct V8RegExp {
    regexp: TypedValue<v8::RegExp>,
}

/// JavaScript Proxy wrapper.
#[gen_stub_pyclass]
#[pyclass(name = "Proxy", unsendable)]
pub(crate) struct V8Proxy {
    proxy: TypedValue<v8::Proxy>,
    can_revoke: bool,
}

impl V8Object {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Object>,
        handle: V8Value,
    ) -> Self {
        Self {
            object: TypedValue::new(v8::Global::new(scope, value), handle),
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.object.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.object.local_value(scope)
    }
}

impl V8Array {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Array>,
        handle: V8Value,
    ) -> Self {
        Self {
            array: TypedValue::new(v8::Global::new(scope, value), handle),
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.array.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.array.local_value(scope)
    }
}

impl V8Function {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Function>,
        handle: V8Value,
    ) -> Self {
        Self {
            function: TypedValue::new(v8::Global::new(scope, value), handle),
            can_create_code_cache: false,
            cached_data_rejected: false,
        }
    }

    pub(crate) fn from_compiled_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Function>,
        handle: V8Value,
        cached_data_rejected: bool,
    ) -> Self {
        Self {
            function: TypedValue::new(v8::Global::new(scope, value), handle),
            can_create_code_cache: true,
            cached_data_rejected,
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.function.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.function.local_value(scope)
    }
}

impl V8Promise {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Promise>,
        handle: V8Value,
    ) -> Self {
        Self {
            promise: TypedValue::new(v8::Global::new(scope, value), handle),
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.promise.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.promise.local_value(scope)
    }
}

impl V8BigInt {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::BigInt>,
        handle: V8Value,
    ) -> Self {
        Self {
            bigint: TypedValue::new(v8::Global::new(scope, value), handle),
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.bigint.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.bigint.local_value(scope)
    }
}

impl V8Symbol {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Symbol>,
        handle: V8Value,
    ) -> Self {
        Self {
            symbol: TypedValue::new(v8::Global::new(scope, value), handle),
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.symbol.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.symbol.local_value(scope)
    }
}

impl V8ArrayBuffer {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::ArrayBuffer>,
        handle: V8Value,
    ) -> Self {
        Self {
            array_buffer: TypedValue::new(v8::Global::new(scope, value), handle),
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.array_buffer.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.array_buffer.local_value(scope)
    }
}

impl V8ArrayBufferView {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::ArrayBufferView>,
        handle: V8Value,
    ) -> Self {
        Self {
            view: TypedValue::new(v8::Global::new(scope, value), handle),
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.view.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.view.local_value(scope)
    }
}

impl V8TypedArray {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::TypedArray>,
        handle: V8Value,
    ) -> Self {
        Self {
            typed_array: TypedValue::new(v8::Global::new(scope, value), handle),
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.typed_array.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.typed_array.local_value(scope)
    }
}

impl V8DataView {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::DataView>,
        handle: V8Value,
    ) -> Self {
        Self {
            data_view: TypedValue::new(v8::Global::new(scope, value), handle),
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.data_view.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.data_view.local_value(scope)
    }
}

impl V8Map {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Map>,
        handle: V8Value,
    ) -> Self {
        Self {
            map: TypedValue::new(v8::Global::new(scope, value), handle),
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.map.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.map.local_value(scope)
    }
}

impl V8Set {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Set>,
        handle: V8Value,
    ) -> Self {
        Self {
            set: TypedValue::new(v8::Global::new(scope, value), handle),
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.set.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.set.local_value(scope)
    }
}

impl V8Date {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Date>,
        handle: V8Value,
    ) -> Self {
        Self {
            date: TypedValue::new(v8::Global::new(scope, value), handle),
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.date.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.date.local_value(scope)
    }
}

impl V8RegExp {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::RegExp>,
        handle: V8Value,
    ) -> Self {
        Self {
            regexp: TypedValue::new(v8::Global::new(scope, value), handle),
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.regexp.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.regexp.local_value(scope)
    }
}

impl V8Proxy {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Proxy>,
        handle: V8Value,
    ) -> Self {
        Self {
            proxy: TypedValue::new(v8::Global::new(scope, value), handle),
            can_revoke: false,
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.proxy.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        self.proxy.local_value(scope)
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8Object {
    /// Return this object as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.object.to_value()
    }

    /// Return the number of internal fields on this object.
    #[getter]
    fn internal_field_count(&self) -> PyResult<usize> {
        self.object
            .with_local(|_, object| Ok(object.internal_field_count()))
    }

    /// Read an object property.
    fn get(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        self.object.with_local(|scope, object| {
            let key = python_to_v8(key.py(), scope, key, self.object.handle.isolate_id, 0)?;
            let result = object.get(scope, key).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to get object property.")
            })?;

            Ok(Value::from_local(
                scope,
                result,
                self.object.handle.context.clone(),
                self.object.handle.isolate.clone(),
                self.object.handle.isolate_id,
            ))
        })
    }

    /// Set an object property.
    fn set(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<bool> {
        self.object.with_local(|scope, object| {
            let key = python_to_v8(key.py(), scope, key, self.object.handle.isolate_id, 0)?;
            let value = python_to_v8(value.py(), scope, value, self.object.handle.isolate_id, 0)?;

            object.set(scope, key, value).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to set object property.")
            })
        })
    }

    /// Return whether this object has a property.
    fn has(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.object.with_local(|scope, object| {
            let key = python_to_v8(key.py(), scope, key, self.object.handle.isolate_id, 0)?;

            object.has(scope, key).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to test object property.")
            })
        })
    }

    /// Delete an object property.
    fn delete(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.object.with_local(|scope, object| {
            let key = python_to_v8(key.py(), scope, key, self.object.handle.isolate_id, 0)?;

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

    /// Return whether this object has a property.
    fn __contains__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.has(key)
    }

    /// Return the number of own property names.
    fn __len__(&self) -> PyResult<usize> {
        self.object.with_local(|scope, object| {
            let names = object
                .get_own_property_names(scope, Default::default())
                .ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to read object keys.")
                })?;

            usize::try_from(names.length()).map_err(|_| {
                pyo3::exceptions::PyOverflowError::new_err("Object key count exceeds usize.")
            })
        })
    }

    /// Return this object's own property names.
    #[gen_stub(override_return_type(type_repr = "list[object]", imports = ()))]
    fn keys(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.object.with_local(|scope, object| {
            let names = object
                .get_own_property_names(scope, Default::default())
                .ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to read object keys.")
                })?;

            value_to_python(py, scope, names.into(), 0)
        })
    }

    /// Define an own data property with optional V8 attributes.
    #[pyo3(signature = (key, value, attributes=None, *, read_only=false, dont_enum=false, dont_delete=false))]
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

        self.object.with_local(|scope, object| {
            define_own_property_on_object(
                key.py(),
                scope,
                object,
                key,
                value,
                attribute,
                self.object.handle.isolate_id,
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
        self.object.with_local(|scope, object| {
            define_property_on_object(
                key.py(),
                scope,
                object,
                key,
                descriptor,
                self.object.handle.isolate_id,
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
        self.object.with_local(|scope, object| {
            get_own_property_descriptor_on_object(py, scope, object, key, &self.object.handle)
        })
    }

    /// Return V8 property attributes for a key.
    fn get_property_attributes(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<PropertyAttribute> {
        self.object.with_local(|scope, object| {
            get_property_attributes_on_object(
                key.py(),
                scope,
                object,
                key,
                self.object.handle.isolate_id,
            )
        })
    }

    /// Set this object's integrity level to "sealed" or "frozen".
    fn set_integrity_level(&self, level: &str) -> PyResult<bool> {
        self.object
            .with_local(|scope, object| set_integrity_level_on_object(scope, object, level))
    }

    /// Freeze this object.
    fn freeze(&self) -> PyResult<bool> {
        self.set_integrity_level("frozen")
    }

    /// Seal this object.
    fn seal(&self) -> PyResult<bool> {
        self.set_integrity_level("sealed")
    }

    /// Read a private property.
    fn get_private(&self, key: PyRef<'_, V8Private>) -> PyResult<Value> {
        self.object.with_local(|scope, object| {
            get_private_on_object(scope, object, &key, &self.object.handle)
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
        self.object.with_local(|scope, object| {
            set_private_on_object(value.py(), scope, object, &key, value, &self.object.handle)
        })
    }

    /// Return whether a private property exists.
    fn has_private(&self, key: PyRef<'_, V8Private>) -> PyResult<bool> {
        self.object.with_local(|scope, object| {
            has_private_on_object(scope, object, &key, &self.object.handle)
        })
    }

    /// Delete a private property.
    fn delete_private(&self, key: PyRef<'_, V8Private>) -> PyResult<bool> {
        self.object.with_local(|scope, object| {
            delete_private_on_object(scope, object, &key, &self.object.handle)
        })
    }

    /// Return an internal field value.
    #[gen_stub(override_return_type(type_repr = "Value | Private | None", imports = ()))]
    fn get_internal_field(&self, py: Python<'_>, index: usize) -> PyResult<Option<Py<PyAny>>> {
        self.object.with_local(|scope, object| {
            get_internal_field_on_object(py, scope, object, index, &self.object.handle)
        })
    }

    /// Set an internal field value.
    fn set_internal_field(
        &self,
        index: usize,
        #[gen_stub(override_type(type_repr = "object", imports = ()))] data: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.object.with_local(|scope, object| {
            set_internal_field_on_object(data.py(), scope, object, index, data, &self.object.handle)
        })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8Array {
    /// Return this array as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.array.to_value()
    }

    /// Return the array length.
    fn length(&self) -> u32 {
        self.array
            .with_local(|_, array| Ok(array.length()))
            .unwrap_or_default()
    }

    /// Return the array length.
    fn __len__(&self) -> usize {
        self.length() as usize
    }

    /// Read an array element.
    fn get(&self, index: u32) -> PyResult<Value> {
        self.array.with_local(|scope, array| {
            let result = array.get_index(scope, index).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to get array item.")
            })?;

            Ok(Value::from_local(
                scope,
                result,
                self.array.handle.context.clone(),
                self.array.handle.isolate.clone(),
                self.array.handle.isolate_id,
            ))
        })
    }

    /// Read an array element.
    fn __getitem__(&self, index: u32) -> PyResult<Value> {
        self.get(index)
    }

    /// Set an array element.
    fn set(
        &self,
        index: u32,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<bool> {
        self.array.with_local(|scope, array| {
            let value = python_to_v8(value.py(), scope, value, self.array.handle.isolate_id, 0)?;

            array.set_index(scope, index, value).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to set array item.")
            })
        })
    }

    /// Set an array element.
    fn __setitem__(
        &self,
        index: u32,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<()> {
        self.set(index, value).map(|_| ())
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8Function {
    /// Return this function as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.function.to_value()
    }

    /// Return the JavaScript function name.
    #[getter]
    fn name(&self) -> PyResult<String> {
        self.function
            .with_local(|scope, function| Ok(function.get_name(scope).to_rust_string_lossy(scope)))
    }

    /// Return V8's script id for this function.
    #[getter]
    fn script_id(&self) -> PyResult<i32> {
        self.function
            .with_local(|_, function| Ok(function.script_id()))
    }

    /// Return the script line number for this function.
    #[getter]
    fn script_line_number(&self) -> PyResult<Option<u32>> {
        self.function
            .with_local(|_, function| Ok(function.get_script_line_number()))
    }

    /// Return the script column number for this function.
    #[getter]
    fn script_column_number(&self) -> PyResult<Option<u32>> {
        self.function
            .with_local(|_, function| Ok(function.get_script_column_number()))
    }

    /// Return the function's resource name.
    #[getter]
    fn resource_name(&self) -> PyResult<Option<String>> {
        self.function.with_local(|scope, function| {
            let origin = function.get_script_origin(scope);

            Ok(origin
                .resource_name()
                .and_then(|value| crate::script::optional_value_to_string(scope, value)))
        })
    }

    /// Return the function's source map URL.
    #[getter]
    fn source_map_url(&self) -> PyResult<Option<String>> {
        self.function.with_local(|scope, function| {
            let origin = function.get_script_origin(scope);

            Ok(origin
                .source_map_url()
                .and_then(|value| crate::script::optional_value_to_string(scope, value)))
        })
    }

    /// Return whether supplied cached data was rejected by V8.
    #[getter]
    fn cached_data_rejected(&self) -> bool {
        self.cached_data_rejected
    }

    /// Create V8 code-cache bytes for this function.
    fn create_code_cache<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        if !self.can_create_code_cache {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Function code cache is only available for functions created by Context.compile_function().",
            ));
        }

        self.function.with_local(|_, function| {
            let cached_data = function.create_code_cache().ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "V8 could not create code cache for this function.",
                )
            })?;

            Ok(PyBytes::new(py, &cached_data))
        })
    }

    /// Call this JavaScript function.
    #[pyo3(signature = (args=None, this_arg=None))]
    fn call(
        &self,
        #[gen_stub(override_type(type_repr = "_JSFunctionArgsLike | None", imports = ()))]
        args: Option<&Bound<'_, PyAny>>,
        #[gen_stub(override_type(type_repr = "_JSValueLike | None", imports = ()))]
        this_arg: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Value> {
        let mut isolate = self.function.handle.isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate));
        let scope = &mut scope.init();
        let context = v8::Local::new(scope, &self.function.handle.context);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(let scope, &mut **scope);
        let function = v8::Local::new(scope, &self.function.value);
        let recv = if let Some(this_arg) = this_arg {
            python_to_v8(
                this_arg.py(),
                scope,
                this_arg,
                self.function.handle.isolate_id,
                0,
            )?
        } else {
            context.global(scope).into()
        };
        let args = python_args_to_v8(args, scope, self.function.handle.isolate_id)?;
        let result = function
            .call(scope, recv, &args)
            .ok_or_else(|| js_exception(scope, "Function execution failed."))?;

        Ok(Value::from_local(
            scope,
            result,
            self.function.handle.context.clone(),
            self.function.handle.isolate.clone(),
            self.function.handle.isolate_id,
        ))
    }

    /// Call this JavaScript function with positional arguments.
    #[pyo3(signature = (*args))]
    fn __call__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] args: &Bound<
            '_,
            PyTuple,
        >,
    ) -> PyResult<Value> {
        self.call(Some(args.as_any()), None)
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8Promise {
    /// Return this promise as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.promise.to_value()
    }

    /// Await this JavaScript Promise from Python.
    #[gen_stub(override_return_type(type_repr = "collections.abc.Generator[object, None, Value]", imports = ("collections.abc")))]
    fn __await__(&self) -> PromiseAwaiter {
        PromiseAwaiter::new(self.promise.handle.clone())
    }

    /// Return the promise state.
    fn state(&self) -> PyResult<&'static str> {
        self.promise
            .with_local(|_, promise| Ok(promise_state_name(&promise.state())))
    }

    /// Return the settled promise result, or None while pending.
    fn result(&self) -> PyResult<Option<Value>> {
        self.promise.with_local(|scope, promise| {
            if promise.state() == v8::PromiseState::Pending {
                return Ok(None);
            }

            let result = promise.result(scope);

            Ok(Some(Value::from_local(
                scope,
                result,
                self.promise.handle.context.clone(),
                self.promise.handle.isolate.clone(),
                self.promise.handle.isolate_id,
            )))
        })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8BigInt {
    /// Return this BigInt as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.bigint.to_value()
    }

    /// Return this BigInt as i64 when the conversion is lossless.
    fn as_i64(&self) -> PyResult<Option<i64>> {
        self.bigint.with_local(|_, bigint| {
            let (value, lossless) = bigint.i64_value();

            Ok(lossless.then_some(value))
        })
    }

    /// Return the decimal string representation.
    fn __str__(&self) -> PyResult<String> {
        self.bigint.with_local(|scope, bigint| {
            let value: v8::Local<'_, v8::Value> = bigint.into();
            bigint_to_decimal_string(scope, value)
        })
    }

    /// Convert this BigInt to a Python int.
    #[gen_stub(override_return_type(type_repr = "int", imports = ()))]
    fn __int__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let decimal = self.__str__()?;

        Ok(py
            .get_type::<pyo3::types::PyInt>()
            .call1((decimal,))?
            .unbind())
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8Symbol {
    /// Return this symbol as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.symbol.to_value()
    }

    /// Return this symbol's description.
    fn description(&self) -> PyResult<Option<String>> {
        self.symbol.with_local(|scope, symbol| {
            let description = symbol.description(scope);

            if description.is_undefined() {
                return Ok(None);
            }

            let description = description.to_string(scope).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "Failed to convert symbol description to string.",
                )
            })?;

            Ok(Some(description.to_rust_string_lossy(scope)))
        })
    }

    /// Return JavaScript's string representation for this symbol.
    fn __str__(&self) -> PyResult<String> {
        self.symbol.with_local(|scope, symbol| {
            let value: v8::Local<'_, v8::Value> = symbol.into();
            symbol_to_string(scope, value)
        })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8ArrayBuffer {
    /// Return this ArrayBuffer as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.array_buffer.to_value()
    }

    /// Return the byte length.
    #[getter]
    fn byte_length(&self) -> PyResult<usize> {
        self.array_buffer
            .with_local(|_, array_buffer| Ok(array_buffer.byte_length()))
    }

    /// Return whether this ArrayBuffer can be detached.
    fn is_detachable(&self) -> PyResult<bool> {
        self.array_buffer
            .with_local(|_, array_buffer| Ok(array_buffer.is_detachable()))
    }

    /// Return whether this ArrayBuffer has been detached.
    fn was_detached(&self) -> PyResult<bool> {
        self.array_buffer
            .with_local(|_, array_buffer| Ok(array_buffer.was_detached()))
    }

    /// Detach this ArrayBuffer.
    fn detach(&self) -> PyResult<bool> {
        self.array_buffer
            .with_local(|_, array_buffer| Ok(array_buffer.detach(None).unwrap_or(false)))
    }

    /// Copy this ArrayBuffer into Python bytes.
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.array_buffer.with_local(|_, array_buffer| {
            let bytes = array_buffer_to_vec(array_buffer)?;
            Ok(PyBytes::new(py, &bytes))
        })
    }

    /// Copy this ArrayBuffer into Python bytes.
    fn copy_contents<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.to_bytes(py)
    }

    /// Return a Python memoryview over copied bytes.
    #[gen_stub(override_return_type(type_repr = "memoryview", imports = ()))]
    fn memoryview(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let bytes = self.to_bytes(py)?;

        Ok(PyMemoryView::from(bytes.as_any())?.into_any().unbind())
    }

    /// Return this ArrayBuffer as Python bytes.
    fn __bytes__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.to_bytes(py)
    }

    /// Return the byte length.
    fn __len__(&self) -> PyResult<usize> {
        self.byte_length()
    }

    /// Create a TypedArray view over this ArrayBuffer.
    #[pyo3(signature = (kind="Uint8Array", byte_offset=0, length=None))]
    fn typed_array(
        &self,
        kind: &str,
        byte_offset: usize,
        length: Option<usize>,
    ) -> PyResult<V8TypedArray> {
        let kind = TypedArrayKind::parse(kind)?;

        self.array_buffer.with_local(|scope, array_buffer| {
            let length = typed_array_length(kind, array_buffer.byte_length(), byte_offset, length)?;
            let typed_array = kind
                .create(scope, array_buffer, byte_offset, length)
                .ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(
                        "Invalid TypedArray byte_offset or length.",
                    )
                })?;
            let handle = V8Value::from_local(
                scope,
                typed_array.into(),
                self.array_buffer.handle.context.clone(),
                self.array_buffer.handle.isolate.clone(),
                self.array_buffer.handle.isolate_id,
            );

            Ok(V8TypedArray::from_local(scope, typed_array, handle))
        })
    }

    /// Create a Uint8Array view over this ArrayBuffer.
    #[pyo3(signature = (byte_offset=0, length=None))]
    fn as_uint8_array(&self, byte_offset: usize, length: Option<usize>) -> PyResult<V8TypedArray> {
        self.typed_array("Uint8Array", byte_offset, length)
    }

    /// Create a DataView over this ArrayBuffer.
    #[pyo3(signature = (byte_offset=0, byte_length=None))]
    fn data_view(&self, byte_offset: usize, byte_length: Option<usize>) -> PyResult<V8DataView> {
        self.array_buffer.with_local(|scope, array_buffer| {
            let available = checked_view_available(array_buffer.byte_length(), byte_offset)?;
            let byte_length = byte_length.unwrap_or(available);
            if byte_length > available {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "DataView byte_length exceeds ArrayBuffer byte_length.",
                ));
            }

            let data_view = v8::DataView::new(scope, array_buffer, byte_offset, byte_length);
            let handle = V8Value::from_local(
                scope,
                data_view.into(),
                self.array_buffer.handle.context.clone(),
                self.array_buffer.handle.isolate.clone(),
                self.array_buffer.handle.isolate_id,
            );

            Ok(V8DataView::from_local(scope, data_view, handle))
        })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8ArrayBufferView {
    /// Return this view as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.view.to_value()
    }

    /// Return the backing ArrayBuffer.
    fn buffer(&self) -> PyResult<V8ArrayBuffer> {
        array_buffer_from_view(&self.view)
    }

    /// Return the view byte length.
    #[getter]
    fn byte_length(&self) -> PyResult<usize> {
        self.view.with_local(|_, view| Ok(view.byte_length()))
    }

    /// Return the byte offset into the backing ArrayBuffer.
    #[getter]
    fn byte_offset(&self) -> PyResult<usize> {
        self.view.with_local(|_, view| Ok(view.byte_offset()))
    }

    /// Return whether this view has a backing buffer.
    fn has_buffer(&self) -> PyResult<bool> {
        self.view.with_local(|_, view| Ok(view.has_buffer()))
    }

    /// Copy this view into Python bytes.
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.view.with_local(|_, view| {
            let bytes = array_buffer_view_to_vec(view)?;
            Ok(PyBytes::new(py, &bytes))
        })
    }

    /// Copy this view into Python bytes.
    fn copy_contents<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.to_bytes(py)
    }

    /// Return a Python memoryview over copied bytes.
    #[gen_stub(override_return_type(type_repr = "memoryview", imports = ()))]
    fn memoryview(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let bytes = self.to_bytes(py)?;

        Ok(PyMemoryView::from(bytes.as_any())?.into_any().unbind())
    }

    /// Return this view as Python bytes.
    fn __bytes__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.to_bytes(py)
    }

    /// Return the view byte length.
    fn __len__(&self) -> PyResult<usize> {
        self.byte_length()
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8TypedArray {
    /// Return this TypedArray as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.typed_array.to_value()
    }

    /// Return the backing ArrayBuffer.
    fn buffer(&self) -> PyResult<V8ArrayBuffer> {
        array_buffer_from_typed_array(&self.typed_array)
    }

    /// Return the JavaScript typed array constructor name.
    #[getter]
    fn type_name(&self) -> PyResult<&'static str> {
        self.typed_array.with_local(|_, typed_array| {
            let value: v8::Local<'_, v8::Value> = typed_array.into();

            Ok(typed_array_type_name(value))
        })
    }

    /// Return the element length.
    #[getter]
    fn length(&self) -> PyResult<usize> {
        self.typed_array
            .with_local(|_, typed_array| Ok(typed_array.length()))
    }

    /// Return the byte length.
    #[getter]
    fn byte_length(&self) -> PyResult<usize> {
        self.typed_array
            .with_local(|_, typed_array| Ok(typed_array.byte_length()))
    }

    /// Return the byte offset into the backing ArrayBuffer.
    #[getter]
    fn byte_offset(&self) -> PyResult<usize> {
        self.typed_array
            .with_local(|_, typed_array| Ok(typed_array.byte_offset()))
    }

    /// Copy this TypedArray into Python bytes.
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.typed_array.with_local(|_, typed_array| {
            let view: v8::Local<'_, v8::ArrayBufferView> = typed_array.into();
            let bytes = array_buffer_view_to_vec(view)?;
            Ok(PyBytes::new(py, &bytes))
        })
    }

    /// Copy this TypedArray into Python bytes.
    fn copy_contents<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.to_bytes(py)
    }

    /// Return a Python memoryview over copied bytes.
    #[gen_stub(override_return_type(type_repr = "memoryview", imports = ()))]
    fn memoryview(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let bytes = self.to_bytes(py)?;

        Ok(PyMemoryView::from(bytes.as_any())?.into_any().unbind())
    }

    /// Return this TypedArray as Python bytes.
    fn __bytes__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.to_bytes(py)
    }

    /// Return the element length.
    fn __len__(&self) -> PyResult<usize> {
        self.length()
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8DataView {
    /// Return this DataView as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.data_view.to_value()
    }

    /// Return the backing ArrayBuffer.
    fn buffer(&self) -> PyResult<V8ArrayBuffer> {
        array_buffer_from_data_view(&self.data_view)
    }

    /// Return the DataView byte length.
    #[getter]
    fn byte_length(&self) -> PyResult<usize> {
        self.data_view
            .with_local(|_, data_view| Ok(data_view.byte_length()))
    }

    /// Return the byte offset into the backing ArrayBuffer.
    #[getter]
    fn byte_offset(&self) -> PyResult<usize> {
        self.data_view
            .with_local(|_, data_view| Ok(data_view.byte_offset()))
    }

    /// Copy this DataView into Python bytes.
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.data_view.with_local(|_, data_view| {
            let view: v8::Local<'_, v8::ArrayBufferView> = data_view.into();
            let bytes = array_buffer_view_to_vec(view)?;
            Ok(PyBytes::new(py, &bytes))
        })
    }

    /// Copy this DataView into Python bytes.
    fn copy_contents<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.to_bytes(py)
    }

    /// Return a Python memoryview over copied bytes.
    #[gen_stub(override_return_type(type_repr = "memoryview", imports = ()))]
    fn memoryview(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let bytes = self.to_bytes(py)?;

        Ok(PyMemoryView::from(bytes.as_any())?.into_any().unbind())
    }

    /// Return this DataView as Python bytes.
    fn __bytes__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.to_bytes(py)
    }

    /// Return the DataView byte length.
    fn __len__(&self) -> PyResult<usize> {
        self.byte_length()
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8Map {
    /// Return this Map as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.map.to_value()
    }

    /// Return the number of Map entries.
    #[getter]
    fn size(&self) -> PyResult<usize> {
        self.map.with_local(|_, map| Ok(map.size()))
    }

    /// Return the number of Map entries.
    fn __len__(&self) -> PyResult<usize> {
        self.size()
    }

    /// Remove all entries from the Map.
    fn clear(&self) -> PyResult<()> {
        self.map.with_local(|_, map| {
            map.clear();
            Ok(())
        })
    }

    /// Return the value for a Map key.
    fn get(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        self.map.with_local(|scope, map| {
            let key = python_to_v8(key.py(), scope, key, self.map.handle.isolate_id, 0)?;
            let value = map.get(scope, key).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to get Map entry.")
            })?;

            Ok(value_from_typed_local(scope, value, &self.map.handle))
        })
    }

    /// Set a Map entry.
    fn set(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<bool> {
        self.map.with_local(|scope, map| {
            let key = python_to_v8(key.py(), scope, key, self.map.handle.isolate_id, 0)?;
            let value = python_to_v8(value.py(), scope, value, self.map.handle.isolate_id, 0)?;

            map.set(scope, key, value).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to set Map entry.")
            })?;

            Ok(true)
        })
    }

    /// Return whether the Map contains a key.
    fn has(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.map.with_local(|scope, map| {
            let key = python_to_v8(key.py(), scope, key, self.map.handle.isolate_id, 0)?;

            map.has(scope, key).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to test Map entry.")
            })
        })
    }

    /// Delete a Map entry.
    fn delete(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.map.with_local(|scope, map| {
            let key = python_to_v8(key.py(), scope, key, self.map.handle.isolate_id, 0)?;

            map.delete(scope, key).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to delete Map entry.")
            })
        })
    }

    /// Return the value for a Map key.
    fn __getitem__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<Value> {
        self.get(key)
    }

    /// Set a Map entry.
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

    /// Delete a Map entry.
    fn __delitem__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.delete(key).map(|_| ())
    }

    /// Return whether the Map contains a key.
    fn __contains__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] key: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.has(key)
    }

    /// Return Map keys.
    #[gen_stub(override_return_type(type_repr = "list[Value]", imports = ()))]
    fn keys(&self) -> PyResult<Vec<Value>> {
        self.map.with_local(|scope, map| {
            let entries = map.as_array(scope);
            let mut keys = Vec::new();

            for index in (0..entries.length()).step_by(2) {
                let key = entries.get_index(scope, index).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to read Map key.")
                })?;
                keys.push(value_from_typed_local(scope, key, &self.map.handle));
            }

            Ok(keys)
        })
    }

    /// Return Map values.
    #[gen_stub(override_return_type(type_repr = "list[Value]", imports = ()))]
    fn values(&self) -> PyResult<Vec<Value>> {
        self.map.with_local(|scope, map| {
            let entries = map.as_array(scope);
            let mut values = Vec::new();

            for index in (1..entries.length()).step_by(2) {
                let value = entries.get_index(scope, index).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to read Map value.")
                })?;
                values.push(value_from_typed_local(scope, value, &self.map.handle));
            }

            Ok(values)
        })
    }

    /// Return Map entries as key-value pairs.
    #[gen_stub(override_return_type(type_repr = "list[tuple[Value, Value]]", imports = ()))]
    fn items(&self) -> PyResult<Vec<(Value, Value)>> {
        self.map.with_local(|scope, map| {
            let entries = map.as_array(scope);
            let mut items = Vec::new();

            for index in (0..entries.length()).step_by(2) {
                let key = entries.get_index(scope, index).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to read Map key.")
                })?;
                let value = entries.get_index(scope, index + 1).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to read Map value.")
                })?;

                items.push((
                    value_from_typed_local(scope, key, &self.map.handle),
                    value_from_typed_local(scope, value, &self.map.handle),
                ));
            }

            Ok(items)
        })
    }

    /// Return Map entries as key-value pairs.
    #[gen_stub(override_return_type(type_repr = "list[tuple[Value, Value]]", imports = ()))]
    fn entries(&self) -> PyResult<Vec<(Value, Value)>> {
        self.items()
    }

    /// Iterate over Map keys.
    #[gen_stub(override_return_type(type_repr = "collections.abc.Iterator[Value]", imports = ("collections.abc")))]
    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        py_iterator(py, self.keys()?)
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8Set {
    /// Return this Set as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.set.to_value()
    }

    /// Return the number of Set entries.
    #[getter]
    fn size(&self) -> PyResult<usize> {
        self.set.with_local(|_, set| Ok(set.size()))
    }

    /// Return the number of Set entries.
    fn __len__(&self) -> PyResult<usize> {
        self.size()
    }

    /// Remove all entries from the Set.
    fn clear(&self) -> PyResult<()> {
        self.set.with_local(|_, set| {
            set.clear();
            Ok(())
        })
    }

    /// Add a value to the Set.
    fn add(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<bool> {
        self.set.with_local(|scope, set| {
            let value = python_to_v8(value.py(), scope, value, self.set.handle.isolate_id, 0)?;

            set.add(scope, value).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to add Set entry.")
            })?;

            Ok(true)
        })
    }

    /// Return whether the Set contains a value.
    fn has(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<bool> {
        self.set.with_local(|scope, set| {
            let value = python_to_v8(value.py(), scope, value, self.set.handle.isolate_id, 0)?;

            set.has(scope, value).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to test Set entry.")
            })
        })
    }

    /// Delete a value from the Set.
    fn delete(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<bool> {
        self.set.with_local(|scope, set| {
            let value = python_to_v8(value.py(), scope, value, self.set.handle.isolate_id, 0)?;

            set.delete(scope, value).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to delete Set entry.")
            })
        })
    }

    /// Return whether the Set contains a value.
    fn __contains__(
        &self,
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: &Bound<
            '_,
            PyAny,
        >,
    ) -> PyResult<bool> {
        self.has(value)
    }

    /// Return Set values.
    #[gen_stub(override_return_type(type_repr = "list[Value]", imports = ()))]
    fn values(&self) -> PyResult<Vec<Value>> {
        self.set.with_local(|scope, set| {
            let entries = set.as_array(scope);
            let step = if entries.length() as usize == set.size() {
                1
            } else {
                2
            };
            let mut values = Vec::new();

            for index in (0..entries.length()).step_by(step) {
                let value = entries.get_index(scope, index).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to read Set entry.")
                })?;
                values.push(value_from_typed_local(scope, value, &self.set.handle));
            }

            Ok(values)
        })
    }

    /// Return Set values.
    #[gen_stub(override_return_type(type_repr = "list[Value]", imports = ()))]
    fn keys(&self) -> PyResult<Vec<Value>> {
        self.values()
    }

    /// Return Set values in iteration order.
    #[gen_stub(override_return_type(type_repr = "list[Value]", imports = ()))]
    fn entries(&self) -> PyResult<Vec<Value>> {
        self.values()
    }

    /// Iterate over Set values.
    #[gen_stub(override_return_type(type_repr = "collections.abc.Iterator[Value]", imports = ("collections.abc")))]
    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        py_iterator(py, self.values()?)
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8Date {
    /// Return this Date as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.date.to_value()
    }

    /// Return milliseconds since the Unix epoch.
    #[getter]
    fn timestamp_ms(&self) -> PyResult<f64> {
        self.date.with_local(|_, date| Ok(date.value_of()))
    }

    /// Return milliseconds since the Unix epoch.
    fn value_of(&self) -> PyResult<f64> {
        self.timestamp_ms()
    }

    /// Return seconds since the Unix epoch.
    fn timestamp(&self) -> PyResult<f64> {
        self.timestamp_ms().map(|value| value / 1000.0)
    }

    /// Convert this Date to a timezone-aware Python datetime.
    #[gen_stub(override_return_type(type_repr = "datetime.datetime", imports = ("datetime")))]
    fn to_datetime(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.date.with_local(|_, date| date_to_python(py, date))
    }

    /// Return milliseconds since the Unix epoch.
    fn __float__(&self) -> PyResult<f64> {
        self.timestamp_ms()
    }

    /// Return JavaScript's string representation for this Date.
    fn __str__(&self) -> PyResult<String> {
        self.date.with_local(|scope, date| {
            let value: v8::Local<'_, v8::Value> = date.into();
            typed_value_to_string(scope, value)
        })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8RegExp {
    /// Return this RegExp as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.regexp.to_value()
    }

    /// Return the RegExp source pattern.
    #[getter]
    fn source(&self) -> PyResult<String> {
        self.regexp
            .with_local(|scope, regexp| Ok(regexp.get_source(scope).to_rust_string_lossy(scope)))
    }

    /// Return the RegExp flags.
    #[getter]
    fn flags(&self) -> PyResult<String> {
        self.regexp.with_local(|scope, regexp| {
            let value: v8::Local<'_, v8::Value> = regexp.into();
            string_property(scope, value, "flags")
        })
    }

    /// Execute this RegExp against a subject string.
    fn exec(&self, subject: &str) -> PyResult<Option<V8Object>> {
        self.regexp.with_local(|scope, regexp| {
            let subject = v8::String::new(scope, subject).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to create RegExp subject.")
            })?;
            let Some(result) = regexp.exec(scope, subject) else {
                return Ok(None);
            };
            let result_value: v8::Local<'_, v8::Value> = result.into();
            if result_value.is_null_or_undefined() {
                return Ok(None);
            }
            let handle = V8Value::from_local(
                scope,
                result_value,
                self.regexp.handle.context.clone(),
                self.regexp.handle.isolate.clone(),
                self.regexp.handle.isolate_id,
            );

            Ok(Some(V8Object::from_local(scope, result, handle)))
        })
    }

    /// Return whether this RegExp matches a subject string.
    fn test(&self, subject: &str) -> PyResult<bool> {
        self.exec(subject).map(|result| result.is_some())
    }

    /// Return JavaScript's string representation for this RegExp.
    fn __str__(&self) -> PyResult<String> {
        self.regexp.with_local(|scope, regexp| {
            let value: v8::Local<'_, v8::Value> = regexp.into();
            typed_value_to_string(scope, value)
        })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8Proxy {
    /// Return this Proxy as a generic V8 Value.
    fn to_value(&self) -> Value {
        self.proxy.to_value()
    }

    /// Return the proxy target object.
    fn target(&self) -> PyResult<Value> {
        self.proxy.with_local(|scope, proxy| {
            Ok(value_from_typed_local(
                scope,
                proxy.get_target(scope),
                &self.proxy.handle,
            ))
        })
    }

    /// Return the proxy handler object.
    fn handler(&self) -> PyResult<Value> {
        self.proxy.with_local(|scope, proxy| {
            Ok(value_from_typed_local(
                scope,
                proxy.get_handler(scope),
                &self.proxy.handle,
            ))
        })
    }

    /// Return whether this proxy has been revoked.
    fn is_revoked(&self) -> PyResult<bool> {
        self.proxy.with_local(|_, proxy| Ok(proxy.is_revoked()))
    }

    /// Revoke this proxy when this wrapper owns the revocation capability.
    fn revoke(&self) -> PyResult<()> {
        if !self.can_revoke {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Proxy cannot be safely revoked through this wrapper.",
            ));
        }

        self.proxy.with_local(|_, proxy| {
            proxy.revoke();
            Ok(())
        })
    }

    /// Return JavaScript's string representation for this Proxy.
    fn __str__(&self) -> PyResult<String> {
        self.proxy.with_local(|scope, proxy| {
            let value: v8::Local<'_, v8::Value> = proxy.into();
            typed_value_to_string(scope, value)
        })
    }
}

#[derive(Clone, Copy)]
enum TypedArrayKind {
    Int8,
    Uint8,
    Uint8Clamped,
    Int16,
    Uint16,
    Int32,
    Uint32,
    BigInt64,
    BigUint64,
    Float16,
    Float32,
    Float64,
}

impl TypedArrayKind {
    fn parse(kind: &str) -> PyResult<Self> {
        let normalized = kind
            .chars()
            .filter(|character| !matches!(character, '_' | '-' | ' '))
            .flat_map(char::to_lowercase)
            .collect::<String>();

        match normalized.as_str() {
            "int8" | "i8" | "int8array" => Ok(Self::Int8),
            "uint8" | "u8" | "uint8array" => Ok(Self::Uint8),
            "uint8clamped" | "uint8clampedarray" => Ok(Self::Uint8Clamped),
            "int16" | "i16" | "int16array" => Ok(Self::Int16),
            "uint16" | "u16" | "uint16array" => Ok(Self::Uint16),
            "int32" | "i32" | "int32array" => Ok(Self::Int32),
            "uint32" | "u32" | "uint32array" => Ok(Self::Uint32),
            "bigint64" | "bigint64array" => Ok(Self::BigInt64),
            "biguint64" | "biguint64array" => Ok(Self::BigUint64),
            "float16" | "f16" | "float16array" => Ok(Self::Float16),
            "float32" | "f32" | "float32array" => Ok(Self::Float32),
            "float64" | "f64" | "float64array" => Ok(Self::Float64),
            _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Unsupported TypedArray kind: {kind}."
            ))),
        }
    }

    fn element_size(self) -> usize {
        match self {
            Self::Int8 | Self::Uint8 | Self::Uint8Clamped => 1,
            Self::Int16 | Self::Uint16 | Self::Float16 => 2,
            Self::Int32 | Self::Uint32 | Self::Float32 => 4,
            Self::BigInt64 | Self::BigUint64 | Self::Float64 => 8,
        }
    }

    fn create<'s>(
        self,
        scope: &v8::PinScope<'s, '_>,
        buffer: v8::Local<'s, v8::ArrayBuffer>,
        byte_offset: usize,
        length: usize,
    ) -> Option<v8::Local<'s, v8::TypedArray>> {
        match self {
            Self::Int8 => v8::Int8Array::new(scope, buffer, byte_offset, length).map(Into::into),
            Self::Uint8 => v8::Uint8Array::new(scope, buffer, byte_offset, length).map(Into::into),
            Self::Uint8Clamped => {
                v8::Uint8ClampedArray::new(scope, buffer, byte_offset, length).map(Into::into)
            }
            Self::Int16 => v8::Int16Array::new(scope, buffer, byte_offset, length).map(Into::into),
            Self::Uint16 => {
                v8::Uint16Array::new(scope, buffer, byte_offset, length).map(Into::into)
            }
            Self::Int32 => v8::Int32Array::new(scope, buffer, byte_offset, length).map(Into::into),
            Self::Uint32 => {
                v8::Uint32Array::new(scope, buffer, byte_offset, length).map(Into::into)
            }
            Self::BigInt64 => {
                v8::BigInt64Array::new(scope, buffer, byte_offset, length).map(Into::into)
            }
            Self::BigUint64 => {
                v8::BigUint64Array::new(scope, buffer, byte_offset, length).map(Into::into)
            }
            Self::Float16 => v8::Float16Array::new(scope, buffer, byte_offset, length)
                .map(|array| unsafe { v8::Local::<v8::TypedArray>::cast_unchecked(array) }),
            Self::Float32 => {
                v8::Float32Array::new(scope, buffer, byte_offset, length).map(Into::into)
            }
            Self::Float64 => {
                v8::Float64Array::new(scope, buffer, byte_offset, length).map(Into::into)
            }
        }
    }
}

pub(crate) fn array_buffer_to_vec(buffer: v8::Local<'_, v8::ArrayBuffer>) -> PyResult<Vec<u8>> {
    let byte_length = buffer.byte_length();

    if byte_length == 0 {
        return Ok(Vec::new());
    }

    let data = buffer.data().ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("ArrayBuffer has no readable backing store.")
    })?;

    // V8 owns the backing store. The bytes are copied immediately into Python/Rust-owned memory.
    let bytes = unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), byte_length) };

    Ok(bytes.to_vec())
}

pub(crate) fn array_buffer_view_to_vec(
    view: v8::Local<'_, v8::ArrayBufferView>,
) -> PyResult<Vec<u8>> {
    let byte_length = view.byte_length();
    let mut bytes = vec![0; byte_length];
    let copied = view.copy_contents(&mut bytes);

    if copied > bytes.len() {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "ArrayBufferView copied more bytes than requested.",
        ));
    }

    bytes.truncate(copied);
    Ok(bytes)
}

pub(crate) fn copy_bytes_to_array_buffer(
    buffer: v8::Local<'_, v8::ArrayBuffer>,
    bytes: &[u8],
) -> PyResult<()> {
    if bytes.is_empty() {
        return Ok(());
    }

    let data = buffer.data().ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("ArrayBuffer has no writable backing store.")
    })?;

    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data.as_ptr().cast::<u8>(), bytes.len());
    }

    Ok(())
}

fn checked_view_available(byte_length: usize, byte_offset: usize) -> PyResult<usize> {
    byte_length.checked_sub(byte_offset).ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err("byte_offset exceeds ArrayBuffer byte_length.")
    })
}

fn typed_array_length(
    kind: TypedArrayKind,
    byte_length: usize,
    byte_offset: usize,
    length: Option<usize>,
) -> PyResult<usize> {
    let element_size = kind.element_size();
    let available = checked_view_available(byte_length, byte_offset)?;

    if !byte_offset.is_multiple_of(element_size) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "TypedArray byte_offset must be aligned to the element size.",
        ));
    }

    match length {
        Some(length) => {
            let required = length.checked_mul(element_size).ok_or_else(|| {
                pyo3::exceptions::PyOverflowError::new_err("TypedArray byte length overflowed.")
            })?;
            if required > available {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "TypedArray length exceeds ArrayBuffer byte_length.",
                ));
            }

            Ok(length)
        }
        None => {
            if available % element_size != 0 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "ArrayBuffer byte_length is not aligned to the TypedArray element size.",
                ));
            }

            Ok(available / element_size)
        }
    }
}

fn array_buffer_from_view(view_value: &TypedValue<v8::ArrayBufferView>) -> PyResult<V8ArrayBuffer> {
    view_value.with_local(|scope, view| {
        let buffer = view.buffer(scope).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "ArrayBufferView does not have an underlying ArrayBuffer.",
            )
        })?;
        let handle = V8Value::from_local(
            scope,
            buffer.into(),
            view_value.handle.context.clone(),
            view_value.handle.isolate.clone(),
            view_value.handle.isolate_id,
        );

        Ok(V8ArrayBuffer::from_local(scope, buffer, handle))
    })
}

fn array_buffer_from_typed_array(
    typed_array_value: &TypedValue<v8::TypedArray>,
) -> PyResult<V8ArrayBuffer> {
    typed_array_value.with_local(|scope, typed_array| {
        let view: v8::Local<'_, v8::ArrayBufferView> = typed_array.into();
        let buffer = view.buffer(scope).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "TypedArray does not have an underlying ArrayBuffer.",
            )
        })?;
        let handle = V8Value::from_local(
            scope,
            buffer.into(),
            typed_array_value.handle.context.clone(),
            typed_array_value.handle.isolate.clone(),
            typed_array_value.handle.isolate_id,
        );

        Ok(V8ArrayBuffer::from_local(scope, buffer, handle))
    })
}

fn array_buffer_from_data_view(
    data_view_value: &TypedValue<v8::DataView>,
) -> PyResult<V8ArrayBuffer> {
    data_view_value.with_local(|scope, data_view| {
        let view: v8::Local<'_, v8::ArrayBufferView> = data_view.into();
        let buffer = view.buffer(scope).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "DataView does not have an underlying ArrayBuffer.",
            )
        })?;
        let handle = V8Value::from_local(
            scope,
            buffer.into(),
            data_view_value.handle.context.clone(),
            data_view_value.handle.isolate.clone(),
            data_view_value.handle.isolate_id,
        );

        Ok(V8ArrayBuffer::from_local(scope, buffer, handle))
    })
}

fn typed_array_type_name(value: v8::Local<'_, v8::Value>) -> &'static str {
    if value.is_uint8_array() {
        "Uint8Array"
    } else if value.is_uint8_clamped_array() {
        "Uint8ClampedArray"
    } else if value.is_int8_array() {
        "Int8Array"
    } else if value.is_uint16_array() {
        "Uint16Array"
    } else if value.is_int16_array() {
        "Int16Array"
    } else if value.is_uint32_array() {
        "Uint32Array"
    } else if value.is_int32_array() {
        "Int32Array"
    } else if value.is_big_uint64_array() {
        "BigUint64Array"
    } else if value.is_big_int64_array() {
        "BigInt64Array"
    } else if value.is_float16_array() {
        "Float16Array"
    } else if value.is_float32_array() {
        "Float32Array"
    } else if value.is_float64_array() {
        "Float64Array"
    } else {
        "TypedArray"
    }
}
