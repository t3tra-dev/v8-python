use pyo3::prelude::{Bound, PyAny, PyModule, PyResult, Python, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyModuleMethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::context::Context;
use crate::runtime::SharedIsolate;

mod atomics;
mod console;
mod dynamic_code_policy;
pub(crate) mod inspector;
mod microtask_queue;
pub(crate) mod module_loader;
mod promise_rejection_tracker;
mod shadow_realm;
mod shared_array_buffer;
mod timer;
mod web_assembly;

pub(crate) enum HostAPIDefinition {
    Timer,
    SharedArrayBuffer,
    Atomics(atomics::AtomicsDefinition),
    Console(console::ConsoleDefinition),
    DynamicCodePolicy(dynamic_code_policy::DynamicCodePolicyDefinition),
    Inspector(inspector::InspectorDefinition),
    ModuleLoader(module_loader::ModuleLoaderDefinition),
    PromiseRejectionTracker(promise_rejection_tracker::PromiseRejectionTrackerDefinition),
    MicrotaskQueue,
    ShadowRealm,
    WebAssembly(web_assembly::WebAssemblyDefinition),
}

impl HostAPIDefinition {
    pub(crate) fn clone_ref(&self, py: Python<'_>) -> Self {
        match self {
            Self::Timer => Self::Timer,
            Self::SharedArrayBuffer => Self::SharedArrayBuffer,
            Self::Atomics(definition) => Self::Atomics(definition.clone_ref()),
            Self::Console(definition) => Self::Console(definition.clone_ref(py)),
            Self::DynamicCodePolicy(definition) => Self::DynamicCodePolicy(definition.clone_ref()),
            Self::Inspector(definition) => Self::Inspector(definition.clone_ref()),
            Self::ModuleLoader(definition) => Self::ModuleLoader(definition.clone_ref(py)),
            Self::PromiseRejectionTracker(definition) => {
                Self::PromiseRejectionTracker(definition.clone_ref(py))
            }
            Self::MicrotaskQueue => Self::MicrotaskQueue,
            Self::ShadowRealm => Self::ShadowRealm,
            Self::WebAssembly(definition) => Self::WebAssembly(definition.clone_ref(py)),
        }
    }

    pub(crate) fn same_kind(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Timer, Self::Timer)
                | (Self::SharedArrayBuffer, Self::SharedArrayBuffer)
                | (Self::Atomics(_), Self::Atomics(_))
                | (Self::Console(_), Self::Console(_))
                | (Self::DynamicCodePolicy(_), Self::DynamicCodePolicy(_))
                | (Self::Inspector(_), Self::Inspector(_))
                | (Self::ModuleLoader(_), Self::ModuleLoader(_))
                | (
                    Self::PromiseRejectionTracker(_),
                    Self::PromiseRejectionTracker(_),
                )
                | (Self::MicrotaskQueue, Self::MicrotaskQueue)
                | (Self::ShadowRealm, Self::ShadowRealm)
                | (Self::WebAssembly(_), Self::WebAssembly(_))
        )
    }
}

/// Base class for host-side APIs that can be installed into a BaseProfile.
#[gen_stub_pyclass]
#[pyclass(module = "v8.api", name = "HostAPI", subclass)]
pub(crate) struct HostAPI;

#[gen_stub_pymethods]
#[pymethods]
impl HostAPI {}

pub(crate) fn install_api_module(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let api = PyModule::new(py, "v8.api")?;

    api.add_class::<HostAPI>()?;
    timer::add_class(&api)?;
    shared_array_buffer::add_class(&api)?;
    atomics::add_class(&api)?;
    console::add_class(&api)?;
    dynamic_code_policy::add_class(&api)?;
    inspector::add_class(&api)?;
    module_loader::add_class(&api)?;
    promise_rejection_tracker::add_class(&api)?;
    microtask_queue::add_class(&api)?;
    shadow_realm::add_class(&api)?;
    web_assembly::add_class(&api)?;
    parent.add_submodule(&api)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("v8.api", api)?;

    Ok(())
}

pub(crate) fn definition_from_python(api: &Bound<'_, PyAny>) -> PyResult<HostAPIDefinition> {
    if !api.is_instance_of::<HostAPI>() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "profile API must be a v8.api.HostAPI instance.",
        ));
    }

    if let Some(definition) = timer::definition_from_python(api) {
        return Ok(definition);
    }

    if let Some(definition) = shared_array_buffer::definition_from_python(api) {
        return Ok(definition);
    }

    if let Some(definition) = atomics::definition_from_python(api)? {
        return Ok(definition);
    }

    if let Some(definition) = console::definition_from_python(api)? {
        return Ok(definition);
    }

    if let Some(definition) = dynamic_code_policy::definition_from_python(api)? {
        return Ok(definition);
    }

    if let Some(definition) = inspector::definition_from_python(api)? {
        return Ok(definition);
    }

    if let Some(definition) = module_loader::definition_from_python(api)? {
        return Ok(definition);
    }

    if let Some(definition) = promise_rejection_tracker::definition_from_python(api)? {
        return Ok(definition);
    }

    if let Some(definition) = microtask_queue::definition_from_python(api) {
        return Ok(definition);
    }

    if let Some(definition) = shadow_realm::definition_from_python(api)? {
        return Ok(definition);
    }

    if let Some(definition) = web_assembly::definition_from_python(api)? {
        return Ok(definition);
    }

    Err(pyo3::exceptions::PyTypeError::new_err(
        "unsupported v8.api.HostAPI implementation.",
    ))
}

pub(crate) fn install(
    py: Python<'_>,
    context: &mut Context,
    api: &HostAPIDefinition,
) -> PyResult<()> {
    match api {
        HostAPIDefinition::Timer => timer::install(context),
        HostAPIDefinition::SharedArrayBuffer => shared_array_buffer::install(context),
        HostAPIDefinition::Atomics(_) => Ok(()),
        HostAPIDefinition::Console(definition) => console::install(py, context, definition),
        HostAPIDefinition::DynamicCodePolicy(definition) => {
            dynamic_code_policy::install(context, definition)
        }
        HostAPIDefinition::Inspector(definition) => inspector::install(context, definition),
        HostAPIDefinition::ModuleLoader(definition) => {
            module_loader::install(py, context, definition)
        }
        HostAPIDefinition::PromiseRejectionTracker(definition) => {
            promise_rejection_tracker::install(py, context, definition)
        }
        HostAPIDefinition::MicrotaskQueue => microtask_queue::install(context),
        HostAPIDefinition::ShadowRealm => shadow_realm::install(context),
        HostAPIDefinition::WebAssembly(definition) => {
            web_assembly::install(py, context, definition)
        }
    }
}

pub(crate) fn install_on_isolate(isolate: &SharedIsolate, api: &HostAPIDefinition) -> PyResult<()> {
    match api {
        HostAPIDefinition::Atomics(definition) => atomics::install_on_isolate(isolate, definition),
        HostAPIDefinition::ShadowRealm => shadow_realm::install_on_isolate(isolate),
        HostAPIDefinition::WebAssembly(_) => web_assembly::install_on_isolate(isolate),
        _ => Ok(()),
    }
}
