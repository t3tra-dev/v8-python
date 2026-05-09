use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};

use pyo3::PyResult;

use crate::error::js_exception;
use crate::runtime::SharedIsolate;

pub(crate) type SharedTaskQueue = Rc<RefCell<TaskQueue>>;

pub(crate) struct TaskQueue {
    next_timer_id: u32,
    tasks: VecDeque<QueuedTask>,
    timers: HashMap<u32, Timer>,
    cancelled_timers: HashSet<u32>,
}

struct Timer {
    callback: v8::Global<v8::Function>,
    args: Vec<v8::Global<v8::Value>>,
    due_at: Instant,
    repeat: Option<Duration>,
}

struct QueuedTask {
    callback: v8::Global<v8::Function>,
    args: Vec<v8::Global<v8::Value>>,
    repeat: Option<(u32, Duration)>,
}

impl TaskQueue {
    fn new() -> Self {
        Self {
            next_timer_id: 1,
            tasks: VecDeque::new(),
            timers: HashMap::new(),
            cancelled_timers: HashSet::new(),
        }
    }

    pub(crate) fn set_timer(
        &mut self,
        scope: &v8::PinScope<'_, '_>,
        callback: v8::Local<'_, v8::Function>,
        args: Vec<v8::Local<'_, v8::Value>>,
        delay: Duration,
        repeat: Option<Duration>,
    ) -> u32 {
        let id = self.allocate_timer_id();
        let due_at = Instant::now() + delay;
        let args = args
            .into_iter()
            .map(|arg| v8::Global::new(scope, arg))
            .collect();

        self.timers.insert(
            id,
            Timer {
                callback: v8::Global::new(scope, callback),
                args,
                due_at,
                repeat,
            },
        );
        self.cancelled_timers.remove(&id);

        id
    }

    pub(crate) fn clear_timer(&mut self, id: u32) {
        self.timers.remove(&id);
        self.cancelled_timers.insert(id);
    }

    fn take_ready_task(&mut self) -> Option<QueuedTask> {
        if let Some(task) = self.tasks.pop_front() {
            return Some(task);
        }

        let now = Instant::now();
        let due_id = self
            .timers
            .iter()
            .filter(|(_, timer)| timer.due_at <= now)
            .min_by_key(|(_, timer)| timer.due_at)
            .map(|(id, _)| *id)?;
        let timer = self.timers.remove(&due_id)?;

        Some(QueuedTask {
            callback: timer.callback,
            args: timer.args,
            repeat: timer.repeat.map(|delay| (due_id, delay)),
        })
    }

    fn complete_task(&mut self, task: QueuedTask) {
        let Some((id, delay)) = task.repeat else {
            return;
        };

        if self.cancelled_timers.remove(&id) {
            return;
        }

        self.timers.insert(
            id,
            Timer {
                callback: task.callback,
                args: task.args,
                due_at: Instant::now() + delay,
                repeat: Some(delay),
            },
        );
    }

    fn next_timer_delay(&self) -> Option<Duration> {
        let now = Instant::now();

        self.timers
            .values()
            .map(|timer| timer.due_at.saturating_duration_since(now))
            .min()
    }

    fn allocate_timer_id(&mut self) -> u32 {
        let start = self.next_timer_id;

        loop {
            let id = self.next_timer_id;
            self.next_timer_id = self.next_timer_id.wrapping_add(1).max(1);

            if !self.timers.contains_key(&id) {
                return id;
            }

            if self.next_timer_id == start {
                self.next_timer_id = 1;
                return id;
            }
        }
    }
}

thread_local! {
    static TASK_QUEUES: RefCell<HashMap<u64, SharedTaskQueue>> = RefCell::new(HashMap::new());
}

pub(crate) fn register_task_queue(isolate_id: u64) -> SharedTaskQueue {
    TASK_QUEUES.with(|queues| {
        let mut queues = queues.borrow_mut();
        let queue = Rc::new(RefCell::new(TaskQueue::new()));

        queues.insert(isolate_id, queue.clone());
        queue
    })
}

pub(crate) fn unregister_task_queue(isolate_id: u64) {
    let _ = TASK_QUEUES.try_with(|queues| {
        queues.borrow_mut().remove(&isolate_id);
    });
}

pub(crate) fn task_queue(isolate_id: u64) -> Option<SharedTaskQueue> {
    TASK_QUEUES.with(|queues| queues.borrow().get(&isolate_id).cloned())
}

pub(crate) fn run_event_loop_once(
    isolate: &SharedIsolate,
    isolate_id: u64,
    context: &v8::Global<v8::Context>,
    timeout: Option<Duration>,
) -> PyResult<bool> {
    if pump_v8_message_loop(isolate) {
        return Ok(true);
    }

    if let Some(timeout) = timeout {
        wait_for_ready_task(isolate, isolate_id, timeout);
        if pump_v8_message_loop(isolate) {
            return Ok(true);
        }
    }

    let Some(queue) = task_queue(isolate_id) else {
        return Ok(false);
    };
    let Some(task) = queue.borrow_mut().take_ready_task() else {
        return Ok(false);
    };

    let result = execute_task(isolate, context, &task);
    queue.borrow_mut().complete_task(task);
    result?;

    Ok(true)
}

pub(crate) fn run_until_idle(
    isolate: &SharedIsolate,
    isolate_id: u64,
    context: &v8::Global<v8::Context>,
    max_tasks: Option<usize>,
) -> PyResult<usize> {
    let mut count = 0;

    while max_tasks.map(|max_tasks| count < max_tasks).unwrap_or(true) {
        if !run_event_loop_once(isolate, isolate_id, context, None)? {
            break;
        }

        count += 1;
    }

    Ok(count)
}

pub(crate) fn sleep_until_next_timer(isolate_id: u64, max_sleep: Duration) {
    if max_sleep.is_zero() {
        return;
    }

    let Some(queue) = task_queue(isolate_id) else {
        return;
    };
    let Some(delay) = queue.borrow().next_timer_delay() else {
        return;
    };

    let sleep_for = delay.min(max_sleep);
    if !sleep_for.is_zero() {
        std::thread::sleep(sleep_for);
    }
}

pub(crate) fn sleep_for_pending_background_task(isolate: &SharedIsolate, max_sleep: Duration) {
    if max_sleep.is_zero() || !isolate.borrow().has_pending_background_tasks() {
        return;
    }

    std::thread::sleep(max_sleep.min(Duration::from_millis(1)));
}

fn wait_for_ready_task(isolate: &SharedIsolate, isolate_id: u64, timeout: Duration) {
    if timeout.is_zero() || has_ready_task(isolate_id) {
        return;
    }

    if isolate.borrow().has_pending_background_tasks() {
        let sleep_for = timeout.min(Duration::from_millis(1));

        if !sleep_for.is_zero() {
            std::thread::sleep(sleep_for);
        }
        return;
    }

    let Some(queue) = task_queue(isolate_id) else {
        return;
    };
    let Some(delay) = queue.borrow().next_timer_delay() else {
        return;
    };

    let sleep_for = delay.min(timeout);
    if !sleep_for.is_zero() {
        std::thread::sleep(sleep_for);
    }
}

fn has_ready_task(isolate_id: u64) -> bool {
    let Some(queue) = task_queue(isolate_id) else {
        return false;
    };
    let queue = queue.borrow();

    !queue.tasks.is_empty()
        || queue
            .timers
            .values()
            .any(|timer| timer.due_at <= Instant::now())
}

fn pump_v8_message_loop(isolate: &SharedIsolate) -> bool {
    let mut isolate_ref = isolate.borrow_mut();
    let platform = v8::V8::get_current_platform();
    let ran_task = v8::Platform::pump_message_loop(&platform, &isolate_ref, false);

    if ran_task {
        isolate_ref.perform_microtask_checkpoint();
    }

    ran_task
}

fn execute_task(
    isolate: &SharedIsolate,
    context: &v8::Global<v8::Context>,
    task: &QueuedTask,
) -> PyResult<()> {
    let mut isolate_ref = isolate.borrow_mut();
    let result = {
        let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate_ref));
        let scope = &mut scope.init();
        let local_context = v8::Local::new(scope, context);
        let scope = &mut v8::ContextScope::new(scope, local_context);
        v8::tc_scope!(let scope, &mut **scope);
        let callback = v8::Local::new(scope, &task.callback);
        let recv = local_context.global(scope).into();
        let args = task
            .args
            .iter()
            .map(|arg| v8::Local::new(scope, arg))
            .collect::<Vec<_>>();

        callback
            .call(scope, recv, &args)
            .map(|_| ())
            .ok_or_else(|| js_exception(scope, "Task callback failed."))
    };

    isolate_ref.perform_microtask_checkpoint();

    result
}
