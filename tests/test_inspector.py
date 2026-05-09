import json
from typing import Any, cast

import v8

from tests.support import V8TestCase


class InspectorTests(V8TestCase):
    def test_context_requires_inspector_host_api(self) -> None:
        context = self.make_context()

        with self.assertRaises(RuntimeError):
            context.inspector()

    def test_inspector_session_dispatches_runtime_evaluate(self) -> None:
        context = self.make_inspector_context("debug-target")
        inspector = context.inspector()
        session = inspector.connect()

        self.assertTrue(inspector.is_alive())
        self.assertEqual(inspector.name, "debug-target")
        self.assertEqual(inspector.context_group_id, 1)
        self.assertTrue(v8.InspectorSession.can_dispatch_method("Runtime.evaluate"))

        session.send({"id": 1, "method": "Runtime.enable"})
        context_id = self._execution_context_id(session.take_messages())
        session.send(
            {
                "id": 2,
                "method": "Runtime.evaluate",
                "params": {
                    "expression": "40 + 2",
                    "contextId": context_id,
                    "returnByValue": True,
                },
            }
        )

        response = self._response_for_id(session.take_messages(), 2)
        result = cast(dict[str, Any], response["result"])
        value = cast(dict[str, Any], result["result"])
        self.assertEqual(value["value"], 42)

    def test_inspector_session_queues_notifications_and_callback_messages(self) -> None:
        context = self.make_inspector_context()
        inspector = context.inspector()
        seen: list[str] = []
        session = inspector.connect(on_message=seen.append)

        session.send({"id": 1, "method": "Runtime.enable"})
        context.eval("console.log('from inspector')")

        messages = session.take_messages()
        self.assertGreaterEqual(len(messages), 1)
        self.assertEqual(messages, seen)
        self.assertTrue(
            any(
                json.loads(message).get("method") == "Runtime.consoleAPICalled"
                for message in messages
            )
        )
        self.assertEqual(len(session), 0)

    def _response_for_id(self, messages: list[str], response_id: int) -> dict[str, Any]:
        for message in messages:
            decoded = json.loads(message)
            if decoded.get("id") == response_id:
                return cast(dict[str, Any], decoded)

        raise AssertionError(f"response id {response_id} not found in {messages!r}")

    def _execution_context_id(self, messages: list[str]) -> int:
        for message in messages:
            decoded = json.loads(message)
            if decoded.get("method") != "Runtime.executionContextCreated":
                continue

            params = cast(dict[str, Any], decoded["params"])
            context = cast(dict[str, Any], params["context"])
            return cast(int, context["id"])

        raise AssertionError(
            f"execution context notification not found in {messages!r}"
        )
