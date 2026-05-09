import v8
from _shared import make_profile_context

context = make_profile_context([v8.api.DynamicCodePolicy()])

print(context.eval("20 + 22"))

try:
    context.eval("eval('20 + 22')")
except RuntimeError as error:
    print(type(error).__name__)
    print(str(error).splitlines()[0])
