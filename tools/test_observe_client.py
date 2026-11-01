"""Client protocol/security regressions. Synthetic packets, not native evidence.
Run: python3 -m unittest discover -s tools -p test_observe_client.py -v
"""
import json
import os
from pathlib import Path
import socket
import struct
import tempfile
import threading
import unittest
from observe_client import MAX_JSON, observe, save_frame


class ClientTests(unittest.TestCase):
    def session(self, metadata=None, png=None, raw=None):
        directory = tempfile.TemporaryDirectory(prefix="rndr-client-test-")
        self.addCleanup(directory.cleanup)
        root = Path(directory.name)
        os.chmod(root, 0o700)
        endpoint = root / "observe.sock"
        listener = socket.socket(socket.AF_UNIX)
        listener.bind(str(endpoint))
        listener.listen(1)
        self.addCleanup(listener.close)
        descriptor = root / "session.json"
        descriptor.write_text(json.dumps({"protocol": "rndr-observe-1", "socket": str(endpoint), "capability": "a" * 64}))
        os.chmod(descriptor, 0o600)
        errors = []

        def send():
            try:
                listener.settimeout(2)
                with listener.accept()[0] as stream:
                    stream.settimeout(2)
                    request = b""
                    while not request.endswith(b"\n"):
                        request += stream.recv(1)
                    self.assertEqual(request, b"RNDR1 " + b"a" * 64 + b" 0\n")
                    if raw is not None:
                        stream.sendall(raw)
                    else:
                        data = json.dumps(metadata).encode()
                        stream.sendall(struct.pack(">I", len(data)) + data + (png or b""))
            except (BrokenPipeError, ConnectionResetError):
                pass  # Client correctly rejected a bounded prefix.
            except Exception as error:
                errors.append(error)
        worker = threading.Thread(target=send, daemon=True)
        worker.start()
        self.addCleanup(lambda: worker.join(timeout=3))
        return descriptor, errors

    @staticmethod
    def packet():
        # Header-only PNG envelope is enough to test client coherence guards;
        # production PNG encoding/decoding is covered by the Rust tests.
        png = b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR" + struct.pack(">II", 2, 3) + bytes(9)
        return {"protocol": "rndr-observe-1", "frame_revision": "1", "width": 2, "height": 3,
                "source": "live-surface-copy-before-present", "presented_confirmation": False,
                "png_bytes": len(png), "nodes": []}, png

    def test_matched_packet(self):
        metadata, png = self.packet()
        descriptor, errors = self.session(metadata, png)
        actual, image = observe(descriptor)
        self.assertEqual(actual, metadata)
        self.assertEqual(image, png)
        self.assertFalse(errors)

    def test_denial_never_returns_image(self):
        descriptor, _ = self.session({"protocol": "rndr-observe-1", "error": "sensitive_frame", "png_bytes": 0})
        with self.assertRaisesRegex(RuntimeError, "sensitive_frame"):
            observe(descriptor)

    def test_oversized_and_truncated_prefix(self):
        for prefix in [struct.pack(">I", MAX_JSON + 1), b"\x00\x01", struct.pack(">I", 100) + b"{}"]:
            with self.subTest(prefix=prefix):
                descriptor, _ = self.session(raw=prefix)
                with self.assertRaises(ValueError):
                    observe(descriptor)

    def test_dimension_revision_and_source_binding(self):
        for key, value in [("width", 4), ("height", 4), ("frame_revision", "0"), ("source", "headless"), ("presented_confirmation", True), ("png_bytes", 20_000_000)]:
            with self.subTest(key=key):
                meta, png = self.packet()
                meta[key] = value
                descriptor, _ = self.session(meta, png)
                with self.assertRaises(ValueError):
                    observe(descriptor)

    def test_invalid_descriptor_or_public_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            descriptor = root / "session.json"
            descriptor.write_text("{}")
            os.chmod(root, 0o755)
            with self.assertRaisesRegex(ValueError, "0700"):
                observe(descriptor)
            os.chmod(root, 0o700)
            with self.assertRaisesRegex(ValueError, "descriptor"):
                observe(descriptor)

    def test_external_socket_is_not_admitted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            os.chmod(root, 0o700)
            descriptor = root / "session.json"
            descriptor.write_text(json.dumps({"protocol": "rndr-observe-1", "capability": "a" * 64, "socket": "/tmp/another-app/observe.sock"}))
            os.chmod(descriptor, 0o600)
            with self.assertRaisesRegex(ValueError, "descriptor"):
                observe(descriptor)

    def test_output_refuses_overwrite_and_uses_private_files(self):
        with tempfile.TemporaryDirectory() as directory:
            prefix = Path(directory) / "frame"
            metadata, png = self.packet()
            save_frame(prefix, metadata, png)
            self.assertEqual((prefix.with_suffix(".png")).stat().st_mode & 0o777, 0o600)
            self.assertEqual((prefix.with_suffix(".json")).stat().st_mode & 0o777, 0o600)
            with self.assertRaises(FileExistsError):
                save_frame(prefix, metadata, png)

    def test_invalid_request_values_are_local_errors(self):
        for after in [-1, 2**64]:
            with self.assertRaises(ValueError):
                observe("not-opened", after)
        with self.assertRaises(ValueError):
            observe("not-opened", timeout=100)


if __name__ == "__main__":
    unittest.main()
