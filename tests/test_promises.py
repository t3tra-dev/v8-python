import asyncio
import warnings

import v8

from tests.support import V8TestCase


class PromiseTests(V8TestCase):
    def test_promise_microtask_checkpoint(self):
        context = self.make_context()
        context.set_microtasks_policy("explicit")

        promise = context.eval("Promise.resolve(20).then((value) => value + 22)")
        self.assertEqual(promise.promise_state(), "pending")

        context.perform_microtask_checkpoint()

        self.assertEqual(promise.promise_state(), "fulfilled")
        result = promise.promise_result()
        assert result is not None
        self.assertEqual(result.as_int32(), 42)

    def test_value_promise_can_be_awaited(self):
        context = self.make_context()
        context.set_microtasks_policy("explicit")

        async def main():
            return await context.eval("Promise.resolve(41).then((value) => value + 1)")

        result = asyncio.run(main())

        self.assertIsInstance(result, v8.Value)
        self.assertEqual(result.as_int32(), 42)

    def test_typed_promise_can_be_awaited(self):
        context = self.make_context()
        context.set_microtasks_policy("explicit")
        promise = context.eval("Promise.resolve('done')").as_v8_promise()
        assert promise is not None

        async def main():
            return await promise

        result = asyncio.run(main())

        self.assertIsInstance(result, v8.Value)
        self.assertEqual(str(result), "done")

    def test_rejected_promise_await_raises_runtime_error(self):
        context = self.make_context()
        context.set_microtasks_policy("explicit")

        async def main():
            return await context.eval("Promise.reject(new Error('boom'))")

        with self.assertRaises(RuntimeError) as raised:
            asyncio.run(main())

        self.assertIn("boom", str(raised.exception))

    def test_non_promise_value_cannot_be_awaited(self):
        context = self.make_context()

        async def main():
            return await context.eval("42")

        with self.assertRaises(TypeError):
            asyncio.run(main())

    def test_promise_rejection_tracker_warns_for_unhandled_rejection(self):
        context = self.make_promise_tracker_context()

        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always", RuntimeWarning)
            context.eval("Promise.reject('boom')")
            context.perform_microtask_checkpoint()

        self.assertEqual(len(caught), 1)
        self.assertTrue(issubclass(caught[0].category, RuntimeWarning))
        self.assertIn(
            "Unhandled JavaScript Promise rejection: boom", str(caught[0].message)
        )

    def test_promise_rejection_tracker_can_ignore_warnings(self):
        context = self.make_promise_tracker_context(policy="ignore")

        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always", RuntimeWarning)
            context.eval("Promise.reject('boom')")
            context.perform_microtask_checkpoint()

        self.assertEqual(caught, [])

    def test_promise_rejection_tracker_callback_receives_events(self):
        events: list[tuple[str, str | None]] = []

        def on_rejection(event: str, reason: str | None) -> None:
            events.append((event, reason))

        context = self.make_promise_tracker_context(
            policy="ignore", callback=on_rejection
        )
        context.eval(
            """
            const rejected = Promise.reject('boom');
            rejected.catch(() => {});
            """
        )
        context.perform_microtask_checkpoint()

        self.assertIn(("reject_with_no_handler", "boom"), events)
        self.assertIn(("handler_added_after_reject", None), events)
