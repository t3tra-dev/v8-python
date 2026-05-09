import v8

from tests.support import V8TestCase


class ScriptMetadataTests(V8TestCase):
    def test_compile_exposes_script_origin_metadata(self):
        context = self.make_context()
        script = context.compile(
            "function answer() { return 42; }\nanswer();",
            filename="answer.js",
            source_map_url="answer.js.map",
        )

        self.assertGreater(script.script_id, 0)
        self.assertEqual(script.resource_name, "answer.js")
        self.assertEqual(script.source_map_url, "answer.js.map")
        self.assertEqual(script.source_mapping_url, "answer.js.map")
        self.assertIsNone(script.source_url)
        self.assertFalse(script.cached_data_rejected)
        self.assertEqual(script.run().as_int32(), 42)

    def test_source_url_magic_comment_is_preserved(self):
        context = self.make_context()
        script = context.compile(
            "1 + 1\n//# sourceURL=inline.js", filename="fallback.js"
        )

        self.assertEqual(script.resource_name, "fallback.js")
        self.assertEqual(script.source_url, "inline.js")
        self.assertEqual(script.run().as_int32(), 2)

    def test_script_code_cache_round_trips(self):
        context = self.make_context()
        source = "function add(left, right) { return left + right; }\nadd(19, 23);"
        script = context.compile(source, filename="cache.js")

        cached_data = script.create_code_cache()
        self.assertIsInstance(cached_data, bytes)
        self.assertGreater(len(cached_data), 0)
        self.assertGreater(v8.cached_data_version_tag(), 0)

        cached_script = context.compile(
            source,
            filename="cache.js",
            cached_data=memoryview(cached_data),
        )
        self.assertFalse(cached_script.cached_data_rejected)
        self.assertEqual(cached_script.run().as_int32(), 42)

    def test_compile_accepts_v8_string_source(self):
        context = self.make_context()
        source = context.new_string("21 * 2")
        script = context.compile(source, filename="string-source.js")

        self.assertEqual(script.source, "21 * 2")
        self.assertEqual(script.resource_name, "string-source.js")
        self.assertEqual(script.run().as_int32(), 42)

    def test_compiled_script_uses_filename_in_javascript_error(self):
        context = self.make_context()
        script = context.compile(
            "function fail() {\n  throw new Error('from script');\n}\nfail();",
            filename="failure.js",
        )

        with self.assertRaises(v8.JavaScriptError) as raised:
            script.run()

        self.assertEqual(raised.exception.script_resource_name, "failure.js")
        self.assertIn("failure.js", str(raised.exception))


class FunctionCodeCacheTests(V8TestCase):
    def test_compile_function_exposes_metadata_and_code_cache(self):
        context = self.make_context()
        function = context.compile_function(
            "return left + right;",
            ["left", "right"],
            filename="adder.js",
        )

        self.assertGreater(function.script_id, 0)
        self.assertEqual(function.resource_name, "adder.js")
        self.assertEqual(function.script_line_number, 0)
        self.assertEqual(function.script_column_number, 0)
        self.assertFalse(function.cached_data_rejected)
        self.assertEqual(function(19, 23).as_int32(), 42)

        cached_data = function.create_code_cache()
        self.assertIsInstance(cached_data, bytes)
        self.assertGreater(len(cached_data), 0)

        cached_function = context.compile_function(
            "return left + right;",
            ["left", "right"],
            filename="adder.js",
            cached_data=bytearray(cached_data),
        )
        self.assertFalse(cached_function.cached_data_rejected)
        self.assertEqual(cached_function(20, 22).as_int32(), 42)

    def test_normal_function_code_cache_is_rejected_before_v8_fatal_error(self):
        context = self.make_context()
        function = context.eval("(function normal() { return 42; })").as_v8_function()
        assert function is not None

        with self.assertRaises(RuntimeError):
            function.create_code_cache()
