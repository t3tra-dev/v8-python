import asyncio

import v8
from _shared import WASM_ANSWER, make_profile_context, pump

cache = v8.WasmModuleCache()
context = make_profile_context(
    [v8.api.WebAssembly({"answer.wasm": WASM_ANSWER}, cache=cache)]
)
context.set_microtasks_policy("explicit")

module = context.compile_wasm_module(WASM_ANSWER, cache)
context.set_global("module", module)
print(context.eval("new WebAssembly.Instance(module).exports.answer()"))

context.eval(
    """
    globalThis.streamingAnswer = 0;
    WebAssembly.compileStreaming("answer.wasm").then((module) => {
      streamingAnswer = new WebAssembly.Instance(module).exports.answer();
    });
    """
)
pump(context)
print(context.eval("streamingAnswer"))
print(f"cache hits={cache.hits} misses={cache.misses} stores={cache.stores}")


async def main():
    return await context.eval("WebAssembly.compile(new Uint8Array(wasmBytes))")


context.set_global("wasmBytes", list(WASM_ANSWER))
print(type(asyncio.run(main())).__name__)
