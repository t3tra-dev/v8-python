from typing import cast

import v8

from tests.support import V8TestCase


class HeapDiagnosticsTests(V8TestCase):
    def test_isolate_heap_statistics_before_context_creation(self):
        isolate = v8.Isolate()

        statistics = isolate.heap_statistics()
        self.assertGreater(cast(int, statistics["heap_size_limit"]), 0)
        self.assertGreaterEqual(cast(int, statistics["used_heap_size"]), 0)
        self.assertIsInstance(statistics["does_zap_garbage"], bool)

        spaces = isolate.heap_space_statistics()
        self.assertGreater(len(spaces), 0)
        self.assertIsInstance(spaces[0]["space_name"], str)
        self.assertGreaterEqual(cast(int, spaces[0]["space_size"]), 0)

        code_statistics = isolate.heap_code_statistics()
        self.assertGreaterEqual(
            code_statistics["code_and_metadata_size"],
            0,
        )

    def test_context_heap_statistics_after_context_creation(self):
        context = self.make_context()
        context.eval("Array.from({ length: 256 }, (_, index) => ({ index }))")

        statistics = context.heap_statistics()
        self.assertGreater(cast(int, statistics["heap_size_limit"]), 0)
        self.assertGreaterEqual(cast(int, statistics["used_heap_size"]), 0)

        spaces = context.heap_space_statistics()
        self.assertGreater(len(spaces), 0)
        self.assertTrue(
            all(isinstance(space["space_name"], str) for space in spaces),
        )

        code_statistics = context.heap_code_statistics()
        self.assertIn("bytecode_and_metadata_size", code_statistics)

    def test_memory_pressure_and_low_memory_notifications(self):
        context = self.make_context()

        context.memory_pressure("none")
        context.memory_pressure("moderate")
        context.memory_pressure("critical")
        context.low_memory_notification()

    def test_request_garbage_collection_for_testing(self):
        context = self.make_context()
        context.eval("Array.from({ length: 512 }, (_, index) => ({ index }))")

        context.request_garbage_collection_for_testing("minor")
        context.request_garbage_collection_for_testing("full")

    def test_invalid_diagnostic_arguments_raise_value_error(self):
        context = self.make_context()

        with self.assertRaises(ValueError):
            context.memory_pressure("heavy")  # pyright: ignore[reportArgumentType]

        with self.assertRaises(ValueError):
            context.request_garbage_collection_for_testing(
                "major",  # pyright: ignore[reportArgumentType]
            )
