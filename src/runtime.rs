use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::{ManuallyDrop, forget};
use std::ops::{Deref, DerefMut};
use std::rc::{Rc, Weak};
use std::sync::Once;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering as AtomicOrdering},
};
use std::time::Duration;

use pyo3::prelude::{Py, PyAny, PyResult, Python};

pub(crate) type SharedIsolate = Rc<RefCell<ManagedIsolate>>;

pub(crate) struct ActiveHostFunction {
    pub(crate) callable: Py<PyAny>,
    pub(crate) context: v8::Global<v8::Context>,
    pub(crate) isolate: SharedIsolate,
    pub(crate) isolate_id: u64,
}

struct RegisteredHostFunction {
    callable: Py<PyAny>,
    context: v8::Global<v8::Context>,
    isolate: Weak<RefCell<ManagedIsolate>>,
    isolate_id: u64,
}

struct RegisteredExternal {
    id: u64,
    payload: Py<PyAny>,
    isolate_id: u64,
}

pub(crate) struct ExecutionTimeout {
    cancelled: Arc<AtomicBool>,
}

impl ExecutionTimeout {
    pub(crate) fn arm(handle: v8::IsolateHandle, timeout_ms: Option<u64>) -> Option<Self> {
        let timeout_ms = timeout_ms.filter(|timeout_ms| *timeout_ms > 0)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let thread_cancelled = cancelled.clone();

        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(timeout_ms));

            if !thread_cancelled.load(AtomicOrdering::Acquire) {
                handle.terminate_execution();
            }
        });

        Some(Self { cancelled })
    }
}

impl Drop for ExecutionTimeout {
    fn drop(&mut self) {
        self.cancelled.store(true, AtomicOrdering::Release);
    }
}

pub(crate) struct ManagedIsolate {
    isolate_id: u64,
    isolate: ManuallyDrop<v8::OwnedIsolate>,
    host_function_ids: Vec<u64>,
    external_tokens: Vec<usize>,
}

impl ManagedIsolate {
    fn new(isolate_id: u64, isolate: v8::OwnedIsolate) -> Self {
        Self {
            isolate_id,
            isolate: ManuallyDrop::new(isolate),
            host_function_ids: Vec::new(),
            external_tokens: Vec::new(),
        }
    }

    pub(crate) fn track_host_function(&mut self, id: u64) {
        self.host_function_ids.push(id);
    }

    pub(crate) fn track_external(&mut self, token: usize) {
        self.external_tokens.push(token);
    }
}

impl Deref for ManagedIsolate {
    type Target = v8::OwnedIsolate;

    fn deref(&self) -> &Self::Target {
        &self.isolate
    }
}

impl DerefMut for ManagedIsolate {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.isolate
    }
}

impl Drop for ManagedIsolate {
    fn drop(&mut self) {
        crate::event_loop::unregister_task_queue(self.isolate_id);
        unregister_host_functions(&self.host_function_ids);
        unregister_externals(&self.external_tokens);
        unregister_untracked_externals_for_isolate(self.isolate_id);

        // SAFETY: `ManagedIsolate` owns the `OwnedIsolate` and this is its Drop.
        let mut isolate = Some(unsafe { ManuallyDrop::take(&mut self.isolate) });

        if ISOLATE_REGISTRY
            .try_with(|registry| {
                let isolate = isolate
                    .take()
                    .expect("ManagedIsolate::drop called without an isolate");
                registry
                    .borrow_mut()
                    .retire_isolate(self.isolate_id, isolate)
            })
            .is_err()
        {
            // During thread-local destruction the registry may already be gone.
            // Leaking here is preferable to panicking during interpreter shutdown.
            if let Some(isolate) = isolate.take() {
                forget(isolate);
            }
        }
    }
}

#[derive(Default)]
struct IsolateRegistry {
    creation_stack: Vec<u64>,
    pending: Vec<(u64, v8::OwnedIsolate)>,
    dropped_isolates: usize,
}

impl IsolateRegistry {
    fn register_isolate(&mut self, isolate_id: u64) {
        self.creation_stack.push(isolate_id);
    }

    fn retire_isolate(&mut self, isolate_id: u64, isolate: v8::OwnedIsolate) {
        self.pending.push((isolate_id, isolate));
    }

    fn drop_ready_isolates(&mut self) -> usize {
        let mut dropped = 0;

        while let Some(isolate_id) = self.creation_stack.last().copied() {
            let Some(index) = self
                .pending
                .iter()
                .position(|(pending_id, _)| *pending_id == isolate_id)
            else {
                break;
            };

            let (_, isolate) = self.pending.swap_remove(index);
            self.creation_stack.pop();
            drop(isolate);
            dropped += 1;
        }

        self.dropped_isolates += dropped;

        dropped
    }

    fn dropped_isolate_count(&self) -> usize {
        self.dropped_isolates
    }
}

impl Drop for IsolateRegistry {
    fn drop(&mut self) {
        self.drop_ready_isolates();

        for (_, isolate) in std::mem::take(&mut self.pending) {
            // If a newer isolate is still alive while the thread-local registry
            // is being destroyed, dropping an older pending isolate would violate
            // rusty_v8's reverse-drop rule. Avoid shutdown panic.
            forget(isolate);
        }
    }
}

static INIT_V8: Once = Once::new();
static V8_INITIALIZED: AtomicBool = AtomicBool::new(false);
static SHADOW_REALM_REQUESTED: AtomicBool = AtomicBool::new(false);
static NEXT_ISOLATE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_HOST_FUNCTION_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_EXTERNAL_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_SCRIPT_ID: AtomicI32 = AtomicI32::new(1);

thread_local! {
    static ISOLATE_REGISTRY: RefCell<IsolateRegistry> = RefCell::new(IsolateRegistry::default());
    static HOST_FUNCTION_REGISTRY: RefCell<HashMap<u64, RegisteredHostFunction>> = RefCell::new(HashMap::new());
    static EXTERNAL_REGISTRY: RefCell<HashMap<usize, RegisteredExternal>> = RefCell::new(HashMap::new());
}

pub(crate) fn init_v8_once() {
    INIT_V8.call_once(|| {
        let mut flags = "--expose_gc".to_owned();

        if SHADOW_REALM_REQUESTED.load(AtomicOrdering::Acquire) {
            flags.push_str(" --harmony-shadow-realm");
        }
        v8::V8::set_flags_from_string(&flags);

        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
        V8_INITIALIZED.store(true, AtomicOrdering::Release);
    });
}

pub(crate) fn request_shadow_realm() -> PyResult<()> {
    if V8_INITIALIZED.load(AtomicOrdering::Acquire)
        && !SHADOW_REALM_REQUESTED.load(AtomicOrdering::Acquire)
    {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "v8.api.ShadowRealm must be installed before the first v8.Isolate is created because V8 flags are process-global.",
        ));
    }

    SHADOW_REALM_REQUESTED.store(true, AtomicOrdering::Release);
    Ok(())
}

pub(crate) fn new_isolate_with_startup_data(
    startup_data: Option<v8::StartupData>,
) -> PyResult<(u64, SharedIsolate)> {
    init_v8_once();

    let params = if let Some(startup_data) = startup_data {
        if !startup_data.is_valid() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "snapshot is not valid for this V8 instance.",
            ));
        }

        v8::Isolate::create_params().snapshot_blob(startup_data)
    } else {
        v8::Isolate::create_params()
    };
    let mut isolate = v8::Isolate::new(params);
    isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 50);
    let isolate_id = NEXT_ISOLATE_ID.fetch_add(1, Ordering::Relaxed);
    ISOLATE_REGISTRY.with(|registry| registry.borrow_mut().register_isolate(isolate_id));

    Ok((
        isolate_id,
        Rc::new(RefCell::new(ManagedIsolate::new(isolate_id, isolate))),
    ))
}

pub(crate) fn next_script_id() -> i32 {
    NEXT_SCRIPT_ID.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn register_host_function(
    py: Python<'_>,
    isolate: &SharedIsolate,
    isolate_id: u64,
    context: &v8::Global<v8::Context>,
    callable: Py<PyAny>,
) -> u64 {
    let id = NEXT_HOST_FUNCTION_ID.fetch_add(1, Ordering::Relaxed);

    HOST_FUNCTION_REGISTRY.with(|registry| {
        registry.borrow_mut().insert(
            id,
            RegisteredHostFunction {
                callable: callable.clone_ref(py),
                context: context.clone(),
                isolate: Rc::downgrade(isolate),
                isolate_id,
            },
        );
    });
    isolate.borrow_mut().track_host_function(id);

    id
}

pub(crate) fn get_host_function(py: Python<'_>, id: u64) -> Option<ActiveHostFunction> {
    HOST_FUNCTION_REGISTRY.with(|registry| {
        let registry = registry.borrow();
        let function = registry.get(&id)?;
        let isolate = function.isolate.upgrade()?;

        Some(ActiveHostFunction {
            callable: function.callable.clone_ref(py),
            context: function.context.clone(),
            isolate,
            isolate_id: function.isolate_id,
        })
    })
}

fn unregister_host_functions(ids: &[u64]) {
    if ids.is_empty() {
        return;
    }

    let _ = HOST_FUNCTION_REGISTRY.try_with(|registry| {
        let mut registry = registry.borrow_mut();

        for id in ids {
            registry.remove(id);
        }
    });
}

pub(crate) fn register_external(isolate: &SharedIsolate, payload: Py<PyAny>) -> *mut c_void {
    let isolate_id = isolate.borrow().isolate_id;
    let id = NEXT_EXTERNAL_ID.fetch_add(1, Ordering::Relaxed);
    let token = Box::into_raw(Box::new(id)) as *mut c_void;
    let token_key = token as usize;

    register_external_entry(
        token_key,
        RegisteredExternal {
            id,
            payload,
            isolate_id,
        },
    );
    isolate.borrow_mut().track_external(token_key);

    token
}

pub(crate) fn register_external_for_isolate_id(isolate_id: u64, payload: Py<PyAny>) -> *mut c_void {
    let id = NEXT_EXTERNAL_ID.fetch_add(1, Ordering::Relaxed);
    let token = Box::into_raw(Box::new(id)) as *mut c_void;
    let token_key = token as usize;

    register_external_entry(
        token_key,
        RegisteredExternal {
            id,
            payload,
            isolate_id,
        },
    );

    token
}

fn register_external_entry(token_key: usize, entry: RegisteredExternal) {
    EXTERNAL_REGISTRY.with(|registry| {
        registry.borrow_mut().insert(token_key, entry);
    });
}

pub(crate) fn external_id(token: *mut c_void) -> Option<u64> {
    EXTERNAL_REGISTRY.with(|registry| {
        registry
            .borrow()
            .get(&(token as usize))
            .map(|entry| entry.id)
    })
}

pub(crate) fn external_payload(py: Python<'_>, token: *mut c_void) -> Option<Py<PyAny>> {
    EXTERNAL_REGISTRY.with(|registry| {
        registry
            .borrow()
            .get(&(token as usize))
            .map(|entry| entry.payload.clone_ref(py))
    })
}

fn unregister_externals(tokens: &[usize]) {
    if tokens.is_empty() {
        return;
    }

    let _ = EXTERNAL_REGISTRY.try_with(|registry| {
        let mut registry = registry.borrow_mut();

        for token in tokens {
            registry.remove(token);
        }
    });

    for token in tokens {
        if *token != 0 {
            // SAFETY: tokens are created by `Box::into_raw(Box<u64>)` in
            // `register_external` and each token is tracked once per isolate.
            unsafe {
                drop(Box::from_raw(*token as *mut u64));
            }
        }
    }
}

fn unregister_untracked_externals_for_isolate(isolate_id: u64) {
    let tokens = EXTERNAL_REGISTRY
        .try_with(|registry| {
            registry
                .borrow()
                .iter()
                .filter_map(|(token, entry)| (entry.isolate_id == isolate_id).then_some(*token))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    unregister_externals(&tokens);
}

pub(crate) fn collect_ready_isolates() -> usize {
    ISOLATE_REGISTRY.with(|registry| registry.borrow_mut().drop_ready_isolates())
}

pub(crate) fn try_collect_ready_isolates() -> Option<usize> {
    ISOLATE_REGISTRY
        .try_with(|registry| registry.borrow_mut().drop_ready_isolates())
        .ok()
}

pub(crate) fn dropped_isolate_count() -> usize {
    ISOLATE_REGISTRY.with(|registry| registry.borrow().dropped_isolate_count())
}
