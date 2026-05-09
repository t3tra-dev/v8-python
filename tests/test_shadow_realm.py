import subprocess
import sys

import v8

from tests.support import V8TestCase


class ShadowRealmTests(V8TestCase):
    def test_shadow_realm_is_profile_installed_host_api(self):
        script = r"""
import v8

profile = v8.BaseProfile().install([v8.api.ShadowRealm()])
isolate = v8.Isolate()
builder = isolate.create_context_builder()
builder.use_profile(profile)
context = builder.build()

assert str(context.eval("typeof ShadowRealm")) == "function"
assert context.eval(
    '''
    globalThis.answer = 99;
    const realm = new ShadowRealm();
    realm.evaluate("globalThis.answer = 42; globalThis.answer");
    '''
).as_int32() == 42
assert context.eval("globalThis.answer").as_int32() == 99
"""

        result = subprocess.run(
            [sys.executable, "-c", script],
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_shadow_realm_flag_is_only_requested_by_host_api_install(self):
        context = self.make_context()
        self.assertEqual(str(context.eval("typeof ShadowRealm")), "undefined")

        with self.assertRaisesRegex(RuntimeError, "before the first v8.Isolate"):
            v8.BaseProfile().install([v8.api.ShadowRealm()])
