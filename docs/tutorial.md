# Tutorial

## Run JavaScript

```python
import v8

isolate = v8.Isolate()
builder = isolate.create_context_builder()
context = builder.build()

result = context.eval("'Hello' + ' from V8'")
print(result)
```

## Expose a Python function

```python
import v8

isolate = v8.Isolate()
builder = isolate.create_context_builder()


@builder.host_function(name="add")
def add(left: int, right: int) -> int:
    return left + right


context = builder.build()
print(context.eval("add(20, 22)"))
```

## Install host APIs

Host APIs are installed through a profile. This keeps the context builder small
and makes reusable runtime setups easy to share.

```python
import v8

profile = v8.BaseProfile().install([v8.api.Timer()])

isolate = v8.Isolate()
builder = isolate.create_context_builder()
builder.use_profile(profile)
context = builder.build()

context.eval(
    """
    globalThis.events = [];
    setTimeout(() => events.push("ready"), 0);
    """
)

context.run_until_idle(max_tasks=10)
print(context.eval("events.join(', ')"))
```

## Await a JavaScript Promise

JavaScript promises can be awaited from Python.

```python
import asyncio
import v8

isolate = v8.Isolate()
builder = isolate.create_context_builder()
context = builder.build()


async def main():
    return await context.eval("Promise.resolve('done')")


print(asyncio.run(main()))
```
