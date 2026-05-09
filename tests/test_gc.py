# pyright: reportPrivateUsage=false
import gc
import importlib
import unittest

import v8


def make_value(expr: str):
    isolate = v8.Isolate()
    scope = isolate.create_scope()
    source = scope.new_string(expr)
    script = scope.compile(source)

    return script.run()


class GCCallbackTests(unittest.TestCase):
    def setUp(self):
        gc.collect()
        v8.collect_garbage()

    def test_gc_callback_is_registered_once(self):
        self.assertIn(v8._gc_callback, gc.callbacks)
        self.assertEqual(
            sum(callback is v8._gc_callback for callback in gc.callbacks),
            1,
        )

        importlib.reload(v8)

        self.assertEqual(
            sum(callback is v8._gc_callback for callback in gc.callbacks),
            1,
        )

    def test_plain_gc_collect_flushes_ready_isolates(self):
        first = make_value("1")
        second = make_value("2")

        del first
        gc.collect()
        del second
        gc.collect()

        self.assertEqual(v8.collect_garbage(), 0)

    def test_collect_garbage_reports_dropped_isolates(self):
        first = make_value("3")
        second = make_value("4")

        del first
        self.assertEqual(v8.collect_garbage(), 0)
        del second
        self.assertEqual(v8.collect_garbage(), 2)

    def test_gc_callback_handles_python_cycles(self):
        cycle: list[object] = []
        cycle.append(make_value("({answer: 42})"))
        cycle.append(cycle)

        del cycle
        gc.collect()

        self.assertEqual(v8.collect_garbage(), 0)
