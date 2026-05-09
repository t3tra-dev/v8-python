import v8

from tests.support import V8TestCase


class SnapshotTests(V8TestCase):
    def test_snapshot_creator_creates_startup_data_for_default_context(self) -> None:
        creator = v8.SnapshotCreator()

        self.assertTrue(creator.is_alive())
        creator.eval("globalThis.__snapshotAnswer = 42")
        snapshot = creator.create_blob()

        self.assertIsInstance(snapshot, v8.StartupData)
        self.assertFalse(creator.is_alive())
        self.assertGreater(len(snapshot), 0)
        self.assertEqual(bytes(v8.StartupData(bytes(snapshot))), bytes(snapshot))
        self.assertTrue(snapshot.is_valid())

        isolate = v8.Isolate(snapshot=snapshot)
        context = isolate.create_context()

        self.assertEqual(context.eval("__snapshotAnswer").as_int32(), 42)

    def test_isolate_accepts_raw_snapshot_bytes(self) -> None:
        creator = v8.SnapshotCreator()
        creator.eval("globalThis.__rawSnapshotValue = 'ready'")
        snapshot = creator.create_blob()

        isolate = v8.Isolate(snapshot.to_bytes())
        context = isolate.create_context()

        self.assertEqual(str(context.eval("__rawSnapshotValue")), "ready")

    def test_snapshot_creator_adds_context_snapshots(self) -> None:
        creator = v8.SnapshotCreator()
        creator.eval("globalThis.__snapshotKind = 'default'")
        index = creator.add_context("globalThis.__snapshotKind = 'extra'")
        snapshot = creator.create_blob()

        default_context = v8.Isolate(snapshot).create_context()
        self.assertEqual(str(default_context.eval("__snapshotKind")), "default")

        extra_context = v8.Isolate(snapshot).create_context_from_snapshot(index)
        self.assertEqual(str(extra_context.eval("__snapshotKind")), "extra")

    def test_context_builder_can_use_context_snapshot(self) -> None:
        creator = v8.SnapshotCreator()
        index = creator.add_context("globalThis.fromSnapshot = 40")
        snapshot = creator.create_blob()
        isolate = v8.Isolate(snapshot)
        builder = isolate.create_context_builder()

        builder.use_snapshot(index)
        builder.set_global("fromBuilder", 2)
        context = builder.build()

        self.assertEqual(context.eval("fromSnapshot + fromBuilder").as_int32(), 42)

    def test_snapshot_creator_can_extend_existing_snapshot(self) -> None:
        base_creator = v8.SnapshotCreator()
        base_creator.eval("globalThis.baseValue = 20")
        base_snapshot = base_creator.create_blob()
        creator = v8.SnapshotCreator(base_snapshot)

        creator.eval("globalThis.extendedValue = baseValue + 22")
        snapshot = creator.create_blob(function_code_handling="keep")
        context = v8.Isolate(snapshot).create_context()

        self.assertEqual(context.eval("extendedValue").as_int32(), 42)

    def test_snapshot_creator_cannot_be_reused_after_blob_creation(self) -> None:
        creator = v8.SnapshotCreator()
        creator.create_blob()

        with self.assertRaises(RuntimeError):
            creator.eval("globalThis.unused = true")
