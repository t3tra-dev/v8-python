import logging

from tests.support import V8TestCase


class ConsoleAndPolicyTests(V8TestCase):
    def test_console_is_profile_installed_host_api(self):
        bare_context = self.make_context()
        self.assertEqual(str(bare_context.eval("typeof console.warning")), "undefined")

        context = self.make_console_context()
        self.assertEqual(str(context.eval("typeof console.log")), "function")
        self.assertEqual(str(context.eval("typeof console.warning")), "function")

    def test_console_logs_to_python_logger(self):
        logger, handler = self.make_recording_logger("v8-python-test-console")
        context = self.make_console_context(logger)
        context.eval(
            """
            console.log('hello', 42);
            console.info('ready');
            console.debug('details');
            console.warn('careful');
            console.warning('alias');
            console.error('boom');
            """
        )

        self.assertEqual(
            [(record.levelno, record.getMessage()) for record in handler.records],
            [
                (logging.INFO, "hello 42"),
                (logging.INFO, "ready"),
                (logging.DEBUG, "details"),
                (logging.WARNING, "careful"),
                (logging.WARNING, "alias"),
                (logging.ERROR, "boom"),
            ],
        )

    def test_console_stateful_helpers_use_python_logger(self):
        logger, handler = self.make_recording_logger("v8-python-test-console-state")
        context = self.make_console_context(logger)
        context.eval(
            """
            console.assert(true, 'skipped');
            console.assert(false, 'bad', 42);
            console.count('items');
            console.count('items');
            console.countReset('items');
            console.countReset('missing');
            console.time('load');
            console.timeLog('load', 'halfway');
            console.timeEnd('load');
            console.group('outer');
            console.log('inner');
            console.groupEnd();
            """
        )
        messages = [record.getMessage() for record in handler.records]

        self.assertIn("Assertion failed: bad 42", messages)
        self.assertIn("items: 1", messages)
        self.assertIn("items: 2", messages)
        self.assertIn("Count for 'missing' does not exist.", messages)
        self.assertTrue(
            any(
                message.startswith("load: ") and message.endswith(" halfway")
                for message in messages
            )
        )
        self.assertTrue(
            any(
                message.startswith("load: ") and message.endswith("ms")
                for message in messages
            )
        )
        self.assertIn("outer", messages)
        self.assertIn("  inner", messages)

    def test_dynamic_code_policy_disallows_eval_and_function_constructor(self):
        bare_context = self.make_context()
        self.assertEqual(bare_context.eval("eval('20 + 22')").as_int32(), 42)
        self.assertEqual(
            bare_context.eval(
                "Function('left', 'right', 'return left + right')(20, 22)"
            ).as_int32(),
            42,
        )

        context = self.make_dynamic_code_context()
        self.assertEqual(context.eval("20 + 22").as_int32(), 42)

        with self.assertRaises(RuntimeError) as eval_error:
            context.eval("eval('20 + 22')")
        self.assertIn(
            "Code generation from strings disallowed", str(eval_error.exception)
        )

        with self.assertRaises(RuntimeError) as function_error:
            context.eval("Function('return 42')()")
        self.assertIn(
            "Code generation from strings disallowed",
            str(function_error.exception),
        )

    def test_dynamic_code_policy_can_explicitly_allow_eval(self):
        context = self.make_dynamic_code_context(allow_eval=True)

        self.assertEqual(context.eval("eval('20 + 22')").as_int32(), 42)
        self.assertEqual(context.eval("Function('return 42')()").as_int32(), 42)
