from _shared import make_context

context = make_context()

mapping = context.eval("new Map([['answer', 42]])").as_v8_map()
values = context.eval("new Set(['a', 'b'])").as_v8_set()
date = context.eval("new Date(1234)").as_v8_date()
regexp = context.eval("/a+/g").as_v8_regexp()
proxy = context.eval("new Proxy({ answer: 42 }, {})").as_v8_proxy()

assert mapping is not None
assert values is not None
assert date is not None
assert regexp is not None
assert proxy is not None

print(mapping["answer"])
print([str(value) for value in values])
print(date.timestamp(), date.to_datetime().isoformat())
print(regexp.test("baa"))
print(proxy.target()["answer"])
