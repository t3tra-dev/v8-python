import v8

isolate = v8.Isolate()
builder = isolate.create_context_builder()


@builder.class_(name="Counter")
class Counter:
    def __init__(self, value: int):
        self.value = value

    def add(self, amount: int) -> int:
        self.value += amount
        return self.value

    @property
    def current(self) -> int:
        return self.value

    @current.setter
    def current(self, value: int) -> None:
        self.value = value


context = builder.build()

print(context.eval("globalThis.counter = new Counter(40); counter.add(2)"))
print(context.eval("counter.current = 10; counter.current"))
