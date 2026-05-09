use pyo3::prelude::{Bound, Py, PyAny, PyRef, PyResult, Python, pyclass, pymethods};
use pyo3::types::PyAnyMethods;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use super::convert::python_to_v8;
use super::handle::V8Value;
use super::value::Value;
use crate::runtime::{self, SharedIsolate};

/// V8 Private key wrapper for hidden object properties.
#[gen_stub_pyclass]
#[pyclass(name = "Private", unsendable)]
pub(crate) struct V8Private {
    value: v8::Global<v8::Private>,
    context: v8::Global<v8::Context>,
    isolate: SharedIsolate,
    isolate_id: u64,
}

/// V8 External wrapper that can carry a Python-owned payload.
#[gen_stub_pyclass]
#[pyclass(name = "External", unsendable)]
pub(crate) struct V8External {
    value: v8::Global<v8::External>,
    handle: V8Value,
}

#[gen_stub_pymethods]
#[pymethods]
impl V8Private {
    /// Return this private key's string name when one exists.
    #[getter]
    fn name(&self) -> PyResult<Option<String>> {
        self.with_local(|scope, private| {
            let name = private.name(scope);

            if name.is_undefined() {
                return Ok(None);
            }

            let name = name.to_string(scope).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "Failed to convert Private name to string.",
                )
            })?;

            Ok(Some(name.to_rust_string_lossy(scope)))
        })
    }

    /// Return this private key's V8 name value.
    fn name_value(&self) -> PyResult<Value> {
        self.with_local(|scope, private| {
            let name = private.name(scope);

            Ok(Value::from_local(
                scope,
                name,
                self.context.clone(),
                self.isolate.clone(),
                self.isolate_id,
            ))
        })
    }

    /// Return a debug representation.
    fn __repr__(&self) -> PyResult<String> {
        match self.name()? {
            Some(name) => Ok(format!("v8.Private({name:?})")),
            None => Ok("v8.Private()".to_owned()),
        }
    }
}

impl V8Private {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Private>,
        context: v8::Global<v8::Context>,
        isolate: SharedIsolate,
        isolate_id: u64,
    ) -> Self {
        Self {
            value: v8::Global::new(scope, value),
            context,
            isolate,
            isolate_id,
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        if self.isolate_id == isolate_id {
            return Ok(());
        }

        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "V8 Private belongs to a different Isolate.",
        ))
    }

    pub(crate) fn local_private<'s>(
        &self,
        scope: &v8::PinScope<'s, '_>,
    ) -> v8::Local<'s, v8::Private> {
        v8::Local::new(scope, &self.value)
    }

    fn with_local<R>(
        &self,
        f: impl for<'s> FnOnce(&v8::PinScope<'s, '_>, v8::Local<'s, v8::Private>) -> PyResult<R>,
    ) -> PyResult<R> {
        let mut isolate = self.isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate));
        let scope = &mut scope.init();
        let context = v8::Local::new(scope, &self.context);
        let scope = &mut v8::ContextScope::new(scope, context);
        let value = v8::Local::new(scope, &self.value);

        f(scope, value)
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl V8External {
    /// Return this external as a generic V8 Value.
    fn to_value(&self) -> Value {
        Value::from_handle(self.handle.clone())
    }

    /// Return the runtime-managed payload id.
    #[getter]
    fn id(&self) -> PyResult<Option<u64>> {
        self.with_local(|_, external| Ok(runtime::external_id(external.value())))
    }

    /// Return whether this external is managed by this runtime.
    fn is_managed(&self) -> PyResult<bool> {
        self.id().map(|id| id.is_some())
    }

    /// Return the Python payload stored in this external.
    #[gen_stub(override_return_type(type_repr = "object", imports = ()))]
    fn payload(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.with_local(|_, external| {
            runtime::external_payload(py, external.value()).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "External payload is not managed by this runtime.",
                )
            })
        })
    }

    /// Return a debug representation.
    fn __repr__(&self) -> PyResult<String> {
        match self.id()? {
            Some(id) => Ok(format!("v8.External(id={id})")),
            None => Ok("v8.External(unmanaged)".to_owned()),
        }
    }
}

impl V8External {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::External>,
        handle: V8Value,
    ) -> Self {
        Self {
            value: v8::Global::new(scope, value),
            handle,
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        self.handle.ensure_isolate(isolate_id)
    }

    pub(crate) fn local_value<'s>(&self, scope: &v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        v8::Local::new(scope, &self.handle.value)
    }

    fn with_local<R>(
        &self,
        f: impl for<'s> FnOnce(&v8::PinScope<'s, '_>, v8::Local<'s, v8::External>) -> PyResult<R>,
    ) -> PyResult<R> {
        self.handle.with_local_value(|scope, _| {
            let value = v8::Local::new(scope, &self.value);

            f(scope, value)
        })
    }
}

pub(crate) fn data_from_python<'s>(
    py: Python<'_>,
    scope: &v8::PinScope<'s, '_>,
    value: &Bound<'_, PyAny>,
    isolate_id: u64,
) -> PyResult<v8::Local<'s, v8::Data>> {
    if let Ok(value) = value.extract::<PyRef<'_, V8Private>>() {
        value.ensure_isolate(isolate_id)?;
        return Ok(value.local_private(scope).into());
    }

    Ok(python_to_v8(py, scope, value, isolate_id, 0)?.into())
}

pub(crate) fn data_to_python(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    data: v8::Local<'_, v8::Data>,
    handle: &V8Value,
) -> PyResult<Py<PyAny>> {
    if data.is_value() {
        let value = v8::Local::<v8::Value>::try_from(data).map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to cast internal field to Value.")
        })?;
        let value = Value::from_local(
            scope,
            value,
            handle.context.clone(),
            handle.isolate.clone(),
            handle.isolate_id,
        );

        return Py::new(py, value).map(|value| value.into_any());
    }

    if data.is_private() {
        let private = v8::Local::<v8::Private>::try_from(data).map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to cast internal field to Private.")
        })?;
        let private = V8Private::from_local(
            scope,
            private,
            handle.context.clone(),
            handle.isolate.clone(),
            handle.isolate_id,
        );

        return Py::new(py, private).map(|value| value.into_any());
    }

    Err(pyo3::exceptions::PyTypeError::new_err(
        "Unsupported V8 Data type in internal field.",
    ))
}

pub(crate) fn get_private_on_object(
    scope: &v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    key: &V8Private,
    handle: &V8Value,
) -> PyResult<Value> {
    key.ensure_isolate(handle.isolate_id)?;
    let key = key.local_private(scope);
    let value = object.get_private(scope, key).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to get private property.")
    })?;

    Ok(Value::from_local(
        scope,
        value,
        handle.context.clone(),
        handle.isolate.clone(),
        handle.isolate_id,
    ))
}

pub(crate) fn set_private_on_object(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    key: &V8Private,
    value: &Bound<'_, PyAny>,
    handle: &V8Value,
) -> PyResult<bool> {
    key.ensure_isolate(handle.isolate_id)?;
    let key = key.local_private(scope);
    let value = python_to_v8(py, scope, value, handle.isolate_id, 0)?;

    object
        .set_private(scope, key, value)
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Failed to set private property."))
}

pub(crate) fn has_private_on_object(
    scope: &v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    key: &V8Private,
    handle: &V8Value,
) -> PyResult<bool> {
    key.ensure_isolate(handle.isolate_id)?;
    let key = key.local_private(scope);

    object.has_private(scope, key).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to test private property.")
    })
}

pub(crate) fn delete_private_on_object(
    scope: &v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    key: &V8Private,
    handle: &V8Value,
) -> PyResult<bool> {
    key.ensure_isolate(handle.isolate_id)?;
    let key = key.local_private(scope);

    object.delete_private(scope, key).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to delete private property.")
    })
}

pub(crate) fn get_internal_field_on_object(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    index: usize,
    handle: &V8Value,
) -> PyResult<Option<Py<PyAny>>> {
    let Some(data) = object.get_internal_field(scope, index) else {
        return Ok(None);
    };

    data_to_python(py, scope, data, handle).map(Some)
}

pub(crate) fn set_internal_field_on_object(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    index: usize,
    data: &Bound<'_, PyAny>,
    handle: &V8Value,
) -> PyResult<bool> {
    let data = data_from_python(py, scope, data, handle.isolate_id)?;

    Ok(object.set_internal_field(index, data))
}
