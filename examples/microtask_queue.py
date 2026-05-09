import v8
from _shared import make_profile_context

context = make_profile_context([v8.api.MicrotaskQueue()])
context.set_microtasks_policy("explicit")

context.eval(
    """
    globalThis.events = [];
    queueMicrotask(() => events.push("microtask"));
    events.push("sync");
    """
)

print(context.eval("events.join(',')"))
context.perform_microtask_checkpoint()
print(context.eval("events.join(',')"))
