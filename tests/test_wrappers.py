from datetime import datetime, timezone

import v8

from tests.support import V8TestCase


class WrapperTests(V8TestCase):
    def test_map_wrapper_uses_python_mapping_protocols(self):
        context = self.make_context()
        value = context.eval("new Map([['answer', 41]])")

        self.assertTrue(value.is_map())
        self.assertEqual(value.kind, "map")
        self.assertEqual(len(value), 1)

        mapping = value.as_v8_map()
        assert mapping is not None
        self.assertIsInstance(mapping, v8.Map)
        self.assertEqual(mapping.size, 1)
        self.assertIn("answer", mapping)
        self.assertEqual(mapping["answer"].as_int32(), 41)

        mapping["answer"] = 42
        self.assertTrue(mapping.set("other", 2))
        self.assertEqual(mapping["answer"].as_int32(), 42)
        self.assertEqual(
            [(str(key), item.as_int32()) for key, item in mapping.items()],
            [("answer", 42), ("other", 2)],
        )
        self.assertEqual([str(key) for key in mapping], ["answer", "other"])
        self.assertEqual(value.to_python(), [("answer", 42), ("other", 2)])

        context.set_global("typedMap", mapping)
        self.assertEqual(context.eval("typedMap.get('answer')").as_int32(), 42)
        self.assertTrue(mapping.delete("other"))
        self.assertNotIn("other", mapping)

    def test_set_wrapper_uses_python_set_protocols(self):
        context = self.make_context()
        value = context.eval("new Set(['a', 'b'])")

        self.assertTrue(value.is_set())
        self.assertEqual(value.kind, "set")
        self.assertEqual(len(value), 2)

        values = value.as_v8_set()
        assert values is not None
        self.assertIsInstance(values, v8.Set)
        self.assertEqual(values.size, 2)
        self.assertIn("a", values)
        self.assertEqual([str(item) for item in values], ["a", "b"])

        self.assertTrue(values.add("c"))
        self.assertTrue(values.has("c"))
        self.assertEqual([str(item) for item in values.values()], ["a", "b", "c"])
        self.assertEqual(value.to_python(), ["a", "b", "c"])

        context.set_global("typedSet", values)
        self.assertTrue(context.eval("typedSet.has('c')").as_boolean())
        self.assertTrue(values.delete("b"))
        self.assertNotIn("b", values)

    def test_date_wrapper_converts_to_datetime(self):
        context = self.make_context()
        value = context.eval("new Date(1234)")

        self.assertTrue(value.is_date())
        self.assertEqual(value.kind, "date")

        date = value.as_v8_date()
        assert date is not None
        self.assertIsInstance(date, v8.Date)
        self.assertEqual(date.timestamp_ms, 1234.0)
        self.assertEqual(date.value_of(), 1234.0)
        self.assertEqual(date.timestamp(), 1.234)
        self.assertEqual(
            date.to_datetime(),
            datetime.fromtimestamp(1.234, tz=timezone.utc),
        )
        self.assertEqual(
            context.eval("new Date(0)").to_python(),
            datetime.fromtimestamp(0, tz=timezone.utc),
        )

    def test_regexp_wrapper_exposes_source_flags_and_exec(self):
        context = self.make_context()
        value = context.eval("/a+/i")

        self.assertTrue(value.is_regexp())
        self.assertEqual(value.kind, "regexp")

        regexp = value.as_v8_regexp()
        assert regexp is not None
        self.assertIsInstance(regexp, v8.RegExp)
        self.assertEqual(regexp.source, "a+")
        self.assertEqual(regexp.flags, "i")
        self.assertTrue(regexp.test("xxAA"))

        match = regexp.exec("xxAA")
        assert match is not None
        self.assertEqual(str(match[0]), "AA")
        self.assertIsNone(regexp.exec("bbb"))
        self.assertEqual(str(regexp), "/a+/i")

    def test_proxy_wrapper_exposes_target_handler_and_revoke(self):
        context = self.make_context()
        value = context.eval("new Proxy({ answer: 42 }, {})")

        self.assertTrue(value.is_proxy())
        self.assertEqual(value.kind, "proxy")

        proxy = value.as_v8_proxy()
        assert proxy is not None
        self.assertIsInstance(proxy, v8.Proxy)
        self.assertFalse(proxy.is_revoked())
        self.assertEqual(proxy.target()["answer"].as_int32(), 42)
        self.assertEqual(proxy.handler().to_python(), {})

        context.set_global("typedProxy", proxy)
        self.assertEqual(context.eval("typedProxy.answer").as_int32(), 42)
        with self.assertRaises(RuntimeError):
            proxy.revoke()
