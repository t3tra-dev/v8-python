import warnings

from _shared import make_context

context = make_context()

print(context.eval("6") * 7)

with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter("always", RuntimeWarning)
    result = context.eval("'20'") + 22

print(result)
print(caught[0].category.__name__)
