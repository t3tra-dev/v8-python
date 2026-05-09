from tests.support import V8TestCase


class AtomicsTests(V8TestCase):
    def test_shared_array_buffer_and_atomics_are_profile_installed_host_apis(self):
        bare_context = self.make_context()
        self.assertEqual(str(bare_context.eval("typeof SharedArrayBuffer")), "function")
        self.assertEqual(str(bare_context.eval("typeof Atomics")), "object")
        self.assertEqual(
            str(bare_context.eval("typeof SharedArrayBuffer.fromHost")), "undefined"
        )

        context = self.make_shared_memory_context()
        self.assertEqual(
            str(context.eval("typeof SharedArrayBuffer.fromHost")), "function"
        )
        self.assertTrue(
            context.eval(
                """
                const buffer = SharedArrayBuffer.fromHost(4);
                const view = new Int32Array(buffer);
                view[0] = 7;
                const previous = Atomics.add(view, 0, 5);
                previous === 7 && view[0] === 12 && buffer.byteLength === 4;
                """
            ).as_boolean()
        )
        self.assertEqual(
            str(
                context.eval(
                    "Atomics.wait(new Int32Array(SharedArrayBuffer.fromHost(4)), 0, 0, 0)"
                )
            ),
            "timed-out",
        )

    def test_atomics_wait_can_be_disabled(self):
        context = self.make_shared_memory_context(allow_wait=False)

        with self.assertRaisesRegex(RuntimeError, "Atomics.wait"):
            context.eval(
                "Atomics.wait(new Int32Array(SharedArrayBuffer.fromHost(4)), 0, 0, 0)"
            )
