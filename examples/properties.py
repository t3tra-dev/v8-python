import v8
from _shared import make_context

context = make_context()
obj = context.eval("({})")
context.set_global("obj", obj)

attributes = v8.PropertyAttribute(
    read_only=True,
    dont_enum=True,
    dont_delete=True,
)
obj.define_own_property("answer", 42, attributes)

descriptor = obj.get_own_property_descriptor("answer")
assert descriptor is not None

print(descriptor.value)
print(descriptor.writable, descriptor.enumerable, descriptor.configurable)
print(context.eval("Object.keys(obj).length"))
print(context.eval("delete obj.answer"))

obj.freeze()
print(context.eval("Object.isFrozen(obj)"))
