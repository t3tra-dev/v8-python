use pyo3::PyClassInitializer;
use pyo3::prelude::{Bound, PyAny, PyModule, PyResult, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyModuleMethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::context::Context;
use crate::host_apis::{HostAPI, HostAPIDefinition};
use crate::runtime::{self, SharedIsolate};

/// Enables V8's ShadowRealm support for contexts that install this HostAPI.
#[gen_stub_pyclass]
#[pyclass(extends = HostAPI, module = "v8.api", name = "ShadowRealm")]
pub(crate) struct ShadowRealmAPI;

#[gen_stub_pymethods]
#[pymethods]
impl ShadowRealmAPI {
    /// Create a HostAPI that enables and installs ShadowRealm support.
    #[gen_stub(override_return_type(type_repr = "ShadowRealm", imports = ()))]
    #[new]
    fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(HostAPI).add_subclass(Self)
    }
}

pub(crate) fn add_class(api_module: &Bound<'_, PyModule>) -> PyResult<()> {
    api_module.add_class::<ShadowRealmAPI>()
}

pub(crate) fn definition_from_python(
    api: &Bound<'_, PyAny>,
) -> PyResult<Option<HostAPIDefinition>> {
    if !api.is_instance_of::<ShadowRealmAPI>() {
        return Ok(None);
    }

    runtime::request_shadow_realm()?;

    Ok(Some(HostAPIDefinition::ShadowRealm))
}

pub(crate) fn install_on_isolate(isolate: &SharedIsolate) -> PyResult<()> {
    isolate
        .borrow_mut()
        .set_host_create_shadow_realm_context_callback(create_shadow_realm_context);

    Ok(())
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
    let key = v8::String::new(scope, "ShadowRealm").ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create ShadowRealm name.")
    })?;
    let shadow_realm = global.get(scope, key.into()).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to read ShadowRealm global.")
    })?;

    if !shadow_realm.is_function() {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "ShadowRealm is not available in this V8 build.",
        ));
    }

    Ok(())
}

fn create_shadow_realm_context<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
) -> Option<v8::Local<'s, v8::Context>> {
    Some(v8::Context::new(scope, Default::default()))
}
