import v8

from tests.support import V8TestCase


class ObjectPropertyTests(V8TestCase):
    def test_define_own_property_with_attributes(self):
        context = self.make_context()
        obj = context.eval("({})")
        context.set_global("obj", obj)

        attributes = v8.PropertyAttribute(
            read_only=True,
            dont_enum=True,
            dont_delete=True,
        )
        self.assertTrue(obj.define_own_property("answer", 42, attributes))

        descriptor = obj.get_own_property_descriptor("answer")
        assert descriptor is not None
        value = descriptor.value
        assert isinstance(value, v8.Value)
        self.assertEqual(value.as_int32(), 42)
        self.assertFalse(descriptor.writable)
        self.assertFalse(descriptor.enumerable)
        self.assertFalse(descriptor.configurable)

        observed = obj.get_property_attributes("answer")
        self.assertTrue(observed.read_only)
        self.assertTrue(observed.dont_enum)
        self.assertTrue(observed.dont_delete)
        self.assertFalse(observed.writable)
        self.assertFalse(observed.enumerable)
        self.assertFalse(observed.configurable)

        self.assertEqual(context.eval("Object.keys(obj).length").as_int32(), 0)
        self.assertFalse(context.eval("delete obj.answer").as_boolean())
        context.eval("obj.answer = 100")
        self.assertEqual(obj["answer"].as_int32(), 42)

    def test_define_property_accepts_data_descriptor(self):
        context = self.make_context()
        obj = context.eval("({})")
        context.set_global("obj", obj)

        descriptor = v8.PropertyDescriptor.data(
            "ok",
            writable=False,
            enumerable=True,
            configurable=True,
        )
        self.assertTrue(obj.define_property("label", descriptor))
        self.assertEqual(str(obj["label"]), "ok")

        observed = obj.get_own_property_descriptor("label")
        assert observed is not None
        self.assertTrue(observed.has_value())
        self.assertTrue(observed.has_writable())
        self.assertFalse(observed.writable)
        self.assertTrue(observed.enumerable)
        self.assertTrue(observed.configurable)

        context.eval("obj.label = 'changed'")
        self.assertEqual(str(obj["label"]), "ok")

    def test_define_property_accepts_accessor_descriptor(self):
        context = self.make_context()
        obj = context.eval("({ answer: 41 })").as_v8_object()
        assert obj is not None

        getter = context.eval("(function() { return this.answer + 1; })")
        descriptor = v8.PropertyDescriptor.accessor(
            get=getter,
            enumerable=True,
            configurable=True,
        )

        self.assertTrue(obj.define_property("next", descriptor))
        self.assertEqual(obj["next"].as_int32(), 42)

        observed = obj.get_own_property_descriptor("next")
        assert observed is not None
        self.assertTrue(observed.has_get())
        self.assertFalse(observed.has_value())
        self.assertTrue(observed.enumerable)
        self.assertTrue(observed.configurable)

    def test_integrity_helpers_freeze_and_seal_objects(self):
        context = self.make_context()
        sealed = context.eval("({ removable: true })")
        frozen = context.eval("({ value: 1 })").as_v8_object()
        assert frozen is not None

        context.set_global("sealed", sealed)
        context.set_global("frozen", frozen)

        self.assertTrue(sealed.seal())
        self.assertTrue(context.eval("Object.isSealed(sealed)").as_boolean())
        self.assertFalse(context.eval("delete sealed.removable").as_boolean())

        self.assertTrue(frozen.freeze())
        self.assertTrue(context.eval("Object.isFrozen(frozen)").as_boolean())
        context.eval("frozen.value = 99")
        self.assertEqual(frozen["value"].as_int32(), 1)

    def test_property_descriptor_rejects_invalid_mixed_descriptor(self):
        context = self.make_context()
        getter = context.eval("(function() { return 1; })")

        with self.assertRaises(TypeError):
            v8.PropertyDescriptor(writable=True)

        with self.assertRaises(TypeError):
            v8.PropertyDescriptor(1, get=getter)
