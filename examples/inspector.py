import json
from typing import Any, cast

import v8
from _shared import make_profile_context


def execution_context_id(messages: list[str]) -> int:
    for message in messages:
        decoded = json.loads(message)
        if decoded.get("method") == "Runtime.executionContextCreated":
            params = cast(dict[str, Any], decoded["params"])
            context = cast(dict[str, Any], params["context"])
            return cast(int, context["id"])
    raise RuntimeError("execution context notification not found")


def response_for_id(messages: list[str], response_id: int) -> dict[str, Any]:
    for message in messages:
        decoded = json.loads(message)
        if decoded.get("id") == response_id:
            return cast(dict[str, Any], decoded)
    raise RuntimeError(f"response {response_id} not found")


context = make_profile_context([v8.api.Inspector("example")])
inspector = context.inspector()
session = inspector.connect()

session.send({"id": 1, "method": "Runtime.enable"})
context_id = execution_context_id(session.take_messages())

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

response = response_for_id(session.take_messages(), 2)
result = cast(dict[str, Any], response["result"])
value = cast(dict[str, Any], result["result"])
print(inspector.name, value["value"])
