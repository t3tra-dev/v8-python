use pyo3::PyClassInitializer;
use pyo3::prelude::{Bound, PyAny, PyModule, PyResult, pyclass, pymethods};
use pyo3::types::{PyAnyMethods, PyModuleMethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::host_apis::{HostAPI, HostAPIDefinition};
use crate::runtime::SharedIsolate;

#[derive(Clone, Copy)]
pub(crate) struct AtomicsDefinition {
    allow_wait: bool,
}

/// Configures V8 Atomics support, including whether blocking waits are allowed.
#[gen_stub_pyclass]
#[pyclass(extends = HostAPI, module = "v8.api", name = "Atomics")]
pub(crate) struct AtomicsAPI {
    allow_wait: bool,
}

impl AtomicsDefinition {
    pub(crate) fn clone_ref(&self) -> Self {
        *self
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl AtomicsAPI {
    /// Create an Atomics HostAPI configuration.
    #[gen_stub(override_return_type(type_repr = "Atomics", imports = ()))]
    #[new]
    #[pyo3(signature = (allow_wait = true))]
    fn new(allow_wait: bool) -> PyClassInitializer<Self> {
        PyClassInitializer::from(HostAPI).add_subclass(Self { allow_wait })
    }

    /// Return whether Atomics.wait is allowed to block.
    #[getter]
    fn allow_wait(&self) -> bool {
        self.allow_wait
    }
}

pub(crate) fn add_class(api_module: &Bound<'_, PyModule>) -> PyResult<()> {
    api_module.add_class::<AtomicsAPI>()
}

pub(crate) fn definition_from_python(
    api: &Bound<'_, PyAny>,
) -> PyResult<Option<HostAPIDefinition>> {
    if !api.is_instance_of::<AtomicsAPI>() {
        return Ok(None);
    }

    let atomics = api.extract::<pyo3::PyRef<'_, AtomicsAPI>>()?;

    Ok(Some(HostAPIDefinition::Atomics(AtomicsDefinition {
        allow_wait: atomics.allow_wait,
    })))
}

pub(crate) fn install_on_isolate(
    isolate: &SharedIsolate,
    definition: &AtomicsDefinition,
) -> PyResult<()> {
    isolate
        .borrow_mut()
        .set_allow_atomics_wait(definition.allow_wait);

    Ok(())
}
