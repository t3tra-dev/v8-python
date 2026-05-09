import logging
from typing import Any, cast

import v8

from tests.support import V8TestCase


class ProfileTests(V8TestCase):
    def assert_host_api_class(self, api_class: type[object]) -> None:
        self.assertTrue(issubclass(api_class, v8.api.HostAPI))

    def test_profile_install_accepts_api_marker_instances(self):
        profile = v8.BaseProfile()

        self.assertIsInstance(v8.StartupData(b"snapshot"), v8.StartupData)
        self.assertIsInstance(v8.SnapshotCreator(), v8.SnapshotCreator)
        self.assertEqual(profile.class_count(), 0)
        self.assert_host_api_class(v8.api.Timer)
        self.assertIsInstance(v8.api.Timer(), v8.api.HostAPI)
        self.assert_host_api_class(v8.api.ModuleLoader)
        self.assertIsInstance(v8.api.ModuleLoader({}), v8.api.HostAPI)
        self.assert_host_api_class(v8.api.PromiseRejectionTracker)
        self.assertIsInstance(v8.api.PromiseRejectionTracker(), v8.api.HostAPI)
        self.assert_host_api_class(v8.api.MicrotaskQueue)
        self.assertIsInstance(v8.api.MicrotaskQueue(), v8.api.HostAPI)
        self.assert_host_api_class(v8.api.Console)
        self.assertIsInstance(v8.api.Console(), v8.api.HostAPI)
        self.assertIsInstance(v8.api.Console().logger, logging.Logger)
        self.assert_host_api_class(v8.api.DynamicCodePolicy)
        self.assertIsInstance(v8.api.DynamicCodePolicy(), v8.api.HostAPI)
        self.assertFalse(v8.api.DynamicCodePolicy().allow_eval)
        self.assertTrue(v8.api.DynamicCodePolicy(True).allow_eval)
        self.assert_host_api_class(v8.api.Inspector)
        inspector = v8.api.Inspector("debug")
        self.assertIsInstance(inspector, v8.api.HostAPI)
        self.assertEqual(inspector.name, "debug")
        self.assertEqual(inspector.context_group_id, 1)
        self.assert_host_api_class(v8.api.SharedArrayBuffer)
        self.assertIsInstance(v8.api.SharedArrayBuffer(), v8.api.HostAPI)
        self.assert_host_api_class(v8.api.Atomics)
        self.assertIsInstance(v8.api.Atomics(), v8.api.HostAPI)
        self.assertTrue(v8.api.Atomics().allow_wait)
        self.assertFalse(v8.api.Atomics(False).allow_wait)
        self.assert_host_api_class(v8.api.ShadowRealm)
        self.assertIsInstance(v8.api.ShadowRealm(), v8.api.HostAPI)
        self.assert_host_api_class(v8.api.WebAssembly)
        self.assertIsInstance(v8.api.WebAssembly(), v8.api.HostAPI)
        self.assertTrue(v8.api.WebAssembly().allow_code_generation)
        cache = v8.WasmModuleCache()
        self.assertIs(v8.api.WebAssembly(cache=cache).cache, cache)
        self.assertIs(profile.install([v8.api.Timer()]), profile)

        with self.assertRaises(TypeError):
            v8.api.HostAPI()

        with self.assertRaises(TypeError):
            profile.install([cast(Any, object())])

        with self.assertRaises(TypeError):
            v8.api.Console(cast(Any, object()))

        with self.assertRaises(TypeError):
            v8.api.WebAssembly(cast(Any, object()))
