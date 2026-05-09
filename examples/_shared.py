from collections.abc import Iterable

import v8


def make_context() -> v8.Context:
    return v8.Isolate().create_context()


def make_profile_context(apis: Iterable[v8.api.HostAPI]) -> v8.Context:
    profile = v8.BaseProfile().install(apis)
    isolate = v8.Isolate()
    builder = isolate.create_context_builder()
    builder.use_profile(profile)
    return builder.build()


def pump(context: v8.Context, steps: int = 32) -> None:
    for _ in range(steps):
        if not context.run_event_loop_once(timeout_ms=1):
            context.perform_microtask_checkpoint()
            return
        context.perform_microtask_checkpoint()


# fmt: off
# A minimal WebAssembly module exporting `answer() -> i32` with value 42.
WASM_ANSWER = bytes(
    [
        0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00,
        0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7F, 0x03,
        0x02, 0x01, 0x00, 0x07, 0x0A, 0x01, 0x06, 0x61,
        0x6E, 0x73, 0x77, 0x65, 0x72, 0x00, 0x00, 0x0A,
        0x06, 0x01, 0x04, 0x00, 0x41, 0x2A,0x0B,
    ]
)
# fmt: on
