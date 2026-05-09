import v8

from tests.support import V8TestCase


class EmbedderDataTests(V8TestCase):
    def test_private_properties_are_hidden_from_js_enumeration(self) -> None:
        context = self.make_context()
        private = context.new_private("slot")
        obj = context.eval("({ public: 1 })").as_v8_object()
        assert obj is not None
        context.set_global("obj", obj)

        self.assertEqual(private.name, "slot")
        self.assertTrue(obj.set_private(private, "hidden"))
        self.assertTrue(obj.has_private(private))
        self.assertEqual(str(obj.get_private(private)), "hidden")
        self.assertEqual(
            context.eval("Object.keys(obj).join(',')").to_string(), "public"
        )

        self.assertTrue(obj.delete_private(private))
        self.assertFalse(obj.has_private(private))

    def test_private_for_api_reuses_same_key_in_isolate(self) -> None:
        context = self.make_context()
        first = context.private_for_api("shared")
        second = context.private_for_api("shared")
        obj = context.eval("({})").as_v8_object()
        assert obj is not None

        self.assertTrue(obj.set_private(first, 42))
        self.assertEqual(obj.get_private(second).as_int32(), 42)

    def test_private_without_name_has_undefined_name_value(self) -> None:
        context = self.make_context()
        private = context.new_private()

        self.assertIsNone(private.name)
        self.assertTrue(private.name_value().is_undefined())

    def test_external_wraps_python_payload(self) -> None:
        context = self.make_context()
        payload: dict[str, int] = {"answer": 42}
        external = context.new_external(payload)

        self.assertIs(external.payload(), payload)
        self.assertTrue(external.is_managed())
        self.assertIsNotNone(external.id)

        context.set_global("external", external)
        value = context.get_global("external")
        self.assertEqual(value.kind, "external")
        self.assertTrue(value.is_external())
        self.assertIs(value.to_python(), payload)

        wrapped = value.as_v8_external()
        assert wrapped is not None
        self.assertIs(wrapped.payload(), payload)

    def test_internal_fields_hold_values_and_private_data(self) -> None:
        context = self.make_context()
        payload: dict[str, int] = {"answer": 42}
        external = context.new_external(payload)
        private = context.new_private("field")
        obj = context.new_object(internal_field_count=2)

        self.assertEqual(obj.internal_field_count, 2)
        self.assertTrue(obj.set_internal_field(0, external))
        self.assertTrue(obj.set_internal_field(1, private))
        self.assertFalse(obj.set_internal_field(2, external))

        field0 = obj.get_internal_field(0)
        assert isinstance(field0, v8.Value)
        self.assertTrue(field0.is_external())
        self.assertIs(field0.to_python(), payload)

        field0_external = field0.as_v8_external()
        assert field0_external is not None
        self.assertIs(field0_external.payload(), payload)

        field1 = obj.get_internal_field(1)
        assert isinstance(field1, v8.Private)
        self.assertEqual(field1.name, "field")

        self.assertIsNone(obj.get_internal_field(2))

        value = obj.to_value()
        self.assertEqual(value.internal_field_count(), 2)
        value_field1 = value.get_internal_field(1)
        assert isinstance(value_field1, v8.Private)
        self.assertEqual(value_field1.name, "field")
