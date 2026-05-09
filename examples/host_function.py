import asyncio

import v8

isolate = v8.Isolate()
builder = isolate.create_context_builder()
builder.set_microtasks_policy("explicit")


@builder.host_function(name="add")
def add(left: int, right: int) -> int:
    return left + right


@builder.host_function(name="asyncAdd")
async def async_add(left: int, right: int) -> int:
    await asyncio.sleep(0)
    return left + right


context = builder.build()

print(context.eval("add(20, 22)"))


async def main() -> v8.Value:
    return await context.eval("asyncAdd(20, 22)")


print(asyncio.run(main()))
