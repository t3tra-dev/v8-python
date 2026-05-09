import v8

from tests.support import V8TestCase


class BufferTests(V8TestCase):
    def test_array_buffer_round_trips_python_bytes(self):
        context = self.make_context()
        buffer = context.new_array_buffer(b"\x01\x02\x03")

        self.assertIsInstance(buffer, v8.ArrayBuffer)
        self.assertEqual(buffer.byte_length, 3)
        self.assertEqual(bytes(buffer), b"\x01\x02\x03")
        self.assertEqual(buffer.to_bytes(), b"\x01\x02\x03")
        self.assertEqual(buffer.memoryview().tobytes(), b"\x01\x02\x03")

        context.set_global("buffer", buffer)
        self.assertTrue(context.eval("buffer instanceof ArrayBuffer").as_boolean())
        self.assertEqual(context.eval("new Uint8Array(buffer)[1]").as_int32(), 2)

        context.eval("new Uint8Array(buffer)[2] = 9")
        self.assertEqual(bytes(buffer), b"\x01\x02\x09")

    def test_array_buffer_accepts_lengths_and_bytes_like_values(self):
        context = self.make_context()

        allocated = context.new_array_buffer(4)
        self.assertEqual(allocated.byte_length, 4)
        self.assertEqual(bytes(allocated), b"\x00\x00\x00\x00")

        from_bytearray = context.new_array_buffer(bytearray(b"\x04\x05"))
        self.assertEqual(bytes(from_bytearray), b"\x04\x05")

        from_memoryview = context.from_python(memoryview(b"\x06\x07"))
        self.assertTrue(from_memoryview.is_array_buffer())
        self.assertEqual(from_memoryview.to_python(), b"\x06\x07")

        with self.assertRaises(TypeError):
            context.new_array_buffer(True)

    def test_typed_array_and_data_view_wrappers(self):
        context = self.make_context()
        buffer = context.new_array_buffer(b"\x01\x02\x03\x04")

        typed_array = buffer.typed_array("Uint8Array", byte_offset=1, length=2)
        self.assertIsInstance(typed_array, v8.TypedArray)
        self.assertEqual(typed_array.type_name, "Uint8Array")
        self.assertEqual(len(typed_array), 2)
        self.assertEqual(typed_array.length, 2)
        self.assertEqual(typed_array.byte_offset, 1)
        self.assertEqual(typed_array.byte_length, 2)
        self.assertEqual(bytes(typed_array), b"\x02\x03")
        self.assertEqual(typed_array.memoryview().tobytes(), b"\x02\x03")
        self.assertEqual(bytes(typed_array.buffer()), b"\x01\x02\x03\x04")

        view = typed_array.to_value().as_v8_array_buffer_view()
        assert view is not None
        self.assertIsInstance(view, v8.ArrayBufferView)
        self.assertTrue(view.has_buffer())
        self.assertEqual(view.byte_offset, 1)
        self.assertEqual(view.byte_length, 2)
        self.assertEqual(bytes(view), b"\x02\x03")
        self.assertEqual(view.memoryview().tobytes(), b"\x02\x03")

        data_view = buffer.data_view(byte_offset=2, byte_length=2)
        self.assertIsInstance(data_view, v8.DataView)
        self.assertEqual(len(data_view), 2)
        self.assertEqual(data_view.byte_offset, 2)
        self.assertEqual(data_view.byte_length, 2)
        self.assertEqual(bytes(data_view), b"\x03\x04")
        self.assertEqual(data_view.memoryview().tobytes(), b"\x03\x04")

        context.set_global("dataView", data_view)
        self.assertTrue(context.eval("dataView instanceof DataView").as_boolean())

    def test_value_casts_for_javascript_buffer_types(self):
        context = self.make_context()

        typed_value = context.eval("new Uint8Array([5, 6, 7]).subarray(1)")
        self.assertTrue(typed_value.is_typed_array())
        self.assertTrue(typed_value.is_array_buffer_view())
        self.assertEqual(typed_value.to_python(), b"\x06\x07")

        typed_array = typed_value.as_v8_typed_array()
        assert typed_array is not None
        self.assertEqual(bytes(typed_array), b"\x06\x07")

        view = typed_value.as_v8_array_buffer_view()
        assert view is not None
        self.assertEqual(bytes(view), b"\x06\x07")

        data_view_value = context.eval("new DataView(new Uint8Array([8, 9]).buffer)")
        self.assertTrue(data_view_value.is_data_view())
        self.assertTrue(data_view_value.is_array_buffer_view())
        data_view = data_view_value.as_v8_data_view()
        assert data_view is not None
        self.assertEqual(bytes(data_view), b"\x08\x09")

        array_buffer_value = context.eval("new Uint8Array([10, 11]).buffer")
        self.assertTrue(array_buffer_value.is_array_buffer())
        array_buffer = array_buffer_value.as_v8_array_buffer()
        assert array_buffer is not None
        self.assertEqual(bytes(array_buffer), b"\x0a\x0b")
