import asyncio

import v8
from _shared import make_profile_context


def import_meta(specifier: str) -> dict[str, object]:
    return {"url": f"v8:{specifier}"}


context = make_profile_context(
    [
        v8.api.ModuleLoader(
            {"./answer.js": "export const answer = 42;"},
            import_meta=import_meta,
        )
    ]
)

module = context.compile_module(
    """
    import { answer } from './answer.js';
    export const result = answer;
    export const url = import.meta.url;
    """,
    specifier="./main.js",
)
module.instantiate()
module.evaluate()

namespace = module.namespace()
print(namespace["result"])
print(namespace["url"])


async def main():
    return await context.eval("import('./answer.js').then((module) => module.answer)")


print(asyncio.run(main()))
