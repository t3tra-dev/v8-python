use std::ffi::c_void;

use pyo3::prelude::{Bound, Py, PyAny, PyResult, Python};
use pyo3::types::{PyAnyMethods, PyTuple};

use crate::context::Context;
use crate::profile::{
    is_python_awaitable, last_path_segment, python_arguments, python_awaitable_to_js_promise,
    set_global_path, throw_js_error,
};
use crate::runtime::{self, get_host_function, register_host_function};
use crate::v8value::{python_to_v8, value_to_python};

pub(crate) struct HostClassDefinition {
    name: String,
    cls: Py<PyAny>,
}

enum ClassMember {
    Method {
        name: String,
        callable: Py<PyAny>,
    },
    Property {
        name: String,
        getter: Option<Py<PyAny>>,
        setter: Option<Py<PyAny>>,
    },
}

enum RegisteredClassMember {
    Method {
        name: String,
        id: u64,
    },
    Property {
        name: String,
        getter_id: Option<u64>,
        setter_id: Option<u64>,
    },
}

impl HostClassDefinition {
    pub(crate) fn new(py: Python<'_>, name: Option<String>, cls: Py<PyAny>) -> PyResult<Self> {
        let cls_ref = cls.bind(py);

        if !cls_ref.is_callable() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "host class must be callable.",
            ));
        }

        let name = match name {
            Some(name) if !name.trim().is_empty() => name,
            Some(_) => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "host class name cannot be empty.",
                ));
            }
            None => cls_ref.getattr("__name__")?.extract()?,
        };

        Ok(Self {
            name,
            cls: cls_ref.clone().unbind(),
        })
    }

    pub(crate) fn clone_ref(&self, py: Python<'_>) -> Self {
        Self {
            name: self.name.clone(),
            cls: self.cls.clone_ref(py),
        }
    }
}

pub(crate) fn install_host_class(
    py: Python<'_>,
    context: &mut Context,
    definition: &HostClassDefinition,
) -> PyResult<()> {
    let members = inspect_class_members(py, definition.cls.bind(py))?;
    let isolate = context
        .isolate
        .as_ref()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive."))?;
    let context_global = context
        .context
        .as_ref()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive."))?;
    let constructor_id = register_host_function(
        py,
        isolate,
        context.isolate_id,
        context_global,
        definition.cls.clone_ref(py),
    );
    let members = register_class_members(py, isolate, context.isolate_id, context_global, members);

    let mut isolate_ref = isolate.borrow_mut();
    let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
    let scope = &mut scope.init();
    let local_context = v8::Local::new(scope, context_global);
    let class_name = last_path_segment(&definition.name)?;
    let constructor_template = build_class_template(scope, class_name, constructor_id, members)?;
    let scope = &mut v8::ContextScope::new(scope, local_context);
    let constructor = constructor_template.get_function(scope).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to instantiate FunctionTemplate.")
    })?;

    set_global_path(scope, local_context, &definition.name, constructor.into())
}

fn build_class_template<'s>(
    scope: &v8::PinScope<'s, '_, ()>,
    class_name: &str,
    constructor_id: u64,
    members: Vec<RegisteredClassMember>,
) -> PyResult<v8::Local<'s, v8::FunctionTemplate>> {
    let constructor_data = v8::External::new(scope, constructor_id as usize as *mut c_void);
    let constructor_template = v8::FunctionTemplate::builder(call_python_class_constructor)
        .data(constructor_data.into())
        .build(scope);
    let name = v8::String::new(scope, class_name)
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Failed to create class name."))?;

    constructor_template.set_class_name(name);
    constructor_template
        .instance_template(scope)
        .set_internal_field_count(1);

    let prototype = constructor_template.prototype_template(scope);

    for member in members {
        match member {
            RegisteredClassMember::Method { name, id } => {
                let data = v8::External::new(scope, id as usize as *mut c_void);
                let method_template = v8::FunctionTemplate::builder(call_python_instance_method)
                    .data(data.into())
                    .build(scope);
                let key = v8::String::new(scope, &name).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to create method name.")
                })?;

                prototype.set(key.into(), method_template.into());
            }
            RegisteredClassMember::Property {
                name,
                getter_id,
                setter_id,
            } => {
                let getter = getter_id.map(|getter_id| {
                    let data = v8::External::new(scope, getter_id as usize as *mut c_void);
                    v8::FunctionTemplate::builder(call_python_property_getter)
                        .data(data.into())
                        .build(scope)
                });
                let setter = setter_id.map(|setter_id| {
                    let data = v8::External::new(scope, setter_id as usize as *mut c_void);
                    v8::FunctionTemplate::builder(call_python_property_setter)
                        .data(data.into())
                        .build(scope)
                });
                let key = v8::String::new(scope, &name).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Failed to create property name.")
                })?;

                if getter.is_some() || setter.is_some() {
                    prototype.set_accessor_property(
                        key.into(),
                        getter,
                        setter,
                        v8::PropertyAttribute::NONE,
                    );
                }
            }
        }
    }

    Ok(constructor_template)
}

fn register_class_members(
    py: Python<'_>,
    isolate: &crate::runtime::SharedIsolate,
    isolate_id: u64,
    context: &v8::Global<v8::Context>,
    members: Vec<ClassMember>,
) -> Vec<RegisteredClassMember> {
    members
        .into_iter()
        .map(|member| match member {
            ClassMember::Method { name, callable } => RegisteredClassMember::Method {
                name,
                id: register_host_function(py, isolate, isolate_id, context, callable),
            },
            ClassMember::Property {
                name,
                getter,
                setter,
            } => RegisteredClassMember::Property {
                name,
                getter_id: getter
                    .map(|getter| register_host_function(py, isolate, isolate_id, context, getter)),
                setter_id: setter
                    .map(|setter| register_host_function(py, isolate, isolate_id, context, setter)),
            },
        })
        .collect()
}

fn inspect_class_members(py: Python<'_>, cls: &Bound<'_, PyAny>) -> PyResult<Vec<ClassMember>> {
    let property_type = py.import("builtins")?.getattr("property")?;
    let dict = cls.getattr("__dict__")?;
    let items = dict.call_method0("items")?;
    let mut members = Vec::new();

    for item in items.try_iter()? {
        let item = item?;
        let (name, value): (String, Py<PyAny>) = item.extract()?;

        if name.starts_with('_') {
            continue;
        }

        let value = value.bind(py);
        if value.is_instance(&property_type)? {
            let getter = value.getattr("fget")?;
            let setter = value.getattr("fset")?;
            let getter = if getter.is_none() {
                None
            } else {
                Some(getter.clone().unbind())
            };
            let setter = if setter.is_none() {
                None
            } else {
                Some(setter.clone().unbind())
            };

            members.push(ClassMember::Property {
                name,
                getter,
                setter,
            });
        } else if value.is_callable() {
            members.push(ClassMember::Method {
                name,
                callable: value.clone().unbind(),
            });
        }
    }

    Ok(members)
}

fn call_python_class_constructor<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let result = Python::attach(|py| {
        invoke_python_class_constructor(py, scope, args).map_err(|err| err.to_string())
    });

    match result {
        Ok(value) => rv.set(value),
        Err(message) => throw_js_error(scope, &message),
    }
}

fn invoke_python_class_constructor<'s>(
    py: Python<'_>,
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
) -> PyResult<v8::Local<'s, v8::Value>> {
    if !args.is_construct_call() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "host class constructors must be called with 'new'.",
        ));
    }

    let host_function = host_function_from_callback_data(py, args.data())?;
    let py_args = PyTuple::new(py, python_arguments(py, scope, &args)?)?;
    let instance = host_function.callable.bind(py).call1(py_args)?;

    if is_python_awaitable(py, &instance)? {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "host class constructors cannot return awaitable objects.",
        ));
    }

    let token =
        runtime::register_external_for_isolate_id(host_function.isolate_id, instance.unbind());
    let external = v8::External::new(scope, token);
    let this = args.this();

    if !this.set_internal_field(0, external.into()) {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "Failed to attach Python instance to host object.",
        ));
    }

    Ok(this.into())
}

fn call_python_instance_method<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let result = Python::attach(|py| {
        invoke_python_instance_method(py, scope, args).map_err(|err| err.to_string())
    });

    match result {
        Ok(value) => rv.set(value),
        Err(message) => throw_js_error(scope, &message),
    }
}

fn invoke_python_instance_method<'s>(
    py: Python<'_>,
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
) -> PyResult<v8::Local<'s, v8::Value>> {
    let host_function = host_function_from_callback_data(py, args.data())?;
    let instance = python_instance_from_this(py, scope, args.this())?;
    let callable = host_function.callable.clone_ref(py);
    let mut py_args = Vec::with_capacity(args.length() as usize + 1);
    py_args.push(instance);
    py_args.extend(python_arguments(py, scope, &args)?);
    let py_args = PyTuple::new(py, py_args)?;
    let result = callable.bind(py).call1(py_args)?;

    if is_python_awaitable(py, &result)? {
        return python_awaitable_to_js_promise(py, scope, &host_function, result.as_any());
    }

    python_to_v8(py, scope, result.as_any(), host_function.isolate_id, 0)
}

fn call_python_property_getter<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let result = Python::attach(|py| {
        invoke_python_property_getter(py, scope, args).map_err(|err| err.to_string())
    });

    match result {
        Ok(value) => rv.set(value),
        Err(message) => throw_js_error(scope, &message),
    }
}

fn invoke_python_property_getter<'s>(
    py: Python<'_>,
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
) -> PyResult<v8::Local<'s, v8::Value>> {
    let host_function = host_function_from_callback_data(py, args.data())?;
    let instance = python_instance_from_this(py, scope, args.this())?;
    let result = host_function.callable.bind(py).call1((instance,))?;

    if is_python_awaitable(py, &result)? {
        return python_awaitable_to_js_promise(py, scope, &host_function, result.as_any());
    }

    python_to_v8(py, scope, result.as_any(), host_function.isolate_id, 0)
}

fn call_python_property_setter<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let result = Python::attach(|py| {
        invoke_python_property_setter(py, scope, args).map_err(|err| err.to_string())
    });

    match result {
        Ok(value) => rv.set(value),
        Err(message) => throw_js_error(scope, &message),
    }
}

fn invoke_python_property_setter<'s>(
    py: Python<'_>,
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
) -> PyResult<v8::Local<'s, v8::Value>> {
    let host_function = host_function_from_callback_data(py, args.data())?;
    let instance = python_instance_from_this(py, scope, args.this())?;
    let value = value_to_python(py, scope, args.get(0), 0)?;

    host_function.callable.bind(py).call1((instance, value))?;

    Ok(v8::undefined(scope).into())
}

fn host_function_from_callback_data(
    py: Python<'_>,
    data: v8::Local<'_, v8::Value>,
) -> PyResult<crate::runtime::ActiveHostFunction> {
    let data = v8::Local::<v8::External>::try_from(data).map_err(|_| {
        pyo3::exceptions::PyRuntimeError::new_err("Host class callback data is invalid.")
    })?;
    let id = data.value() as usize as u64;

    get_host_function(py, id).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Host class callback is no longer alive.")
    })
}

fn python_instance_from_this(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    this: v8::Local<'_, v8::Object>,
) -> PyResult<Py<PyAny>> {
    let data = this.get_internal_field(scope, 0).ok_or_else(|| {
        pyo3::exceptions::PyTypeError::new_err(
            "Host class method receiver is not a managed instance.",
        )
    })?;
    let external = v8::Local::<v8::External>::try_from(data).map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err(
            "Host class method receiver has invalid internal data.",
        )
    })?;

    runtime::external_payload(py, external.value()).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Host class instance is no longer alive.")
    })
}
