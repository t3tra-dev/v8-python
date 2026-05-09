import asyncio

from tests.support import V8TestCase


class ModuleTests(V8TestCase):
    def test_source_text_module_without_imports(self):
        context = self.make_context()
        module = context.compile_module("export const answer = 42;")

        self.assertEqual(module.status, "uninstantiated")
        self.assertTrue(module.instantiate())
        self.assertEqual(module.status, "instantiated")

        module.evaluate()
        self.assertEqual(module.status, "evaluated")
        self.assertEqual(module.namespace()["answer"].as_int32(), 42)

    def test_source_text_module_with_source_imports(self):
        context = self.make_context()
        module = context.compile_module(
            "import { answer } from './answer.js'; export const doubled = answer * 2;",
            specifier="./main.js",
        )

        self.assertTrue(
            module.instantiate({"./answer.js": "export const answer = 21;"})
        )
        module.evaluate()

        self.assertEqual(module.namespace()["doubled"].as_int32(), 42)

    def test_source_text_module_with_precompiled_imports(self):
        context = self.make_context()
        dependency = context.compile_module(
            "export const answer = 40 + 2;",
            specifier="./dependency.js",
        )
        module = context.compile_module(
            "import { answer } from './dependency.js'; export const result = answer;",
            specifier="./main.js",
        )

        self.assertTrue(module.instantiate({"./dependency.js": dependency}))
        module.evaluate()

        self.assertEqual(module.namespace()["result"].as_int32(), 42)

    def test_module_loader_resolves_static_imports(self):
        context = self.make_module_loader_context(
            {"./answer.js": "export const answer = 21;"}
        )
        module = context.compile_module(
            "import { answer } from './answer.js'; export const doubled = answer * 2;",
            specifier="./main.js",
        )

        self.assertTrue(module.instantiate())
        module.evaluate()

        self.assertEqual(module.namespace()["doubled"].as_int32(), 42)

    def test_module_loader_resolver_receives_referrer_and_import_attributes(self):
        calls: list[tuple[str, str | None, dict[str, str]]] = []

        def resolver(
            specifier: str,
            referrer: str | None,
            attributes: dict[str, str],
        ) -> str:
            calls.append((specifier, referrer, dict(attributes)))
            return "export const label = 'json';"

        context = self.make_module_loader_context(resolver)
        module = context.compile_module(
            """
            import { label } from './data.json' with { type: "json" };
            export const result = label;
            """,
            specifier="./main.js",
        )

        self.assertTrue(module.instantiate())
        module.evaluate()

        self.assertEqual(module.namespace()["result"].__str__(), "json")
        self.assertEqual(calls, [("./data.json", "./main.js", {"type": "json"})])

    def test_module_loader_initializes_import_meta(self):
        def import_meta(specifier: str) -> dict[str, object]:
            return {"url": f"v8:{specifier}"}

        context = self.make_module_loader_context(
            {"./dependency.js": "export const url = import.meta.url;"},
            import_meta=import_meta,
        )
        module = context.compile_module(
            """
            import { url } from './dependency.js';
            export const dependencyUrl = url;
            export const mainUrl = import.meta.url;
            """,
            specifier="./main.js",
        )

        self.assertTrue(module.instantiate())
        module.evaluate()

        namespace = module.namespace()
        self.assertEqual(namespace["dependencyUrl"].__str__(), "v8:./dependency.js")
        self.assertEqual(namespace["mainUrl"].__str__(), "v8:./main.js")

    def test_module_loader_dynamic_import_resolves_namespace(self):
        context = self.make_module_loader_context(
            {"./answer.js": "export const answer = 42;"}
        )

        async def main():
            return await context.eval(
                "import('./answer.js').then((module) => module.answer)"
            )

        result = asyncio.run(main())

        self.assertEqual(result.as_int32(), 42)

    def test_missing_module_import_reports_specifier(self):
        context = self.make_context()
        module = context.compile_module("import './missing.js';")

        with self.assertRaises(RuntimeError) as raised:
            module.instantiate({})

        self.assertIn("./missing.js", str(raised.exception))
