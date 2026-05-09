import v8

from tests.support import V8TestCase


class JavaScriptErrorTests(V8TestCase):
    def test_javascript_error_exposes_message_and_stack_trace(self):
        context = self.make_context()

        with self.assertRaises(v8.JavaScriptError) as raised:
            context.eval(
                """
                function inner() {
                  throw new TypeError("boom");
                }
                function outer() {
                  inner();
                }
                outer();
                //# sourceURL=app.js
                """
            )

        error = raised.exception
        self.assertIsInstance(error, RuntimeError)
        self.assertIn("TypeError: boom", str(error))
        self.assertEqual(error.message, "TypeError: boom")
        self.assertEqual(error.script_resource_name, "app.js")
        self.assertIsNotNone(error.source_line)

        message = error.message_info
        assert message is not None
        self.assertIsInstance(message, v8.Message)
        self.assertIn("TypeError: boom", message.text)
        self.assertEqual(message.script_resource_name, "app.js")

        stack = error.stack
        assert stack is not None
        self.assertIn("inner", stack)

        stack_trace = error.stack_trace
        assert stack_trace is not None
        self.assertIsInstance(stack_trace, v8.StackTrace)
        self.assertEqual(len(stack_trace), len(error.frames))
        self.assertGreaterEqual(len(stack_trace), 2)

        top_frame = stack_trace[0]
        self.assertIsInstance(top_frame, v8.StackFrame)
        self.assertEqual(top_frame.function_name, "inner")
        self.assertEqual(top_frame.script_name_or_source_url, "app.js")
        self.assertEqual(top_frame.line, 3)
        self.assertFalse(top_frame.is_wasm)
        self.assertFalse(top_frame.is_eval)

        self.assertEqual(stack_trace[-1].line, error.frames[-1].line)
        with self.assertRaises(IndexError):
            stack_trace[len(stack_trace)]

    def test_syntax_error_exposes_v8_message_metadata(self):
        context = self.make_context()

        with self.assertRaises(v8.JavaScriptError) as raised:
            context.eval("function nope( {")

        error = raised.exception
        self.assertIn("SyntaxError", error.message)

        message = error.message_info
        assert message is not None
        self.assertIn("SyntaxError", message.text)
        self.assertEqual(message.line_number, 1)
        self.assertIsNotNone(message.source_line)
        self.assertEqual(message.start_position, error.start_position)
        self.assertEqual(message.end_position, error.end_position)

    def test_javascript_error_can_be_caught_as_runtime_error(self):
        context = self.make_context()

        with self.assertRaises(RuntimeError) as raised:
            context.eval("throw new Error('runtime-compatible')")

        self.assertIsInstance(raised.exception, v8.JavaScriptError)
