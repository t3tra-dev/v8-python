import asyncio

import v8

from tests.support import V8TestCase


class HostFunctionTests(V8TestCase):
    def test_context_builder_sets_initial_state(self):
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()

        builder.set_microtasks_policy("explicit")
        builder.set_global("payload", {"answer": 42})
        context = builder.build()

        self.assertFalse(builder.is_alive())
        self.assertEqual(context.eval("payload.answer").as_int32(), 42)

        promise = context.eval("Promise.resolve(41).then((value) => value + 1)")
        self.assertEqual(promise.promise_state(), "pending")

        context.perform_microtask_checkpoint()

        result = promise.promise_result()
        assert result is not None
        self.assertEqual(result.as_int32(), 42)

    def test_context_builder_registers_python_host_function(self):
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()

        @builder.host_function(name="add")
        def add(left: int, right: int) -> int:
            return left + right

        _ = add
        context = builder.build()

        self.assertEqual(context.eval("add(20, 22)").as_int32(), 42)

    def test_base_profile_registers_python_host_function(self):
        profile = v8.BaseProfile()

        @profile.host_function(name="host.double")
        def double(value: int) -> int:
            return value * 2

        _ = double
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.use_profile(profile)
        context = builder.build()

        self.assertEqual(context.eval("host.double(21)").as_int32(), 42)

    def test_python_async_host_function_returns_javascript_promise(self):
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.set_microtasks_policy("explicit")

        @builder.host_function(name="asyncAdd")
        async def async_add(left: int, right: int) -> int:
            await asyncio.sleep(0)
            return left + right

        _ = async_add
        context = builder.build()

        async def main():
            return await context.eval("asyncAdd(20, 22)")

        result = asyncio.run(main())

        self.assertIsInstance(result, v8.Value)
        self.assertEqual(result.as_int32(), 42)

    def test_python_host_function_can_return_javascript_promise(self):
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.set_microtasks_policy("explicit")
        values: dict[str, v8.Value] = {}

        @builder.host_function(name="returnPromise")
        def return_promise() -> v8.Value:
            return values["promise"]

        _ = return_promise
        context = builder.build()
        values["promise"] = context.eval(
            "Promise.resolve(41).then((value) => value + 1)"
        )

        async def main():
            return await context.eval("returnPromise()")

        result = asyncio.run(main())

        self.assertIsInstance(result, v8.Value)
        self.assertEqual(result.as_int32(), 42)
