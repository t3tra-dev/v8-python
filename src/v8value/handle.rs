use pyo3::PyResult;

use super::kind::{ValueKind, classify_value};
use crate::runtime::SharedIsolate;

#[derive(Clone)]
pub(crate) struct V8Value {
    pub(crate) value: v8::Global<v8::Value>,
    pub(crate) context: v8::Global<v8::Context>,
    pub(crate) isolate: SharedIsolate,
    pub(crate) isolate_id: u64,
    pub(crate) kind: ValueKind,
}

impl V8Value {
    pub(crate) fn from_local<'s>(
        scope: &v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Value>,
        context: v8::Global<v8::Context>,
        isolate: SharedIsolate,
        isolate_id: u64,
    ) -> Self {
        Self {
            value: v8::Global::new(scope, value),
            context,
            isolate,
            isolate_id,
            kind: classify_value(value),
        }
    }

    pub(crate) fn ensure_isolate(&self, isolate_id: u64) -> PyResult<()> {
        if self.isolate_id == isolate_id {
            return Ok(());
        }

        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "V8 Value belongs to a different Isolate.",
        ))
    }

    pub(crate) fn with_local_value<R>(
        &self,
        f: impl for<'s> FnOnce(&v8::PinScope<'s, '_>, v8::Local<'s, v8::Value>) -> PyResult<R>,
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
