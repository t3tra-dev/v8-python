import v8
from _shared import make_context

context = make_context()

try:
    context.eval(
        """
        function inner() {
          throw new TypeError("boom");
        }
        function outer() {
          inner();
        }
        outer();
        //# sourceURL=app.js
        """
    )
except v8.JavaScriptError as error:
    print(error.message)
    print(error.script_resource_name)
    if error.stack_trace is not None:
        frame = error.stack_trace[0]
        print(frame.function_name, frame.line)
