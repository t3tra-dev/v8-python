import asyncio

import v8


def create_timer_context():
    profile = v8.BaseProfile().install([v8.api.Timer()])

    isolate = v8.Isolate()
    builder = isolate.create_context_builder()
    builder.use_profile(profile)
    return builder.build()


context = create_timer_context()

context.eval(
    """
    globalThis.events = [];

    setTimeout(() => {
      events.push("timeout");
    }, 0);

    let ticks = 0;
    const intervalId = setInterval(() => {
      ticks += 1;
      events.push(`interval:${ticks}`);

      if (ticks === 2) {
        clearInterval(intervalId);
      }
    }, 0);
    """
)

task_count = context.run_until_idle(max_tasks=10)

print(f"tasks: {task_count}")
print(context.eval("events.join(', ')"))


async def main():
    return await context.eval(
        """
        new Promise((resolve) => {
          setTimeout(() => resolve("promise resolved"), 1);
        })
        """
    )


print(asyncio.run(main()))
