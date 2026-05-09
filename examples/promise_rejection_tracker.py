import warnings

import v8
from _shared import make_profile_context

events: list[tuple[str, str | None]] = []
context = make_profile_context(
    [
        v8.api.PromiseRejectionTracker(
            policy="warn",
            callback=lambda event, reason: events.append((event, reason)),
        )
    ]
)

with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter("always", RuntimeWarning)
    context.eval("Promise.reject('boom')")
    context.perform_microtask_checkpoint()

print(caught[0].category.__name__)
print(caught[0].message)
print(events)
