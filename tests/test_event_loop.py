import asyncio
import time

import v8

from tests.support import V8TestCase


class EventLoopTests(V8TestCase):
    def test_timers_are_profile_installed_host_api(self):
        bare_context = self.make_context()
        self.assertEqual(str(bare_context.eval("typeof setTimeout")), "undefined")

        timer_context = self.make_timer_context()
        self.assertEqual(str(timer_context.eval("typeof setTimeout")), "function")

    def test_microtask_queue_is_profile_installed_host_api(self):
        bare_context = self.make_context()
        self.assertEqual(str(bare_context.eval("typeof queueMicrotask")), "undefined")

        context = self.make_microtask_context()
        self.assertEqual(str(context.eval("typeof queueMicrotask")), "function")

    def test_queue_microtask_runs_at_checkpoint(self):
        context = self.make_microtask_context()
        context.set_microtasks_policy("explicit")
        context.eval(
            """
            globalThis.events = [];
            queueMicrotask(() => events.push("microtask"));
            events.push("sync");
            """
        )

        self.assertEqual(context.eval("events.join(',')").__str__(), "sync")
        context.perform_microtask_checkpoint()
        self.assertEqual(
            context.eval("events.join(',')").__str__(),
            "sync,microtask",
        )

    def test_queue_microtask_requires_function(self):
        context = self.make_microtask_context()

        with self.assertRaises(RuntimeError) as raised:
            context.eval("queueMicrotask(42)")

        self.assertIn(
            "queueMicrotask callback must be a function", str(raised.exception)
        )

    def test_queue_microtask_runs_after_timer_task(self):
        profile = v8.BaseProfile().install([v8.api.Timer(), v8.api.MicrotaskQueue()])
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.use_profile(profile)
        context = builder.build()
        context.set_microtasks_policy("explicit")
        context.eval(
            """
            globalThis.done = 0;
            setTimeout(() => {
              queueMicrotask(() => { done = 42; });
            }, 0);
            """
        )

        self.assertTrue(context.run_event_loop_once())
        self.assertEqual(context.eval("done").as_int32(), 42)

    def test_timers_run_as_event_loop_tasks(self):
        context = self.make_timer_context()
        context.eval("globalThis.done = 0; setTimeout(() => { done = 42; }, 0)")

        self.assertEqual(context.eval("done").as_int32(), 0)
        self.assertTrue(context.run_event_loop_once())
        self.assertEqual(context.eval("done").as_int32(), 42)

    def test_timer_task_runs_microtasks_after_callback(self):
        context = self.make_timer_context()
        context.set_microtasks_policy("explicit")
        context.eval(
            """
            globalThis.done = 0;
            setTimeout(() => {
              Promise.resolve().then(() => { done = 42; });
            }, 0);
            """
        )

        self.assertTrue(context.run_event_loop_once())
        self.assertEqual(context.eval("done").as_int32(), 42)

    def test_clear_timeout_prevents_timer_task(self):
        context = self.make_timer_context()
        context.eval(
            """
            globalThis.done = 0;
            const id = setTimeout(() => { done = 42; }, 0);
            clearTimeout(id);
            """
        )

        self.assertFalse(context.run_event_loop_once())
        self.assertEqual(context.eval("done").as_int32(), 0)

    def test_interval_repeats_until_cleared(self):
        context = self.make_timer_context()
        context.eval(
            """
            globalThis.count = 0;
            globalThis.intervalId = setInterval(() => {
              count += 1;
              if (count === 2) clearInterval(intervalId);
            }, 0);
            """
        )

        self.assertEqual(context.run_until_idle(max_tasks=10), 2)
        self.assertEqual(context.eval("count").as_int32(), 2)
        self.assertFalse(context.run_event_loop_once())

    def test_run_event_loop_once_can_wait_for_timer(self):
        context = self.make_timer_context()
        context.eval("globalThis.done = 0; setTimeout(() => { done = 42; }, 10)")

        started = time.monotonic()
        self.assertTrue(context.run_event_loop_once(timeout_ms=100))

        self.assertGreaterEqual(time.monotonic() - started, 0.005)
        self.assertEqual(context.eval("done").as_int32(), 42)

    def test_await_javascript_timer_promise(self):
        context = self.make_timer_context()
        context.set_microtasks_policy("explicit")

        async def main():
            return await context.eval(
                "new Promise((resolve) => setTimeout(() => resolve(42), 1))"
            )

        result = asyncio.run(main())

        self.assertIsInstance(result, v8.Value)
        self.assertEqual(result.as_int32(), 42)
