import v8
from _shared import make_context

context = make_context()
private = context.new_private("slot")
external = context.new_external({"answer": 42})
obj = context.new_object(internal_field_count=2)

obj.set_private(private, "hidden")
obj.set_internal_field(0, external)
obj.set_internal_field(1, private)

print(private.name)
print(obj.get_private(private))

field0 = obj.get_internal_field(0)
assert isinstance(field0, v8.Value)
field0_external = field0.as_v8_external()
assert field0_external is not None
print(field0_external.payload())

print(obj.get_internal_field(1))
