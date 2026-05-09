use pyo3::prelude::{Bound, Py, PyAny, PyRef, PyResult, Python, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyDict, PyDictMethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use super::convert::python_to_v8;
use super::handle::V8Value;
use super::value::Value;

/// V8 object property attribute bits.
#[gen_stub_pyclass]
#[pyclass(unsendable, skip_from_py_object)]
#[derive(Clone, Copy, Default)]
pub(crate) struct PropertyAttribute {
    read_only: bool,
    dont_enum: bool,
    dont_delete: bool,
}

/// ECMAScript property descriptor used by Object.defineProperty.
#[gen_stub_pyclass]
#[pyclass(unsendable)]
pub(crate) struct PropertyDescriptor {
    value: Option<Py<PyAny>>,
    get: Option<Py<PyAny>>,
    set: Option<Py<PyAny>>,
    writable: Option<bool>,
    enumerable: Option<bool>,
    configurable: Option<bool>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PropertyAttribute {
    /// Create property attributes from boolean flags.
    #[new]
    #[pyo3(signature = (*, read_only=false, dont_enum=false, dont_delete=false))]
    fn new(read_only: bool, dont_enum: bool, dont_delete: bool) -> Self {
        Self {
            read_only,
            dont_enum,
            dont_delete,
        }
    }

    /// Return an attribute set with no flags.
    #[staticmethod]
    fn none() -> Self {
        Self::default()
    }

    /// Return the read-only attribute.
    #[staticmethod]
    fn read_only_attribute() -> Self {
        Self::from_v8(v8::PropertyAttribute::READ_ONLY)
    }

    /// Return the dont-enum attribute.
    #[staticmethod]
    fn dont_enum_attribute() -> Self {
        Self::from_v8(v8::PropertyAttribute::DONT_ENUM)
    }

    /// Return the dont-delete attribute.
    #[staticmethod]
    fn dont_delete_attribute() -> Self {
        Self::from_v8(v8::PropertyAttribute::DONT_DELETE)
    }

    /// Return the raw V8 attribute bits.
    #[getter]
    fn bits(&self) -> u32 {
        self.to_v8().as_u32()
    }

    /// Return whether the property is read-only.
    #[getter]
    fn read_only(&self) -> bool {
        self.read_only
    }

    /// Return whether the property is excluded from enumeration.
    #[getter]
    fn dont_enum(&self) -> bool {
        self.dont_enum
    }

    /// Return whether the property cannot be deleted.
    #[getter]
    fn dont_delete(&self) -> bool {
        self.dont_delete
    }

    /// Return whether the property is writable.
    #[getter]
    fn writable(&self) -> bool {
        !self.read_only
    }

    /// Return whether the property is enumerable.
    #[getter]
    fn enumerable(&self) -> bool {
        !self.dont_enum
    }

    /// Return whether the property is configurable.
    #[getter]
    fn configurable(&self) -> bool {
        !self.dont_delete
    }

    /// Return whether no attribute flags are set.
    fn is_none(&self) -> bool {
        !self.read_only && !self.dont_enum && !self.dont_delete
    }

    /// Combine two attribute sets.
    fn __or__(&self, other: PyRef<'_, Self>) -> Self {
        Self {
            read_only: self.read_only || other.read_only,
            dont_enum: self.dont_enum || other.dont_enum,
            dont_delete: self.dont_delete || other.dont_delete,
        }
    }

    /// Return a debug representation.
    fn __repr__(&self) -> String {
        let mut parts = Vec::new();
        if self.read_only {
            parts.push("read_only=True");
        }
        if self.dont_enum {
            parts.push("dont_enum=True");
        }
        if self.dont_delete {
            parts.push("dont_delete=True");
        }

        if parts.is_empty() {
            "PropertyAttribute()".to_owned()
        } else {
            format!("PropertyAttribute({})", parts.join(", "))
        }
    }
}

impl PropertyAttribute {
    pub(crate) fn from_v8(attribute: v8::PropertyAttribute) -> Self {
        Self {
            read_only: attribute.is_read_only(),
            dont_enum: attribute.is_dont_enum(),
            dont_delete: attribute.is_dont_delete(),
        }
    }

    pub(crate) fn to_v8(self) -> v8::PropertyAttribute {
        let mut attribute = v8::PropertyAttribute::NONE;
        if self.read_only {
            attribute = attribute | v8::PropertyAttribute::READ_ONLY;
        }
        if self.dont_enum {
            attribute = attribute | v8::PropertyAttribute::DONT_ENUM;
        }
        if self.dont_delete {
            attribute = attribute | v8::PropertyAttribute::DONT_DELETE;
        }

        attribute
    }

    pub(crate) fn combine(self, other: Self) -> Self {
        Self {
            read_only: self.read_only || other.read_only,
            dont_enum: self.dont_enum || other.dont_enum,
            dont_delete: self.dont_delete || other.dont_delete,
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PropertyDescriptor {
    /// Create a property descriptor.
    #[new]
    #[pyo3(signature = (value=None, *, get=None, set=None, writable=None, enumerable=None, configurable=None))]
    fn new(
        #[gen_stub(override_type(type_repr = "_JSValueLike | None", imports = ()))] value: Option<
            Py<PyAny>,
        >,
        #[gen_stub(override_type(type_repr = "_JSAccessorLike | None", imports = ()))] get: Option<
            Py<PyAny>,
        >,
        #[gen_stub(override_type(type_repr = "_JSAccessorLike | None", imports = ()))] set: Option<
            Py<PyAny>,
        >,
        writable: Option<bool>,
        enumerable: Option<bool>,
        configurable: Option<bool>,
    ) -> PyResult<Self> {
        let descriptor = Self {
            value,
            get,
            set,
            writable,
            enumerable,
            configurable,
        };
        descriptor.validate()?;

        Ok(descriptor)
    }

    /// Create a data descriptor.
    #[staticmethod]
    #[pyo3(signature = (value, *, writable=None, enumerable=None, configurable=None))]
    fn data(
        #[gen_stub(override_type(type_repr = "_JSValueLike", imports = ()))] value: Py<PyAny>,
        writable: Option<bool>,
        enumerable: Option<bool>,
        configurable: Option<bool>,
    ) -> Self {
        Self {
            value: Some(value),
            get: None,
            set: None,
            writable,
            enumerable,
            configurable,
        }
    }

    /// Create an accessor descriptor.
    #[staticmethod]
    #[pyo3(signature = (*, get=None, set=None, enumerable=None, configurable=None))]
    fn accessor(
        #[gen_stub(override_type(type_repr = "_JSAccessorLike | None", imports = ()))] get: Option<
            Py<PyAny>,
        >,
        #[gen_stub(override_type(type_repr = "_JSAccessorLike | None", imports = ()))] set: Option<
            Py<PyAny>,
        >,
        enumerable: Option<bool>,
        configurable: Option<bool>,
    ) -> Self {
        Self {
            value: None,
            get,
            set,
            writable: None,
            enumerable,
            configurable,
        }
    }

    /// Return the descriptor value.
    #[getter]
    #[gen_stub(override_return_type(type_repr = "_JSValueLike | None", imports = ()))]
    fn value(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.value.as_ref().map(|value| value.clone_ref(py))
    }

    /// Return the getter function.
    #[getter]
    #[gen_stub(override_return_type(type_repr = "_JSAccessorLike | None", imports = ()))]
    fn get(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.get.as_ref().map(|value| value.clone_ref(py))
    }

    /// Return the setter function.
    #[getter]
    #[gen_stub(override_return_type(type_repr = "_JSAccessorLike | None", imports = ()))]
    fn set(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.set.as_ref().map(|value| value.clone_ref(py))
    }

    /// Return the writable flag.
    #[getter]
    fn writable(&self) -> Option<bool> {
        self.writable
    }

    /// Return the enumerable flag.
    #[getter]
    fn enumerable(&self) -> Option<bool> {
        self.enumerable
    }

    /// Return the configurable flag.
    #[getter]
    fn configurable(&self) -> Option<bool> {
        self.configurable
    }

    /// Return whether this descriptor has a value field.
    fn has_value(&self) -> bool {
        self.value.is_some()
    }

    /// Return whether this descriptor has a getter.
    fn has_get(&self) -> bool {
        self.get.is_some()
    }

    /// Return whether this descriptor has a setter.
    fn has_set(&self) -> bool {
        self.set.is_some()
    }

    /// Return whether this descriptor has a writable flag.
    fn has_writable(&self) -> bool {
        self.writable.is_some()
    }

    /// Return whether this descriptor has an enumerable flag.
    fn has_enumerable(&self) -> bool {
        self.enumerable.is_some()
    }

    /// Return whether this descriptor has a configurable flag.
    fn has_configurable(&self) -> bool {
        self.configurable.is_some()
    }

    /// Return whether this is a data descriptor.
    fn is_data_descriptor(&self) -> bool {
        self.value.is_some() || self.writable.is_some()
    }

    /// Return whether this is an accessor descriptor.
    fn is_accessor_descriptor(&self) -> bool {
        self.get.is_some() || self.set.is_some()
    }

    /// Convert this descriptor into a Python dict.
    #[gen_stub(override_return_type(type_repr = "dict[str, object]", imports = ()))]
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);

        if let Some(value) = &self.value {
            dict.set_item("value", value.bind(py))?;
        }
        if let Some(get) = &self.get {
            dict.set_item("get", get.bind(py))?;
        }
        if let Some(set) = &self.set {
            dict.set_item("set", set.bind(py))?;
        }
        if let Some(writable) = self.writable {
            dict.set_item("writable", writable)?;
        }
        if let Some(enumerable) = self.enumerable {
            dict.set_item("enumerable", enumerable)?;
        }
        if let Some(configurable) = self.configurable {
            dict.set_item("configurable", configurable)?;
        }

        Ok(dict.into_any().unbind())
    }

    /// Return a debug representation.
    fn __repr__(&self) -> String {
        let mut parts = Vec::new();
        if self.value.is_some() {
            parts.push("value=...");
        }
        if self.get.is_some() {
            parts.push("get=...");
        }
        if self.set.is_some() {
            parts.push("set=...");
        }
        if let Some(writable) = self.writable {
            parts.push(if writable {
                "writable=True"
            } else {
                "writable=False"
            });
        }
        if let Some(enumerable) = self.enumerable {
            parts.push(if enumerable {
                "enumerable=True"
            } else {
                "enumerable=False"
            });
        }
        if let Some(configurable) = self.configurable {
            parts.push(if configurable {
                "configurable=True"
            } else {
                "configurable=False"
            });
        }

        if parts.is_empty() {
            "PropertyDescriptor()".to_owned()
        } else {
            format!("PropertyDescriptor({})", parts.join(", "))
        }
    }
}

impl PropertyDescriptor {
    fn validate(&self) -> PyResult<()> {
        if self.is_data_descriptor() && self.is_accessor_descriptor() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "PropertyDescriptor cannot mix data and accessor fields.",
            ));
        }

        if self.writable.is_some() && self.value.is_none() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "PropertyDescriptor writable requires a data value.",
            ));
        }

        Ok(())
    }

    pub(crate) fn to_v8_descriptor(
        &self,
        py: Python<'_>,
        scope: &v8::PinScope<'_, '_>,
        isolate_id: u64,
    ) -> PyResult<v8::PropertyDescriptor> {
        self.validate()?;

        let mut descriptor = if self.is_accessor_descriptor() {
            let get = match &self.get {
                Some(get) => python_to_v8(py, scope, get.bind(py), isolate_id, 0)?,
                None => v8::undefined(scope).into(),
            };
            let set = match &self.set {
                Some(set) => python_to_v8(py, scope, set.bind(py), isolate_id, 0)?,
                None => v8::undefined(scope).into(),
            };

            v8::PropertyDescriptor::new_from_get_set(get, set)
        } else if let Some(value) = &self.value {
            let value = python_to_v8(py, scope, value.bind(py), isolate_id, 0)?;
            if let Some(writable) = self.writable {
                v8::PropertyDescriptor::new_from_value_writable(value, writable)
            } else {
                v8::PropertyDescriptor::new_from_value(value)
            }
        } else {
            v8::PropertyDescriptor::new()
        };

        if let Some(enumerable) = self.enumerable {
            descriptor.set_enumerable(enumerable);
        }
        if let Some(configurable) = self.configurable {
            descriptor.set_configurable(configurable);
        }

        Ok(descriptor)
    }

    pub(crate) fn from_descriptor_object<'s>(
        py: Python<'_>,
        scope: &v8::PinScope<'s, '_>,
        descriptor_value: v8::Local<'s, v8::Value>,
        handle: &V8Value,
    ) -> PyResult<Self> {
        let descriptor = descriptor_value.to_object(scope).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Property descriptor value is not an object.")
        })?;

        Ok(Self {
            value: descriptor_py_value(py, scope, descriptor, "value", handle)?,
            get: descriptor_py_value(py, scope, descriptor, "get", handle)?,
            set: descriptor_py_value(py, scope, descriptor, "set", handle)?,
            writable: descriptor_bool(scope, descriptor, "writable")?,
            enumerable: descriptor_bool(scope, descriptor, "enumerable")?,
            configurable: descriptor_bool(scope, descriptor, "configurable")?,
        })
    }
}

fn descriptor_py_value<'s>(
    py: Python<'_>,
    scope: &v8::PinScope<'s, '_>,
    descriptor: v8::Local<'s, v8::Object>,
    name: &str,
    handle: &V8Value,
) -> PyResult<Option<Py<PyAny>>> {
    let key = v8::String::new(scope, name).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create descriptor field name.")
    })?;
    let has_field = descriptor
        .has_own_property(scope, key.into())
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to inspect descriptor field.")
        })?;
    if !has_field {
        return Ok(None);
    }

    let value = descriptor.get(scope, key.into()).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to read descriptor field.")
    })?;
    let value = Value::from_local(
        scope,
        value,
        handle.context.clone(),
        handle.isolate.clone(),
        handle.isolate_id,
    );

    Ok(Some(Py::new(py, value)?.into_any()))
}

fn descriptor_bool<'s>(
    scope: &v8::PinScope<'s, '_>,
    descriptor: v8::Local<'s, v8::Object>,
    name: &str,
) -> PyResult<Option<bool>> {
    let key = v8::String::new(scope, name).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create descriptor field name.")
    })?;
    let has_field = descriptor
        .has_own_property(scope, key.into())
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to inspect descriptor field.")
        })?;
    if !has_field {
        return Ok(None);
    }

    let value = descriptor.get(scope, key.into()).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to read descriptor field.")
    })?;

    Ok(Some(value.boolean_value(scope)))
}

pub(crate) fn property_attribute_from_python(
    value: Option<&Bound<'_, PyAny>>,
    read_only: bool,
    dont_enum: bool,
    dont_delete: bool,
) -> PyResult<PropertyAttribute> {
    let mut attribute = PropertyAttribute {
        read_only,
        dont_enum,
        dont_delete,
    };

    if let Some(value) = value
        && !value.is_none()
    {
        let provided = value.extract::<PyRef<'_, PropertyAttribute>>()?;
        attribute = attribute.combine(*provided);
    }

    Ok(attribute)
}

pub(crate) fn define_own_property_on_object(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    key: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
    attribute: PropertyAttribute,
    isolate_id: u64,
) -> PyResult<bool> {
    let key = python_to_v8_name(py, scope, key, isolate_id)?;
    let value = python_to_v8(py, scope, value, isolate_id, 0)?;

    object
        .define_own_property(scope, key, value, attribute.to_v8())
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Failed to define own property."))
}

pub(crate) fn define_property_on_object(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    key: &Bound<'_, PyAny>,
    descriptor: PyRef<'_, PropertyDescriptor>,
    isolate_id: u64,
) -> PyResult<bool> {
    let key = python_to_v8_name(py, scope, key, isolate_id)?;
    let descriptor = descriptor.to_v8_descriptor(py, scope, isolate_id)?;

    object
        .define_property(scope, key, &descriptor)
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Failed to define property."))
}

pub(crate) fn get_own_property_descriptor_on_object(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    key: &Bound<'_, PyAny>,
    handle: &V8Value,
) -> PyResult<Option<PropertyDescriptor>> {
    let key = python_to_v8_name(py, scope, key, handle.isolate_id)?;
    let Some(descriptor) = object.get_own_property_descriptor(scope, key) else {
        return Ok(None);
    };

    PropertyDescriptor::from_descriptor_object(py, scope, descriptor, handle).map(Some)
}

pub(crate) fn get_property_attributes_on_object(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    key: &Bound<'_, PyAny>,
    isolate_id: u64,
) -> PyResult<PropertyAttribute> {
    let key = python_to_v8(py, scope, key, isolate_id, 0)?;
    let attributes = object.get_property_attributes(scope, key).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to get property attributes.")
    })?;

    Ok(PropertyAttribute::from_v8(attributes))
}

pub(crate) fn set_integrity_level_on_object(
    scope: &v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    level: &str,
) -> PyResult<bool> {
    let level = parse_integrity_level(level)?;

    object.set_integrity_level(scope, level).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to set object integrity level.")
    })
}

fn python_to_v8_name<'s>(
    py: Python<'_>,
    scope: &v8::PinScope<'s, '_>,
    value: &Bound<'_, PyAny>,
    isolate_id: u64,
) -> PyResult<v8::Local<'s, v8::Name>> {
    let value = python_to_v8(py, scope, value, isolate_id, 0)?;

    v8::Local::<v8::Name>::try_from(value).map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err("Property key must be a string or symbol.")
    })
}

fn parse_integrity_level(level: &str) -> PyResult<v8::IntegrityLevel> {
    match level {
        "frozen" | "freeze" => Ok(v8::IntegrityLevel::Frozen),
        "sealed" | "seal" => Ok(v8::IntegrityLevel::Sealed),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "integrity level must be 'frozen' or 'sealed'.",
        )),
    }
}
