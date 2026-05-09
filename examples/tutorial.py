import v8

isolate = v8.Isolate()
scope = isolate.create_scope()
source = scope.new_string("'Hello' + ' from V8' + '!'")

print(source.value)

script = scope.compile(source)
result = script.run()

print(result.kind)
print(result)
