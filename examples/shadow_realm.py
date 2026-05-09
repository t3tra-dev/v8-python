import v8

profile = v8.BaseProfile().install([v8.api.ShadowRealm()])
isolate = v8.Isolate()
builder = isolate.create_context_builder()
builder.use_profile(profile)
context = builder.build()

print(context.eval("typeof ShadowRealm"))
print(
    context.eval(
        """
        globalThis.answer = 99;
        const realm = new ShadowRealm();
        realm.evaluate("globalThis.answer = 42; globalThis.answer");
        """
    )
)
print(context.eval("globalThis.answer"))
