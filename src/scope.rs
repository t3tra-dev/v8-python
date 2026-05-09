use pyo3::prelude::{PyRef, PyResult, pyclass, pymethods};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use super::runtime::{SharedIsolate, next_script_id};
use super::script::Script;
use super::v8value::V8String;

/// Low-level V8 handle scope used for creating strings and compiling a single script.
#[gen_stub_pyclass]
#[pyclass(unsendable)]
pub(super) struct Scope {
    pub(crate) isolate: Option<SharedIsolate>,
    pub(crate) isolate_id: u64,
}

#[gen_stub_pymethods]
#[pymethods]
impl Scope {
    /// Return whether this scope still owns its isolate.
    fn is_alive(&self) -> bool {
        self.isolate.is_some()
    }

    /// Create a V8 string in this scope.
    fn new_string(&mut self, value: &str) -> PyResult<V8String> {
        let isolate = self.isolate.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Scope is no longer alive.")
        })?;

        let mut isolate_ref = isolate.borrow_mut();
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();

        let string = v8::String::new(scope, value).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Failed to create v8::String.")
        })?;

        Ok(V8String {
            value: v8::Global::new(scope, string),
            text: value.to_owned(),
            isolate: isolate.clone(),
            isolate_id: self.isolate_id,
            handle: None,
        })
    }

    /// Compile a string into a script, consuming this scope.
    fn compile(&mut self, source: PyRef<'_, V8String>) -> PyResult<Script> {
        source.ensure_isolate(self.isolate_id)?;

        let isolate = self.isolate.take().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Scope is no longer alive.")
        })?;

        let script_id = next_script_id();

        let (context, script, unbound_script, source_url, source_mapping_url) = {
            let mut isolate_ref = isolate.borrow_mut();
            let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
            let scope = &mut scope.init();
            let context = v8::Context::new(scope, Default::default());
            let context_global = v8::Global::new(scope, context);
            let scope = &mut v8::ContextScope::new(scope, context);

            let source_code = v8::Local::new(scope, &source.value);
            let resource_name = v8::String::new(scope, "<scope>").ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("Failed to create script resource name.")
            })?;
            let origin = v8::ScriptOrigin::new(
                scope,
                resource_name.into(),
                0,
                0,
                false,
                script_id,
                None,
                false,
                false,
                false,
                None,
            );

            let script =
                v8::Script::compile(scope, source_code, Some(&origin)).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("v8::Script::compile returned None.")
                })?;
            let unbound_script = script.get_unbound_script(scope);
            let source_url = super::script::optional_value_to_string(
                scope,
                unbound_script.get_source_url(scope),
            );
            let source_mapping_url = super::script::optional_value_to_string(
                scope,
                unbound_script.get_source_mapping_url(scope),
            );
            let script_global = v8::Global::new(scope, script);
            let unbound_script_global = v8::Global::new(scope, unbound_script);

            (
                context_global,
                script_global,
                unbound_script_global,
                source_url,
                source_mapping_url,
            )
        };

        Ok(Script {
            script,
            unbound_script,
            context,
            isolate,
            isolate_id: self.isolate_id,
            source: source.text.clone(),
            script_id,
            resource_name: Some("<scope>".to_owned()),
            source_map_url: None,
            source_url,
            source_mapping_url,
            cached_data_rejected: false,
        })
    }
}
