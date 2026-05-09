from _shared import make_context

context = make_context()
value = context.eval(
    """
    (() => {
      const object = {
        items: [1, { label: "two" }],
        map: new Map([["answer", 42]]),
        typed: new Uint8Array([5, 6, 7]),
      };
      object.self = object;
      return object;
    })()
    """
)

encoded = context.serialize(value)
clone = context.deserialize(memoryview(encoded))
context.set_global("clone", clone)

print(len(encoded))
print(context.eval("clone.items[1].label"))
print(context.eval("clone.map.get('answer')"))
print(context.eval("clone.self === clone"))
print(context.eval("clone.typed").to_python())
