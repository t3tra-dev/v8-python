import v8
from _shared import make_profile_context

context = make_profile_context(
    [
        v8.api.SharedArrayBuffer(),
        v8.api.Atomics(allow_wait=True),
    ]
)

print(
    context.eval(
        """
        const buffer = SharedArrayBuffer.fromHost(4);
        const view = new Int32Array(buffer);
        view[0] = 7;
        const previous = Atomics.add(view, 0, 5);
        `${previous} -> ${view[0]} (${buffer.byteLength} bytes)`;
        """
    )
)
print(
    context.eval("Atomics.wait(new Int32Array(SharedArrayBuffer.fromHost(4)), 0, 0, 0)")
)
