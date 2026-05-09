use pyo3::exceptions::{PyRuntimeError, PyStopIteration};
use pyo3::prelude::{Py, PyAny, PyRef, PyResult, Python, pyclass, pymethods};

use super::convert::symbol_to_string;
use super::handle::V8Value;
use super::kind::promise_state_name;
use super::value::Value;
use crate::event_loop;

#[pyclass(unsendable)]
pub(crate) struct PromiseAwaiter {
    handle: V8Value,
    done: bool,
}

enum PromisePoll {
    Pending,
    Fulfilled(Value),
    Rejected(String),
}

#[pymethods]
impl PromiseAwaiter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if self.done {
            return Err(PyStopIteration::new_err(py.None()));
        }

        match poll_promise(&self.handle)? {
            PromisePoll::Pending => Ok(py.None()),
            PromisePoll::Fulfilled(value) => {
                self.done = true;
                Err(PyStopIteration::new_err(Py::new(py, value)?))
            }
            PromisePoll::Rejected(reason) => {
                self.done = true;
                Err(PyRuntimeError::new_err(reason))
            }
        }
    }
}

impl PromiseAwaiter {
    pub(crate) fn new(handle: V8Value) -> Self {
        Self {
            handle,
            done: false,
        }
    }
}

fn poll_promise(handle: &V8Value) -> PyResult<PromisePoll> {
    event_loop::run_until_idle(
        &handle.isolate,
        handle.isolate_id,
        &handle.context,
        Some(1024),
    )?;

    let poll = {
        let mut isolate = handle.isolate.borrow_mut();
        isolate.perform_microtask_checkpoint();

        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate));
        let scope = &mut scope.init();
        let context = v8::Local::new(scope, &handle.context);
        let scope = &mut v8::ContextScope::new(scope, context);
        let value = v8::Local::new(scope, &handle.value);
        let promise = v8::Local::<v8::Promise>::try_from(value).map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("Value is not a JavaScript Promise.")
        })?;

        match promise.state() {
            v8::PromiseState::Pending => Ok(PromisePoll::Pending),
            v8::PromiseState::Fulfilled => {
                let result = promise.result(scope);
                Ok(PromisePoll::Fulfilled(Value::from_local(
                    scope,
                    result,
                    handle.context.clone(),
                    handle.isolate.clone(),
                    handle.isolate_id,
                )))
            }
            v8::PromiseState::Rejected => {
                let reason = promise.result(scope);
                Ok(PromisePoll::Rejected(format!(
                    "JavaScript Promise rejected: {}",
                    rejection_reason_to_string(scope, reason)?
                )))
            }
        }
    };

    if matches!(poll, Ok(PromisePoll::Pending)) {
        event_loop::sleep_until_next_timer(handle.isolate_id, std::time::Duration::from_millis(1));
        event_loop::sleep_for_pending_background_task(
            &handle.isolate,
            std::time::Duration::from_millis(1),
        );
    }

    poll
}

fn rejection_reason_to_string(
    scope: &v8::PinScope<'_, '_>,
    reason: v8::Local<'_, v8::Value>,
) -> PyResult<String> {
    if reason.is_undefined() {
        return Ok("undefined".to_owned());
    }

    if reason.is_null() {
        return Ok("null".to_owned());
    }

    if reason.is_symbol() {
        return symbol_to_string(scope, reason);
    }

    if let Some(reason) = reason.to_string(scope) {
        return Ok(reason.to_rust_string_lossy(scope));
    }

    Ok(format!(
        "<{}>",
        promise_state_name(&v8::PromiseState::Rejected)
    ))
}
