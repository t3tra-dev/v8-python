use pyo3::PyClassInitializer;
use pyo3::prelude::{Bound, PyAny, PyModule, PyResult, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyModuleMethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::context::Context;
use crate::host_apis::{HostAPI, HostAPIDefinition};

#[derive(Clone, Copy)]
pub(crate) struct DynamicCodePolicyDefinition {
    allow_eval: bool,
}

/// Controls whether dynamic JavaScript code generation is allowed.
#[gen_stub_pyclass]
#[pyclass(extends = HostAPI, module = "v8.api", name = "DynamicCodePolicy")]
pub(crate) struct DynamicCodePolicyAPI {
    allow_eval: bool,
}

impl DynamicCodePolicyDefinition {
    pub(crate) fn clone_ref(&self) -> Self {
        *self
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl DynamicCodePolicyAPI {
    /// Create a dynamic code policy HostAPI.
    #[gen_stub(override_return_type(type_repr = "DynamicCodePolicy", imports = ()))]
    #[new]
    #[pyo3(signature = (allow_eval = false))]
    fn new(allow_eval: bool) -> PyClassInitializer<Self> {
        PyClassInitializer::from(HostAPI).add_subclass(Self { allow_eval })
    }

    /// Return whether eval and Function-style dynamic code generation are allowed.
    #[getter]
    fn allow_eval(&self) -> bool {
        self.allow_eval
    }
}

pub(crate) fn add_class(api_module: &Bound<'_, PyModule>) -> PyResult<()> {
    api_module.add_class::<DynamicCodePolicyAPI>()
}

pub(crate) fn definition_from_python(
    api: &Bound<'_, PyAny>,
) -> PyResult<Option<HostAPIDefinition>> {
    if !api.is_instance_of::<DynamicCodePolicyAPI>() {
        return Ok(None);
    }

    let policy = api.extract::<pyo3::PyRef<'_, DynamicCodePolicyAPI>>()?;

    Ok(Some(HostAPIDefinition::DynamicCodePolicy(
        DynamicCodePolicyDefinition {
            allow_eval: policy.allow_eval,
        },
    )))
}

pub(crate) fn install(
    context: &mut Context,
    definition: &DynamicCodePolicyDefinition,
) -> PyResult<()> {
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
    local_context.set_allow_generation_from_strings(definition.allow_eval);

    Ok(())
}
