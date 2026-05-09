from _shared import make_context

context = make_context()
buffer = context.new_array_buffer(b"\x01\x02\x03\x04")

typed = buffer.typed_array("Uint8Array", byte_offset=1, length=2)
view = buffer.data_view(byte_offset=2, byte_length=2)

print(buffer.byte_length, bytes(buffer))
print(typed.type_name, bytes(typed))
print(bytes(view))

context.set_global("buffer", buffer)
context.eval("new Uint8Array(buffer)[3] = 9")
print(bytes(buffer))
