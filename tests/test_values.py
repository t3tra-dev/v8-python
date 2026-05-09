import warnings

import v8

from tests.support import V8TestCase


class ValueTests(V8TestCase):
    def test_eval_reuses_context(self):
        context = self.make_context()

        self.assertEqual(context.eval("1 + 2").as_int32(), 3)
        self.assertEqual(context.eval("let x = 4; x * 2").as_int32(), 8)

    def test_python_values_globals_and_properties(self):
        context = self.make_context()

        self.assertTrue(
            context.set_global(
                "payload",
                {"items": [1, True, None], "label": "from-python"},
            )
        )

        payload = context.get_global("payload")
        self.assertEqual(str(payload["label"]), "from-python")
        self.assertEqual(len(payload["items"]), 3)
        self.assertTrue(payload["items"][1].as_boolean())

        payload["count"] = 9
        self.assertEqual(
            context.eval("payload.count + payload.items[0]").as_int32(), 10
        )
        self.assertIn("count", payload.keys())
        self.assertIn("count", payload)
        del payload["count"]
        self.assertNotIn("count", payload)

    def test_function_call_accepts_args_and_this(self):
        context = self.make_context()
        fn = context.eval("(function(a, b) { return this.base + a + b; })")
        this_arg = context.from_python({"base": 10})

        self.assertEqual(fn.call([2, 3], this_arg).as_int32(), 15)
        self.assertEqual(
            context.eval("(function(a, b) { return a + b; })")(20, 22).as_int32(), 42
        )

    def test_json_helpers(self):
        context = self.make_context()
        value = context.parse_json('{"answer":42,"items":[1,2]}')

        self.assertEqual(value["answer"].as_int32(), 42)
        self.assertEqual(value["items"][0].as_int32(), 1)
        self.assertEqual(value.to_json(), '{"answer":42,"items":[1,2]}')

    def test_value_conversion_type_and_comparison_helpers(self):
        context = self.make_context()

        value = context.eval("({items:[1, 'two'], nested:{ok:true}})")
        self.assertEqual(len(value), 2)
        self.assertEqual(
            value.to_python(),
            {"items": [1, "two"], "nested": {"ok": True}},
        )
        self.assertTrue(bool(context.eval("[]")))
        self.assertFalse(bool(context.eval("0")))
        self.assertEqual(float(context.eval("1.5")), 1.5)
        self.assertEqual(context.eval("42"), 42)
        self.assertNotEqual(context.eval("42"), 43)

        symbol = context.eval("Symbol('tag')")
        self.assertTrue(symbol.is_symbol())
        self.assertEqual(symbol.typeof(), "symbol")
        self.assertEqual(symbol.to_string(), "Symbol(tag)")
        self.assertEqual(str(symbol), "Symbol(tag)")

        bigint = context.eval("9007199254740993n")
        self.assertTrue(bigint.is_big_int())
        self.assertEqual(bigint.as_big_int(), 9007199254740993)
        self.assertEqual(int(bigint), 9007199254740993)
        self.assertEqual(bigint.as_big_int_i64(), 9007199254740993)
        self.assertEqual(bigint.as_big_int_string(), "9007199254740993")
        self.assertIsNone(context.eval("9223372036854775808n").as_big_int_i64())

        context.set_global("shared", {})
        self.assertTrue(
            context.get_global("shared").strict_equals(context.get_global("shared"))
        )
        self.assertTrue(context.eval("NaN").same_value(context.eval("NaN")))
        self.assertTrue(context.eval("[]").instance_of(context.eval("Array")))

        function = context.eval("(function named() { return 1; })")
        function_object = function.as_function()
        assert function_object is not None
        self.assertEqual(function_object["name"], "named")

    def test_value_operators_use_javascript_semantics(self):
        context = self.make_context()

        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            self.assertEqual((context.eval("20") + 22).as_int32(), 42)
        self.assertEqual(caught, [])

        with self.assertWarns(RuntimeWarning):
            concatenated = context.eval("'answer:'") + 42
        self.assertEqual(str(concatenated), "answer:42")

        with self.assertWarns(RuntimeWarning):
            coerced_difference = context.eval("'40'") - 2
        self.assertEqual(coerced_difference.as_int32(), 38)

        self.assertEqual((context.eval("21") / 2).as_number(), 10.5)
        self.assertEqual((context.eval("5") % 2).as_int32(), 1)
        self.assertEqual((2 ** context.eval("3")).as_int32(), 8)
        self.assertEqual((context.eval("1") << 3).as_int32(), 8)
        self.assertEqual((context.eval("8") >> 1).as_int32(), 4)
        self.assertEqual((context.eval("6") & 3).as_int32(), 2)
        self.assertEqual((context.eval("6") | 1).as_int32(), 7)
        self.assertEqual((context.eval("6") ^ 3).as_int32(), 5)
        self.assertEqual((-context.eval("5")).as_int32(), -5)
        self.assertEqual((~context.eval("5")).as_int32(), -6)

        with self.assertWarns(RuntimeWarning):
            self.assertTrue(context.eval("42") == "42")
        with self.assertWarns(RuntimeWarning):
            self.assertTrue(context.eval("'2'") < 10)

    def test_typed_value_wrappers_round_trip_to_value_handles(self):
        context = self.make_context()

        string = context.eval("'hello'").as_v8_string()
        assert string is not None
        self.assertIsInstance(string, v8.String)
        self.assertEqual(string.value, "hello")
        self.assertEqual(str(string.to_value()), "hello")
        context.set_global("typedString", string)
        self.assertEqual(str(context.eval("typedString + '!'")), "hello!")

        obj = context.eval("({answer: 41})").as_v8_object()
        assert obj is not None
        self.assertIsInstance(obj, v8.Object)
        obj["answer"] = 42
        self.assertEqual(obj["answer"].as_int32(), 42)
        self.assertIn("answer", obj)
        self.assertEqual(len(obj), 1)
        context.set_global("typedObject", obj)
        self.assertEqual(context.eval("typedObject.answer").as_int32(), 42)

        array = context.eval("[1, 2]").as_v8_array()
        assert array is not None
        self.assertIsInstance(array, v8.Array)
        self.assertEqual(len(array), 2)
        array[1] = 41
        self.assertEqual(array[1].as_int32(), 41)
        context.set_global("typedArray", array)
        self.assertEqual(context.eval("typedArray[0] + typedArray[1]").as_int32(), 42)

        function = context.eval(
            "(function addOne(value) { return value + 1; })"
        ).as_v8_function()
        assert function is not None
        self.assertIsInstance(function, v8.Function)
        self.assertEqual(function.name, "addOne")
        self.assertEqual(function(41).as_int32(), 42)

        context.set_microtasks_policy("explicit")
        promise = context.eval(
            "Promise.resolve(41).then((value) => value + 1)"
        ).as_v8_promise()
        assert promise is not None
        self.assertIsInstance(promise, v8.Promise)
        self.assertEqual(promise.state(), "pending")
        context.perform_microtask_checkpoint()
        promise_result = promise.result()
        assert promise_result is not None
        self.assertEqual(promise_result.as_int32(), 42)

        bigint = context.eval("42n").as_v8_big_int()
        assert bigint is not None
        self.assertIsInstance(bigint, v8.BigInt)
        self.assertEqual(bigint.as_i64(), 42)
        self.assertEqual(str(bigint), "42")
        self.assertEqual(int(bigint), 42)

        symbol = context.eval("Symbol('tag')").as_v8_symbol()
        assert symbol is not None
        self.assertIsInstance(symbol, v8.Symbol)
        self.assertEqual(symbol.description(), "tag")
        self.assertEqual(str(symbol), "Symbol(tag)")

    def test_javascript_exception_contains_message(self):
        context = self.make_context()

        with self.assertRaises(RuntimeError) as raised:
            context.eval("throw new Error('boom')")

        self.assertIn("boom", str(raised.exception))

    def test_execution_timeout(self):
        context = self.make_context()

        with self.assertRaises(TimeoutError):
            context.eval("while (true) {}", timeout_ms=20)

        context.cancel_terminate_execution()
        self.assertEqual(context.eval("1 + 1").as_int32(), 2)
