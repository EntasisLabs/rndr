#!/usr/bin/env python3
"""Packaging control-flow regressions with fake platform/build tools, not native signoff."""
import os
import plistlib
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/package-desktop-macos.sh"
ICON_PACKAGER = ROOT / "scripts/package-app-icon.py"
ICONS = ROOT / "examples/desktop/icons"


class Packaging(unittest.TestCase):
    def setUp(self):
        (ROOT / "target").mkdir(exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix="package-test-", dir=ROOT / "target")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.bin = self.base / "tools"
        self.bin.mkdir()
        self.target = self.base / "cargo-output"
        self.env = dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}",
                        CARGO_TARGET_DIR=str(self.target),
                        CARGO_BUILD_TARGET="foreign-target-from-config",
                        PACKAGE_TEST_BASE=str(self.base))
        self.tool("uname", "print('Darwin')")
        self.tool("df", "print('Filesystem blocks used available'); print('test 9999999 0 9999999')")
        self.tool("rustc", "print('host: aarch64-apple-darwin')")
        # Enter the actual packager with a simulated Mac platform.
        self.tool("python3", "import runpy; sys.platform = 'darwin'; "
                  "script = sys.argv.pop(1); runpy.run_path(script, run_name='__main__')")
        self.tool("sips", "Path(sys.argv[-1]).write_bytes(b'scaled PNG')")
        self.tool("iconutil", "assert len(list(Path(sys.argv[3]).glob('*.png'))) == 10; "
                  "Path(sys.argv[-1]).write_bytes(b'icon')")
        self.tool("plutil", "assert Path(sys.argv[-1]).is_file()")
        self.tool("cargo", r"""
args = sys.argv[1:]
base = Path(os.environ['PACKAGE_TEST_BASE'])
(base / 'build-args').write_text('\n'.join(args))
if os.environ.get('PACKAGE_TEST_FAIL'):
    sys.exit(7)
host = args[args.index('--target') + 1]
target = Path(args[args.index('--target-dir') + 1])
profile = args[args.index('--profile') + 1]
binary = target / host / ('debug' if profile == 'dev' else 'release') / 'desktop'
binary.parent.mkdir(parents=True, exist_ok=True)
binary.write_text('fresh native artifact')
""")

    def tool(self, name, body):
        path = self.bin / name
        path.write_text(f"#!{sys.executable}\nimport os, sys\nfrom pathlib import Path\n{body}\n")
        path.chmod(0o755)

    def run_package(self, profile="dev"):
        return subprocess.run(["sh", str(SCRIPT), profile], env=self.env,
                              capture_output=True, text=True, check=False)

    def test_fresh_explicit_native_target_wins_over_stale_host_binary(self):
        stale = self.target / "debug/desktop"
        stale.parent.mkdir(parents=True)
        stale.write_text("stale host artifact")
        result = self.run_package()
        self.assertEqual(result.returncode, 0, result.stderr)
        binary = self.target / "bundle/Desktop Workshop.app/Contents/MacOS/DesktopWorkshop"
        self.assertEqual(binary.read_text(), "fresh native artifact")
        self.assertEqual(stale.read_text(), "stale host artifact")
        args = (self.base / "build-args").read_text().splitlines()
        self.assertEqual(args[args.index("--target") + 1], "aarch64-apple-darwin")
        self.assertEqual(args[args.index("--target-dir") + 1], str(self.target))
        contents = binary.parents[1]
        with (contents / "Info.plist").open("rb") as f:
            plist = plistlib.load(f)
        self.assertEqual(plist["CFBundleIdentifier"], "org.rndr.desktop-workshop")
        self.assertEqual(plist["CFBundleExecutable"], binary.name)
        self.assertEqual((contents / "Resources" / plist["CFBundleIconFile"]).read_bytes(), b"icon")
        self.assertFalse(list((self.target / "bundle").glob(".rndr-icon-*")))
        # Existing bundles are preserved and refused before another build.
        (self.base / "build-args").unlink()
        result = self.run_package()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Refusing to replace existing bundle", result.stderr)
        self.assertEqual(binary.read_text(), "fresh native artifact")
        self.assertFalse((self.base / "build-args").exists())

    def test_failed_build_never_bundles_stale_binary(self):
        stale = self.target / "release/desktop"
        stale.parent.mkdir(parents=True)
        stale.write_text("stale")
        self.env["PACKAGE_TEST_FAIL"] = "1"
        result = self.run_package("release")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.target / "bundle/Desktop Workshop.app").exists())
        self.assertEqual(stale.read_text(), "stale")


    def linux_package(self, *extra):
        binary = self.base / "example-binary"
        binary.write_bytes(b"executable")
        return subprocess.run([sys.executable, str(ICON_PACKAGER), "linux",
                               "--binary", str(binary), "--icon", str(ICONS / "window.png"),
                               "--id", "org.example.Workshop", "--name", "Workshop Café",
                               "--output", str(self.base / "stage"), *extra],
                              text=True, capture_output=True, check=False)

    def test_linux_stage_joins_identity_launcher_and_artwork(self):
        result = self.linux_package("--prefix", "/home/example/My Apps")
        self.assertEqual(result.returncode, 0, result.stderr)
        root = self.base / "stage"
        app_id = "org.example.Workshop"
        executable = root / "bin" / app_id
        self.assertEqual(executable.read_bytes(), b"executable")
        self.assertTrue(os.access(executable, os.X_OK))
        icon = root / "share/icons/hicolor/256x256/apps" / f"{app_id}.png"
        self.assertEqual(icon.read_bytes(), (ICONS / "window.png").read_bytes())
        launcher = root / "share/applications" / f"{app_id}.desktop"
        self.assertIn(f'Exec="/home/example/My Apps/bin/{app_id}"\n', launcher.read_text())
        self.assertIn(f"Icon={app_id}\nStartupWMClass={app_id}\n", launcher.read_text())
        result = self.linux_package()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("refusing to replace existing output", result.stderr)

    def test_invalid_linux_input_leaves_no_stage(self):
        for extra in [("--id", "../../evil"), ("--prefix", "/usr/%k"),
                      ("--name", "App\nExec=unexpected"), ("--icon", str(ICONS / "app.ico"))]:
            with self.subTest(extra=extra):
                result = self.linux_package(*extra)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((self.base / "stage").exists())
                self.assertFalse(list(self.base.glob(".rndr-icon-*")))


if __name__ == "__main__":
    unittest.main()
