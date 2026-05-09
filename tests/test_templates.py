import asyncio

import v8

from tests.support import V8TestCase


class TemplateTests(V8TestCase):
    def test_context_builder_registers_python_class_template(self) -> None:
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()

        @builder.class_(name="Counter")
        class Counter:
            def __init__(self, value: int):
                self.value = value

            def add(self, amount: int) -> int:
                self.value += amount
                return self.value

            @property
            def current(self) -> int:
                return self.value

            @current.setter
            def current(self, value: int) -> None:
                self.value = value

        _ = Counter
        context = builder.build()

        self.assertTrue(
            context.eval(
                "globalThis.counter = new Counter(40); counter instanceof Counter"
            ).as_boolean()
        )
        self.assertEqual(context.eval("counter.add(2)").as_int32(), 42)
        self.assertEqual(context.eval("counter.current").as_int32(), 42)
        self.assertEqual(
            context.eval("counter.current = 10; counter.current").as_int32(), 10
        )

    def test_base_profile_registers_class_template_on_path(self) -> None:
        profile = v8.BaseProfile()

        @profile.class_(name="host.Box")
        class Box:
            def __init__(self, value: int):
                self.value = value

            def get(self) -> int:
                return self.value

        _ = Box
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.use_profile(profile)
        context = builder.build()

        self.assertEqual(profile.class_count(), 1)
        self.assertEqual(context.eval("new host.Box(42).get()").as_int32(), 42)

    def test_class_template_constructor_requires_new(self) -> None:
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()

        @builder.class_(name="RequiresNew")
        class RequiresNew:
            pass

        _ = RequiresNew
        context = builder.build()

        with self.assertRaises(v8.JavaScriptError):
            context.eval("RequiresNew()")

    def test_instance_method_can_return_javascript_promise(self) -> None:
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.set_microtasks_policy("explicit")

        @builder.class_(name="AsyncCounter")
        class AsyncCounter:
            def __init__(self, value: int):
                self.value = value

            async def add(self, amount: int) -> int:
                await asyncio.sleep(0)
                return self.value + amount

        _ = AsyncCounter
        context = builder.build()

        async def main() -> v8.Value:
            return await context.eval("new AsyncCounter(20).add(22)")

        result = asyncio.run(main())

        self.assertEqual(result.as_int32(), 42)
