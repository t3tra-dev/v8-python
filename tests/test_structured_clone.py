from typing import Any, cast

from tests.support import V8TestCase


class StructuredCloneTests(V8TestCase):
    def test_serialize_deserialize_javascript_graph(self):
        context = self.make_context()
        value = context.eval(
            """
            (() => {
              const object = {
                items: [1, { label: "two" }],
                map: new Map([["answer", 42]]),
                date: new Date(1234),
                typed: new Uint8Array([5, 6, 7]),
              };
              object.self = object;
              return object;
            })()
            """
        )

        encoded = context.serialize(value)
        self.assertIsInstance(encoded, bytes)

        clone = context.deserialize(encoded)
        context.set_global("clone", clone)
        self.assertEqual(context.eval("clone.items[1].label").to_string(), "two")
        self.assertEqual(context.eval("clone.map.get('answer')").as_int32(), 42)
        self.assertEqual(context.eval("clone.date.getTime()").as_int32(), 1234)
        self.assertTrue(context.eval("clone.self === clone").as_boolean())
        self.assertTrue(context.eval("clone.typed instanceof Uint8Array").as_boolean())
        self.assertEqual(context.eval("clone.typed").to_python(), b"\x05\x06\x07")
        self.assertFalse(context.eval("clone === clone.self.self.items").as_boolean())

    def test_serialize_accepts_python_values(self):
        context = self.make_context()

        encoded = context.serialize(
            {
                "items": [1, True, None],
                "payload": b"\x01\x02",
            }
        )
        clone = context.deserialize(memoryview(encoded))

        self.assertEqual(
            clone.to_python(),
            {"items": [1, True, None], "payload": b"\x01\x02"},
        )

    def test_deserialize_accepts_bytearray(self):
        context = self.make_context()
        encoded = context.serialize([1, 2, 3])

        clone = context.deserialize(bytearray(encoded))

        self.assertEqual(clone.to_python(), [1, 2, 3])

    def test_uncloneable_values_raise(self):
        context = self.make_context()

        with self.assertRaises(RuntimeError):
            context.serialize(context.eval("(function nope() {})"))

        with self.assertRaises(TypeError):
            context.deserialize(cast(Any, 1))

        with self.assertRaises((RuntimeError, ValueError)):
            context.deserialize(b"not a structured clone")
