import asyncio

from _shared import make_context

context = make_context()
context.set_microtasks_policy("explicit")

promise = context.eval("Promise.resolve(20).then((value) => value + 22)")
print(promise.promise_state())

context.perform_microtask_checkpoint()
print(promise.promise_state())
print(promise.promise_result())


async def main():
    return await context.eval("Promise.resolve('awaited')")


print(asyncio.run(main()))
