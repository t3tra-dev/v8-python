import asyncio

import v8

from tests.support import V8TestCase

# fmt: off
WASM_EMPTY = bytes([0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00])
WASM_ANSWER = bytes(
    [
        0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00,
        0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7F, 0x03,
        0x02, 0x01, 0x00, 0x07, 0x0A, 0x01, 0x06, 0x61,
        0x6E, 0x73, 0x77, 0x65, 0x72, 0x00, 0x00, 0x0A,
        0x06, 0x01, 0x04, 0x00, 0x41, 0x2A, 0x0B,
    ]
)
# fmt: on


class WebAssemblyTests(V8TestCase):
    def test_context_compile_wasm_module_uses_cache(self):
        context = self.make_context()
        cache = v8.WasmModuleCache()

        module = context.compile_wasm_module(WASM_ANSWER, cache)
        self.assertIsInstance(module, v8.WasmModule)
        self.assertEqual(len(cache), 1)
        self.assertEqual(cache.misses, 1)
        self.assertEqual(cache.hits, 0)
        self.assertEqual(cache.stores, 1)
        self.assertTrue(cache.contains(memoryview(WASM_ANSWER)))
        self.assertIn(WASM_ANSWER, cache)
        self.assertNotIn(object(), cache)
        self.assertEqual(module.wire_bytes, WASM_ANSWER)

        cached = context.compile_wasm_module(bytearray(WASM_ANSWER), cache)
        self.assertEqual(len(cache), 1)
        self.assertEqual(cache.hits, 1)
        context.set_global("module", cached)
        self.assertEqual(
            context.eval(
                "new WebAssembly.Instance(module).exports.answer()"
            ).as_int32(),
            42,
        )

        compiled = module.get_compiled_module()
        self.assertIsInstance(compiled, v8.CompiledWasmModule)
        self.assertEqual(compiled.byte_length, len(WASM_ANSWER))
        self.assertEqual(compiled.wire_bytes, WASM_ANSWER)

        recreated = context.wasm_module_from_compiled(compiled)
        context.set_global("recreated", recreated)
        self.assertEqual(
            context.eval(
                "new WebAssembly.Instance(recreated).exports.answer()"
            ).as_int32(),
            42,
        )

    def test_webassembly_host_api_cache_is_used_by_compile_streaming(self):
        cache = v8.WasmModuleCache()
        context = self.make_webassembly_context(
            {"answer.wasm": WASM_ANSWER}, cache=cache
        )
        context.set_microtasks_policy("explicit")

        context.eval(
            """
            globalThis.answer = 0;
            WebAssembly.compileStreaming("answer.wasm").then((module) => {
              answer = new WebAssembly.Instance(module).exports.answer();
            });
            """
        )
        context.perform_microtask_checkpoint()
        self.assertEqual(context.eval("answer").as_int32(), 42)
        self.assertEqual(cache.misses, 1)
        self.assertEqual(cache.stores, 1)
        self.assertEqual(cache.hits, 0)

        context.eval('WebAssembly.compileStreaming("answer.wasm")')
        self.assertEqual(cache.hits, 1)
        self.assertEqual(len(cache), 1)

    def test_webassembly_host_api_installs_streaming_entry_points(self):
        bare_context = self.make_context()
        self.assertEqual(
            str(bare_context.eval("typeof WebAssembly.compileStreaming")),
            "undefined",
        )

        context = self.make_webassembly_context()
        self.assertEqual(
            str(context.eval("typeof WebAssembly.compileStreaming")),
            "function",
        )
        self.assertEqual(
            str(context.eval("typeof WebAssembly.instantiateStreaming")),
            "function",
        )

    def test_webassembly_streaming_compiles_from_loader(self):
        context = self.make_webassembly_context({"answer.wasm": WASM_ANSWER})
        context.set_microtasks_policy("explicit")
        context.eval(
            """
            globalThis.answer = 0;
            WebAssembly.compileStreaming("answer.wasm").then((module) => {
              const instance = new WebAssembly.Instance(module);
              answer = instance.exports.answer();
            });
            """
        )

        context.perform_microtask_checkpoint()
        for _ in range(100):
            if context.eval("answer").as_int32() == 42:
                break
            context.run_event_loop_once(timeout_ms=10)
            context.perform_microtask_checkpoint()
        self.assertEqual(context.eval("answer").as_int32(), 42)

    def test_webassembly_streaming_compiles_from_typed_array(self):
        context = self.make_webassembly_context()
        context.set_global("wasmBytes", list(WASM_ANSWER))
        context.set_microtasks_policy("explicit")
        context.eval(
            """
            globalThis.answer = 0;
            WebAssembly.compileStreaming(new Uint8Array(wasmBytes)).then((module) => {
              const instance = new WebAssembly.Instance(module);
              answer = instance.exports.answer();
            });
            """
        )

        context.perform_microtask_checkpoint()
        for _ in range(100):
            if context.eval("answer").as_int32() == 42:
                break
            context.run_event_loop_once(timeout_ms=10)
            context.perform_microtask_checkpoint()
        self.assertEqual(context.eval("answer").as_int32(), 42)

    def test_webassembly_instantiate_streaming_uses_loader(self):
        context = self.make_webassembly_context({"answer.wasm": WASM_ANSWER})
        context.set_microtasks_policy("explicit")
        context.eval(
            """
            globalThis.answer = 0;
            WebAssembly.instantiateStreaming("answer.wasm").then((result) => {
              answer = result.instance.exports.answer();
            });
            """
        )

        context.perform_microtask_checkpoint()
        for _ in range(100):
            if context.eval("answer").as_int32() == 42:
                break
            context.run_event_loop_once(timeout_ms=10)
            context.perform_microtask_checkpoint()
        self.assertEqual(context.eval("answer").as_int32(), 42)

    def test_webassembly_policy_can_disallow_code_generation(self):
        context = self.make_webassembly_context(allow_code_generation=False)
        context.set_global("wasmBytes", list(WASM_EMPTY))

        with self.assertRaises(RuntimeError) as raised:
            context.eval("new WebAssembly.Module(new Uint8Array(wasmBytes))")

        self.assertIn("Wasm code generation", str(raised.exception))

    def test_webassembly_async_compile_can_be_pumped(self):
        context = self.make_webassembly_context()
        context.set_global("wasmBytes", list(WASM_EMPTY))
        context.set_microtasks_policy("explicit")
        promise = context.eval("WebAssembly.compile(new Uint8Array(wasmBytes))")

        for _ in range(100):
            if promise.promise_state() != "pending":
                break
            context.run_event_loop_once(timeout_ms=10)
            context.perform_microtask_checkpoint()

        self.assertEqual(promise.promise_state(), "fulfilled")

    def test_webassembly_async_compile_can_be_awaited(self):
        context = self.make_webassembly_context()
        context.set_global("wasmBytes", list(WASM_ANSWER))
        context.set_microtasks_policy("explicit")

        async def main():
            return await context.eval("WebAssembly.compile(new Uint8Array(wasmBytes))")

        module = asyncio.run(main())

        context.set_global("module", module)
        self.assertEqual(
            context.eval(
                "new WebAssembly.Instance(module).exports.answer()"
            ).as_int32(),
            42,
        )
