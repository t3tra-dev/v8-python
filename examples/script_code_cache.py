import v8
from _shared import make_context

context = make_context()
source = "function add(left, right) { return left + right; }\nadd(19, 23);"

script = context.compile(source, filename="cache.js")
cached_data = script.create_code_cache()
cached_script = context.compile(source, filename="cache.js", cached_data=cached_data)

print(v8.cached_data_version_tag())
print(script.run())
print(cached_script.cached_data_rejected)
print(cached_script.run())

function = context.compile_function(
    "return left + right;",
    ["left", "right"],
    filename="adder.js",
)
print(function(20, 22))
print(len(function.create_code_cache()))
