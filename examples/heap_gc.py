import v8
from _shared import make_context

isolate = v8.Isolate()
statistics = isolate.heap_statistics()
print(statistics["used_heap_size"], statistics["heap_size_limit"])

context = isolate.create_context()
context.eval("Array.from({ length: 256 }, (_, index) => ({ index }))")
print(context.heap_code_statistics()["bytecode_and_metadata_size"])

context.memory_pressure("moderate")
context.low_memory_notification()
context.request_garbage_collection_for_testing("minor")

temporary = make_context().eval("({ answer: 42 })")
del temporary
print(v8.collect_garbage())
