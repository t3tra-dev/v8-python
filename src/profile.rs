use std::convert::TryFrom;
use std::ffi::c_void;

use pyo3::prelude::{Bound, Py, PyAny, PyResult, Python, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyTuple};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::context::{Context, ContextBuilder};
use crate::host_apis::{self, HostAPIDefinition};
use crate::runtime::{
    ActiveHostFunction, SharedIsolate, get_host_function, register_host_function,
};
use crate::templates::HostClassDefinition;
use crate::v8value::{python_to_v8, value_to_python};

#[derive(Debug)]
pub(crate) struct HostFunctionDefinition {
    pub(crate) name: String,
    pub(crate) callable: Py<PyAny>,
}

impl HostFunctionDefinition {
    pub(crate) fn new(py: Python<'_>, name: Option<String>, callable: Py<PyAny>) -> PyResult<Self> {
        let callable = callable.bind(py);

        if !callable.is_callable() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "host function must be callable.",
            ));
        }

        let name = match name {
            Some(name) if !name.trim().is_empty() => name,
            Some(_) => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "host function name cannot be empty.",
                ));
            }
            None => callable.getattr("__name__")?.extract()?,
        };

        Ok(Self {
            name,
            callable: callable.clone().unbind(),
        })
    }

    pub(crate) fn clone_ref(&self, py: Python<'_>) -> Self {
        Self {
            name: self.name.clone(),
            callable: self.callable.clone_ref(py),
        }
    }
}

/// Reusable collection of host functions, host classes, and HostAPI installers.
#[gen_stub_pyclass]
#[pyclass(unsendable, subclass)]
pub(crate) struct BaseProfile {
    host_functions: Vec<HostFunctionDefinition>,
    host_classes: Vec<HostClassDefinition>,
    host_apis: Vec<HostAPIDefinition>,
}

enum HostFunctionOwner {
    Builder(Py<ContextBuilder>),
    Profile(Py<BaseProfile>),
}

enum HostClassOwner {
    Builder(Py<ContextBuilder>),
    Profile(Py<BaseProfile>),
}

#[pyclass(unsendable)]
struct HostFunctionDecorator {
    owner: HostFunctionOwner,
    name: Option<String>,
}

#[pyclass(unsendable)]
struct HostClassDecorator {
    owner: HostClassOwner,
    name: Option<String>,
}

#[pyclass(unsendable)]
struct HostPromiseResolver {
    resolver: v8::Global<v8::PromiseResolver>,
    context: v8::Global<v8::Context>,
    isolate: SharedIsolate,
    isolate_id: u64,
    settled: bool,
}

#[gen_stub_pymethods]
#[pymethods]
impl BaseProfile {
    /// Create an empty profile.
    #[new]
    fn new() -> Self {
        Self {
            host_functions: Vec::new(),
            host_classes: Vec::new(),
            host_apis: Vec::new(),
        }
    }

    /// Register a Python callable as a JavaScript global function.
    ///
    /// Can be used directly or as a decorator. When `name` is omitted, the
    /// callable's `__name__` is used as the JavaScript global name.
    #[gen_stub(override_return_type(type_repr = "_HostCallable | _HostFunctionDecorator", imports = ()))]
    #[pyo3(signature = (function=None, *, name=None))]
    fn host_function(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        #[gen_stub(override_type(type_repr = "_HostCallable | None", imports = ()))]
        function: Option<Py<PyAny>>,
        name: Option<String>,
    ) -> PyResult<Py<PyAny>> {
        if let Some(function) = function {
            slf.borrow_mut()
                .add_host_function(py, name, function.clone_ref(py))?;
            return Ok(function);
        }

        Ok(Py::new(
            py,
            HostFunctionDecorator {
                owner: HostFunctionOwner::Profile(slf.clone().unbind()),
                name,
            },
        )?
        .into_any())
    }

    /// Register a Python class as a JavaScript constructor template.
    ///
    /// Can be used directly or as a decorator. Methods and properties visible
    /// on the Python class are exposed through V8 object templates.
    #[gen_stub(override_return_type(type_repr = "_HostClassDecorator", imports = ()))]
    #[pyo3(name = "class_", signature = (cls=None, *, name=None))]
    fn class_(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        #[gen_stub(override_type(type_repr = "_HostClassLike | None", imports = ()))] cls: Option<
            Py<PyAny>,
        >,
        name: Option<String>,
    ) -> PyResult<Py<PyAny>> {
        if let Some(cls) = cls {
            slf.borrow_mut()
                .add_host_class(py, name, cls.clone_ref(py))?;
            return Ok(cls);
        }

        Ok(Py::new(
            py,
            HostClassDecorator {
                owner: HostClassOwner::Profile(slf.clone().unbind()),
                name,
            },
        )?
        .into_any())
    }

    /// Return the number of registered host functions.
    fn host_function_count(&self) -> usize {
        self.host_functions.len()
    }

    /// Return the number of registered host classes.
    fn class_count(&self) -> usize {
        self.host_classes.len()
    }

    /// Install HostAPI instances into this profile and return the profile.
    fn install(
        slf: &Bound<'_, Self>,
        #[gen_stub(override_type(type_repr = "collections.abc.Iterable[api.HostAPI]", imports = ("collections.abc")))]
        apis: &Bound<'_, PyAny>,
    ) -> PyResult<Py<Self>> {
        let profile = slf.clone().unbind();
        {
            let mut profile_ref = slf.borrow_mut();

            for api in apis.try_iter()? {
                profile_ref.add_host_api(host_apis::definition_from_python(&api?)?);
            }
        }

        Ok(profile)
    }
}

impl BaseProfile {
    pub(crate) fn add_host_function(
        &mut self,
        py: Python<'_>,
        name: Option<String>,
        function: Py<PyAny>,
    ) -> PyResult<()> {
        self.host_functions
            .push(HostFunctionDefinition::new(py, name, function)?);
        Ok(())
    }

    pub(crate) fn host_functions(&self) -> &[HostFunctionDefinition] {
        &self.host_functions
    }

    pub(crate) fn add_host_class(
        &mut self,
        py: Python<'_>,
        name: Option<String>,
        cls: Py<PyAny>,
    ) -> PyResult<()> {
        self.host_classes
            .push(HostClassDefinition::new(py, name, cls)?);
        Ok(())
    }

    pub(crate) fn host_classes(&self) -> &[HostClassDefinition] {
        &self.host_classes
    }

    pub(crate) fn add_host_api(&mut self, api: HostAPIDefinition) {
        if !self
            .host_apis
            .iter()
            .any(|installed| installed.same_kind(&api))
        {
            self.host_apis.push(api);
        }
    }

    pub(crate) fn host_apis(&self) -> &[HostAPIDefinition] {
        &self.host_apis
    }
}

#[pymethods]
impl HostFunctionDecorator {
    fn __call__(&self, py: Python<'_>, function: Py<PyAny>) -> PyResult<Py<PyAny>> {
        match &self.owner {
            HostFunctionOwner::Builder(builder) => {
                builder.bind(py).borrow_mut().add_host_function(
                    py,
                    self.name.clone(),
                    function.clone_ref(py),
                )?;
            }
            HostFunctionOwner::Profile(profile) => {
                profile.bind(py).borrow_mut().add_host_function(
                    py,
                    self.name.clone(),
                    function.clone_ref(py),
                )?;
            }
        }

        Ok(function)
    }
}

#[pymethods]
impl HostClassDecorator {
    fn __call__(&self, py: Python<'_>, cls: Py<PyAny>) -> PyResult<Py<PyAny>> {
        match &self.owner {
            HostClassOwner::Builder(builder) => {
                builder.bind(py).borrow_mut().add_host_class(
                    py,
                    self.name.clone(),
                    cls.clone_ref(py),
                )?;
            }
            HostClassOwner::Profile(profile) => {
                profile.bind(py).borrow_mut().add_host_class(
                    py,
                    self.name.clone(),
                    cls.clone_ref(py),
                )?;
            }
        }

        Ok(cls)
    }
}

#[pymethods]
impl HostPromiseResolver {
    fn __call__(&mut self, py: Python<'_>, future: &Bound<'_, PyAny>) -> PyResult<()> {
        if self.settled {
            return Ok(());
        }
        self.settled = true;

        let result = if future.call_method0("cancelled")?.extract()? {
            Err("Python awaitable was cancelled.".to_owned())
        } else {
            future.call_method0("result").map_err(|err| err.to_string())
        };

        let mut isolate = self.isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate));
        let scope = &mut scope.init();
        let context = v8::Local::new(scope, &self.context);
        let scope = &mut v8::ContextScope::new(scope, context);
        let resolver = v8::Local::new(scope, &self.resolver);

        match result {
            Ok(value) => {
                let value = match python_to_v8(py, scope, &value, self.isolate_id, 0) {
                    Ok(value) => value,
                    Err(err) => {
                        reject_with_message(scope, resolver, &err.to_string());
                        return Ok(());
                    }
                };
                resolver.resolve(scope, value);
            }
            Err(message) => {
                reject_with_message(scope, resolver, &message);
            }
        }

        Ok(())
    }
}

pub(crate) fn builder_host_function(
    py: Python<'_>,
    builder: &Bound<'_, ContextBuilder>,
    function: Option<Py<PyAny>>,
    name: Option<String>,
) -> PyResult<Py<PyAny>> {
    if let Some(function) = function {
        builder
            .borrow_mut()
            .add_host_function(py, name, function.clone_ref(py))?;
        return Ok(function);
    }

    Ok(Py::new(
        py,
        HostFunctionDecorator {
            owner: HostFunctionOwner::Builder(builder.clone().unbind()),
            name,
        },
    )?
    .into_any())
}

pub(crate) fn builder_host_class(
    py: Python<'_>,
    builder: &Bound<'_, ContextBuilder>,
    cls: Option<Py<PyAny>>,
    name: Option<String>,
) -> PyResult<Py<PyAny>> {
    if let Some(cls) = cls {
        builder
            .borrow_mut()
            .add_host_class(py, name, cls.clone_ref(py))?;
        return Ok(cls);
    }

    Ok(Py::new(
        py,
        HostClassDecorator {
            owner: HostClassOwner::Builder(builder.clone().unbind()),
            name,
        },
    )?
    .into_any())
}

pub(crate) fn install_host_function(
    py: Python<'_>,
    context: &mut Context,
    definition: &HostFunctionDefinition,
) -> PyResult<()> {
    let isolate = context
        .isolate
        .as_ref()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive."))?;
    let context_global = context
        .context
        .as_ref()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Context is no longer alive."))?;

    let id = register_host_function(
        py,
        isolate,
        context.isolate_id,
        context_global,
        definition.callable.clone_ref(py),
    );

    let mut isolate_ref = isolate.borrow_mut();
    let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
    let scope = &mut scope.init();
    let local_context = v8::Local::new(scope, context_global);
    let scope = &mut v8::ContextScope::new(scope, local_context);

    let data = v8::External::new(scope, id as usize as *mut c_void);
    let function = v8::Function::builder(call_python_host_function)
        .data(data.into())
        .build(scope)
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create host function.")
        })?;
    let function_name = last_path_segment(&definition.name)?;
    let function_name = v8::String::new(scope, function_name).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create host function name.")
    })?;
    function.set_name(function_name);

    set_global_path(scope, local_context, &definition.name, function.into())
}

fn call_python_host_function<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let result = Python::attach(|py| {
        invoke_python_host_function(py, scope, args).map_err(|err| err.to_string())
    });

    match result {
        Ok(value) => rv.set(value),
        Err(message) => throw_js_error(scope, &message),
    }
}

fn invoke_python_host_function<'s>(
    py: Python<'_>,
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
) -> PyResult<v8::Local<'s, v8::Value>> {
    let data = args.data();
    let data = v8::Local::<v8::External>::try_from(data).map_err(|_| {
        pyo3::exceptions::PyRuntimeError::new_err("Host function callback data is invalid.")
    })?;
    let id = data.value() as usize as u64;
    let host_function = get_host_function(py, id).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Host function is no longer alive.")
    })?;
    let callable = host_function.callable.clone_ref(py);
    let py_args = python_arguments(py, scope, &args)?;
    let py_args = PyTuple::new(py, py_args)?;
    let result = callable.bind(py).call1(py_args)?;

    if is_python_awaitable(py, &result)? {
        return python_awaitable_to_js_promise(py, scope, &host_function, result.as_any());
    }

    python_to_v8(py, scope, result.as_any(), host_function.isolate_id, 0)
}

pub(crate) fn python_arguments(
    py: Python<'_>,
    scope: &v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments<'_>,
) -> PyResult<Vec<Py<PyAny>>> {
    (0..args.length())
        .map(|index| value_to_python(py, scope, args.get(index), 0))
        .collect()
}

pub(crate) fn python_awaitable_to_js_promise<'s>(
    py: Python<'_>,
    scope: &mut v8::PinScope<'s, '_>,
    host_function: &ActiveHostFunction,
    awaitable: &Bound<'_, PyAny>,
) -> PyResult<v8::Local<'s, v8::Value>> {
    let resolver = v8::PromiseResolver::new(scope).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create Promise resolver.")
    })?;
    let promise = resolver.get_promise(scope);
    let host_resolver = Py::new(
        py,
        HostPromiseResolver {
            resolver: v8::Global::new(scope, resolver),
            context: host_function.context.clone(),
            isolate: host_function.isolate.clone(),
            isolate_id: host_function.isolate_id,
            settled: false,
        },
    )?;

    match schedule_python_awaitable(py, awaitable, host_resolver) {
        Ok(()) => {}
        Err(err) => reject_with_message(scope, resolver, &err.to_string()),
    }

    Ok(promise.into())
}

fn schedule_python_awaitable(
    py: Python<'_>,
    awaitable: &Bound<'_, PyAny>,
    resolver: Py<HostPromiseResolver>,
) -> PyResult<()> {
    let asyncio = py.import("asyncio")?;
    let future = asyncio.call_method1("ensure_future", (awaitable,))?;
    future.call_method1("add_done_callback", (resolver,))?;
    Ok(())
}

pub(crate) fn is_python_awaitable(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    py.import("inspect")?
        .call_method1("isawaitable", (value,))?
        .extract()
}

pub(crate) fn set_global_path(
    scope: &mut v8::PinScope<'_, '_>,
    context: v8::Local<'_, v8::Context>,
    path: &str,
    value: v8::Local<'_, v8::Value>,
) -> PyResult<()> {
    let segments = path_segments(path)?;
    let global = context.global(scope);
    let mut target = global;

    for segment in &segments[..segments.len() - 1] {
        let key = v8::String::new(scope, segment).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create host function path.")
        })?;
        let existing = target.get(scope, key.into());
        let object = existing
            .and_then(|value| {
                if value.is_object() && !value.is_null() {
                    value.to_object(scope)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                let object = v8::Object::new(scope);
                target.set(scope, key.into(), object.into());
                object
            });

        target = object;
    }

    let key = v8::String::new(scope, segments[segments.len() - 1]).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create host function key.")
    })?;

    target
        .set(scope, key.into(), value)
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Failed to set host function."))
        .map(|_| ())
}

fn path_segments(path: &str) -> PyResult<Vec<&str>> {
    let segments = path.split('.').collect::<Vec<_>>();

    if segments.is_empty() || segments.iter().any(|segment| segment.is_empty()) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "host function name must contain non-empty path segments.",
        ));
    }

    Ok(segments)
}

pub(crate) fn last_path_segment(path: &str) -> PyResult<&str> {
    path_segments(path).map(|segments| segments[segments.len() - 1])
}

pub(crate) fn throw_js_error(scope: &mut v8::PinScope<'_, '_>, message: &str) {
    let message = v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::error(scope, message);
    scope.throw_exception(exception);
}

fn reject_with_message(
    scope: &mut v8::PinScope<'_, '_>,
    resolver: v8::Local<'_, v8::PromiseResolver>,
    message: &str,
) {
    let message = v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
    let exception = v8::Exception::error(scope, message);
    resolver.reject(scope, exception);
}
