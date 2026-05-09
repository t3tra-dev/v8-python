import v8

creator = v8.SnapshotCreator()
creator.eval("globalThis.fromSnapshot = 40")
index = creator.add_context("globalThis.extra = 2")
snapshot = creator.create_blob()

print(snapshot.byte_length, snapshot.is_valid())

default_context = v8.Isolate(snapshot).create_context()
print(default_context.eval("fromSnapshot"))
del default_context

extra_context = v8.Isolate(snapshot).create_context_from_snapshot(index)
print(extra_context.eval("extra"))
