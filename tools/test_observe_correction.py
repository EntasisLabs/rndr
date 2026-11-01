"""Correction regressions: local synthetic files/packets, not native evidence."""
import json
import os
from pathlib import Path
import tempfile
import threading
import time
import unittest
from unittest import mock
import observe_client as client


class CorrectionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        os.chmod(self.root, 0o700)
        self.path = self.root / "session.json"
        self.prefix = self.root / "frame"

    def descriptor(self, data=b"{}"):
        self.path.write_bytes(data)
        os.chmod(self.path, 0o600)

    def test_fifo_rejected_without_writer(self):
        os.mkfifo(self.path, 0o600)
        start = time.monotonic()
        with self.assertRaises(ValueError):
            client.observe(self.path, timeout=.05)
        self.assertLess(time.monotonic() - start, .5)

    def test_descriptor_size_type_permissions_json(self):
        for data in [b"x" * 4097, b"", b"not json", b"[]", b'{"capability":1}',
                     b'{"capability":"a", "socket":[]}']:
            with self.subTest(data=data[:30]):
                self.descriptor(data)
                with self.assertRaises(ValueError):
                    client.observe(self.path, timeout=.05)
        self.descriptor()
        os.chmod(self.path, 0o644)
        with self.assertRaises(ValueError):
            client.observe(self.path)
        self.path.unlink()
        self.path.mkdir()
        with self.assertRaises(ValueError):
            client.observe(self.path)

    def test_symlink_and_open_race_do_not_follow_target(self):
        target = self.root / "target"
        target.write_text("secret")
        self.path.symlink_to(target)
        with self.assertRaises(OSError):
            client.observe(self.path)
        self.path.unlink()
        self.descriptor()
        original = os.open
        def swap(path, flags, *args, **kwargs):
            if path == self.path.name:
                self.path.unlink()
                self.path.symlink_to(target)
            return original(path, flags, *args, **kwargs)
        with mock.patch.object(client.os, "open", side_effect=swap):
            with self.assertRaises(OSError):
                client.observe(self.path)
        self.assertEqual(target.read_text(), "secret")

    def test_growth_bounded_and_deadline_includes_read(self):
        self.descriptor()
        original = os.read
        def grow(fd, count):
            self.assertEqual(count, 4097)
            self.path.write_bytes(b"x" * 5000)
            return original(fd, count)
        with mock.patch.object(client.os, "read", side_effect=grow):
            with self.assertRaises(ValueError):
                client.observe(self.path)
        self.descriptor()
        def slow(fd, count):
            time.sleep(.02)
            return original(fd, count)
        with mock.patch.object(client.os, "read", side_effect=slow):
            with self.assertRaises(TimeoutError):
                client.observe(self.path, timeout=.005)

    def test_existing_either_output_preserved_without_partial_pair(self):
        for suffix in [".png", ".json"]:
            old = Path(str(self.prefix) + suffix)
            old.write_bytes(b"old")
            with self.assertRaises(FileExistsError):
                client.save_frame(self.prefix, {}, b"new")
            self.assertEqual(old.read_bytes(), b"old")
            other = Path(str(self.prefix) + (".json" if suffix == ".png" else ".png"))
            self.assertFalse(other.exists())
            old.unlink()

    def test_second_publication_failure_rolls_back_png(self):
        original = os.link
        def fail(src, dst, **kwargs):
            if str(dst).endswith(".json"):
                raise OSError("injected second publication failure")
            return original(src, dst, **kwargs)
        with mock.patch.object(client.os, "link", side_effect=fail):
            with self.assertRaises(OSError):
                client.save_frame(self.prefix, {}, b"new")
        self.assertEqual(list(self.root.iterdir()), [])

    def test_second_write_failure_cleans_staging_and_outputs(self):
        original = os.fdopen
        def fail(fd, *args, **kwargs):
            wrapped = original(fd, *args, **kwargs)
            if fail.calls:
                class FailedWrite:
                    def __enter__(self):
                        return self
                    def write(self, data):
                        raise OSError("injected second write failure")
                    def __exit__(self, *exc):
                        wrapped.close()
                return FailedWrite()
            fail.calls += 1
            return wrapped
        fail.calls = 0
        with mock.patch.object(client.os, "fdopen", side_effect=fail):
            with self.assertRaises(OSError):
                client.save_frame(self.prefix, {}, b"new")
        self.assertEqual(list(self.root.iterdir()), [])

    def test_collision_race_preserves_raced_json(self):
        original = os.link
        def race(src, dst, **kwargs):
            if str(dst).endswith(".json"):
                self.prefix.with_suffix(".json").write_bytes(b"raced")
            return original(src, dst, **kwargs)
        with mock.patch.object(client.os, "link", side_effect=race):
            with self.assertRaises(FileExistsError):
                client.save_frame(self.prefix, {}, b"new")
        self.assertEqual(self.prefix.with_suffix(".json").read_bytes(), b"raced")
        self.assertFalse(self.prefix.with_suffix(".png").exists())

    def test_rollback_preserves_observed_replacement(self):
        original = os.link
        def race(src, dst, **kwargs):
            if str(dst).endswith(".json"):
                image = self.prefix.with_suffix(".png")
                image.unlink()
                image.write_bytes(b"replacement")
                raise OSError("raced replacement")
            return original(src, dst, **kwargs)
        with mock.patch.object(client.os, "link", side_effect=race):
            with self.assertRaises(OSError):
                client.save_frame(self.prefix, {}, b"new")
        self.assertEqual(self.prefix.with_suffix(".png").read_bytes(), b"replacement")

    def test_output_symlink_collision_preserves_target_and_link(self):
        target = self.root / "target"
        target.write_bytes(b"untouched")
        for suffix in [".png", ".json"]:
            path = Path(str(self.prefix) + suffix)
            path.symlink_to(target)
            with self.assertRaises(FileExistsError):
                client.save_frame(self.prefix, {}, b"new")
            self.assertTrue(path.is_symlink())
            self.assertEqual(target.read_bytes(), b"untouched")
            other = Path(str(self.prefix) + (".json" if suffix == ".png" else ".png"))
            self.assertFalse(other.exists())
            path.unlink()

    def test_descriptor_fifo_substitution_at_open_is_rejected(self):
        self.descriptor()
        original = os.open
        def swap(path, flags, *args, **kwargs):
            if path == self.path.name:
                self.path.unlink()
                os.mkfifo(self.path, 0o600)
            return original(path, flags, *args, **kwargs)
        start = time.monotonic()
        with mock.patch.object(client.os, "open", side_effect=swap):
            with self.assertRaises(ValueError):
                client.observe(self.path, timeout=.05)
        self.assertLess(time.monotonic() - start, .5)

    def test_concurrent_publishers_return_one_consistent_pair(self):
        barrier = threading.Barrier(2)
        outcomes = []
        def publish(value):
            barrier.wait()
            try:
                client.save_frame(self.prefix, {"owner": value}, value.encode())
                outcomes.append(value)
            except FileExistsError:
                pass
        workers = [threading.Thread(target=publish, args=(v,)) for v in ["one", "two"]]
        for worker in workers:
            worker.start()
        for worker in workers:
            worker.join(2)
            self.assertFalse(worker.is_alive())
        self.assertEqual(len(outcomes), 1)
        self.assertEqual(self.prefix.with_suffix(".png").read_text(), outcomes[0])
        self.assertEqual(json.loads(self.prefix.with_suffix(".json").read_text())["owner"], outcomes[0])


if __name__ == "__main__":
    unittest.main()
