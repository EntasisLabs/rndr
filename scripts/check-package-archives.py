#!/usr/bin/env python3
"""Verify the exact set of crates.io archives for a workspace package dry-run.

Derives the expected archive set from workspace packages that remain publishable
(`publish` unset / null). Unpublished examples (`publish = false`) must be
excluded from `cargo package --workspace` and must not appear as `.crate` files.

Used by `scripts/publish-crates.sh --dry-run` and the Linux CI packaging gate.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest


PUBLISH_ORDER = (
    "rndr_core",
    "rndr_paint",
    "rndr_std",
    "rndr_term",
    "rndr_ui",
    "rndr_app",
    "rndr_liquid",
    "rndr_chart",
    "rndr_liquid_ui",
    "rndr_canvas",
    "rndr_canvas_ui",
    "rndr_render",
    "rndr_media",
    "rndr_browser",
    "rndr",
)

LICENSES = {
    "rndr_paint": "MIT AND OFL-1.1",
    "rndr_render": "MIT AND OFL-1.1",
}


def cargo_metadata(locked=True, no_deps=True):
    command = ["cargo", "metadata", "--format-version", "1"]
    if locked:
        command.append("--locked")
    if no_deps:
        command.append("--no-deps")
    return json.loads(subprocess.check_output(command))


def workspace_packages(metadata=None):
    metadata = metadata or cargo_metadata()
    return {package["name"]: package for package in metadata["packages"]}


def is_publishable(package):
    publish = package.get("publish")
    return publish is None


def publishable_packages(packages=None):
    packages = packages or workspace_packages()
    publishable = {name: packages[name] for name in packages if is_publishable(packages[name])}
    missing = [name for name in PUBLISH_ORDER if name not in publishable]
    if missing:
        raise SystemExit(f"publish order references unpublished or missing crates: {missing}")
    unexpected = sorted(set(publishable) - set(PUBLISH_ORDER))
    if unexpected:
        raise SystemExit(
            "publishable workspace crates are not in the release upload order: "
            f"{unexpected}"
        )
    return {name: publishable[name] for name in PUBLISH_ORDER}


def unpublished_packages(packages=None):
    packages = packages or workspace_packages()
    return sorted(name for name, package in packages.items() if not is_publishable(package))


def package_exclude_args(packages=None):
    return [arg for name in unpublished_packages(packages) for arg in ("--exclude", name)]


def expected_archives(packages=None):
    publishable = publishable_packages(packages)
    return [f"{name}-{publishable[name]['version']}.crate" for name in PUBLISH_ORDER]


def list_archives(package_dir: pathlib.Path):
    return sorted(path.name for path in package_dir.glob("*.crate"))


def verify_archive_set(package_dir: pathlib.Path, packages=None):
    """Reject missing and unexpected archives against the publishable set."""
    packages = packages or workspace_packages()
    expected = expected_archives(packages)
    found = list_archives(package_dir)
    expected_set = set(expected)
    found_set = set(found)
    missing = sorted(expected_set - found_set)
    unexpected = sorted(found_set - expected_set)
    if missing or unexpected:
        parts = []
        if missing:
            parts.append(f"missing: {', '.join(missing)}")
        if unexpected:
            parts.append(f"unexpected: {', '.join(unexpected)}")
        raise SystemExit(
            "packaged archive set does not match publishable crates ("
            + "; ".join(parts)
            + f"); expected {len(expected)} archives"
        )
    return found


def verify_archive_contents(package_dir: pathlib.Path, packages=None):
    packages = packages or workspace_packages()
    publishable = publishable_packages(packages)
    found = verify_archive_set(package_dir, packages)
    workspace = set(PUBLISH_ORDER)
    for name in PUBLISH_ORDER:
        version = publishable[name]["version"]
        archive_name = f"{name}-{version}.crate"
        with tarfile.open(package_dir / archive_name) as tar:
            archived = tar.getnames()
            if f"{name}-{version}/LICENSE" not in archived:
                raise SystemExit(f"{name} archive is missing LICENSE")
            manifest = tomllib.loads(
                tar.extractfile(f"{name}-{version}/Cargo.toml").read().decode()
            )
        package = manifest["package"]
        if package["version"] != version:
            raise SystemExit(f"{name} archive version {package['version']} != {version}")
        if package.get("license") != LICENSES.get(name, "MIT"):
            raise SystemExit(f"{name} license {package.get('license')!r}")
        if package.get("publish") is False or package.get("publish") == []:
            raise SystemExit(f"{name} manifest sets publish = false")
        if not package.get("description") or not package.get("readme"):
            raise SystemExit(f"{name} is missing description or readme")
        if name in ("rndr_ui", "rndr_app", "rndr_browser") and not str(
            package.get("documentation", "")
        ).startswith("https://docs.rs/"):
            raise SystemExit(f"{name} documentation URL {package.get('documentation')!r}")
        if name == "rndr":
            features = manifest.get("features", {})
            if features.get("ui") != ["dep:rndr_ui"] or "observe" not in features:
                raise SystemExit(f"facade features changed: ui={features.get('ui')!r}")
            if "ui" in features.get("default", []) or "observe" in features.get("default", []):
                raise SystemExit("ui or observe became a default feature")
            if "browser" in features.get("default", []):
                raise SystemExit("browser became a default feature")
            if features.get("browser") != ["dep:rndr_browser"]:
                raise SystemExit(f"browser feature changed: {features.get('browser')!r}")
            if "media" in features.get("default", []):
                raise SystemExit("media became a default feature")
            if features.get("media") != ["dep:rndr_media"]:
                raise SystemExit(f"media feature changed: {features.get('media')!r}")
            for feature in ("chart", "liquid", "canvas"):
                if feature not in features or feature in features.get("default", []):
                    raise SystemExit(f"{feature} must be an optional, non-default feature")
        for table_name in ("dependencies", "dev-dependencies"):
            for dep_name, spec in manifest.get(table_name, {}).items():
                if dep_name not in workspace or not isinstance(spec, dict):
                    continue
                if "path" in spec:
                    raise SystemExit(f"{name} packaged {table_name}.{dep_name} still has a path")
                if spec.get("version") != version:
                    raise SystemExit(
                        f"{name} {table_name}.{dep_name} version {spec.get('version')}, want {version}"
                    )
                if (
                    name == "rndr"
                    and table_name == "dependencies"
                    and dep_name == "rndr_ui"
                    and spec.get("optional") is not True
                ):
                    raise SystemExit(f"{name} rndr_ui dependency is not optional: {spec}")
                if (
                    name == "rndr"
                    and table_name == "dependencies"
                    and dep_name
                    in (
                        "rndr_browser",
                        "rndr_media",
                        "rndr_chart",
                        "rndr_liquid",
                        "rndr_liquid_ui",
                        "rndr_canvas",
                        "rndr_canvas_ui",
                    )
                    and spec.get("optional") is not True
                ):
                    raise SystemExit(f"{name} {dep_name} dependency is not optional: {spec}")
    print("packaged archives:", ", ".join(found))
    return found


def run_workspace_package(packages=None):
    packages = packages or workspace_packages()
    command = [
        "cargo",
        "package",
        "--workspace",
        "--locked",
        "--no-verify",
        *package_exclude_args(packages),
    ]
    print(" ".join(command))
    subprocess.run(command, check=True)


class ArchiveSetTests(unittest.TestCase):
    def test_extra_archive_is_rejected(self):
        packages = {
            name: {"name": name, "version": "0.5.0", "publish": None} for name in PUBLISH_ORDER
        }
        packages["titlebar"] = {"name": "titlebar", "version": "0.5.0", "publish": []}
        packages["lifecycle"] = {"name": "lifecycle", "version": "0.5.0", "publish": []}
        with tempfile.TemporaryDirectory() as temp:
            root = pathlib.Path(temp)
            for name in expected_archives(packages):
                (root / name).write_bytes(b"fake")
            (root / "titlebar-0.5.0.crate").write_bytes(b"extra")
            with self.assertRaises(SystemExit) as raised:
                verify_archive_set(root, packages)
            message = str(raised.exception)
            self.assertIn("unexpected: titlebar-0.5.0.crate", message)
            self.assertIn("expected 15 archives", message)

    def test_missing_archive_is_rejected(self):
        packages = {
            name: {"name": name, "version": "0.5.0", "publish": None} for name in PUBLISH_ORDER
        }
        with tempfile.TemporaryDirectory() as temp:
            root = pathlib.Path(temp)
            for name in expected_archives(packages)[:-1]:
                (root / name).write_bytes(b"fake")
            with self.assertRaises(SystemExit) as raised:
                verify_archive_set(root, packages)
            self.assertIn("missing: rndr-0.5.0.crate", str(raised.exception))

    def test_exact_set_passes(self):
        packages = {
            name: {"name": name, "version": "0.5.0", "publish": None} for name in PUBLISH_ORDER
        }
        with tempfile.TemporaryDirectory() as temp:
            root = pathlib.Path(temp)
            for name in expected_archives(packages):
                (root / name).write_bytes(b"fake")
            found = verify_archive_set(root, packages)
            self.assertEqual(len(found), 15)

    def test_exclude_args_cover_unpublished_examples(self):
        packages = {
            name: {"name": name, "version": "0.5.0", "publish": None} for name in PUBLISH_ORDER
        }
        packages["titlebar"] = {"name": "titlebar", "version": "0.5.0", "publish": []}
        packages["lifecycle"] = {"name": "lifecycle", "version": "0.5.0", "publish": []}
        args = package_exclude_args(packages)
        self.assertEqual(args.count("--exclude"), 2)
        self.assertIn("titlebar", args)
        self.assertIn("lifecycle", args)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--package-dir",
        type=pathlib.Path,
        default=None,
        help="Directory of *.crate files (default: target/package from cargo metadata)",
    )
    parser.add_argument(
        "--run-package",
        action="store_true",
        help="Run cargo package --workspace with excludes derived from publish = false",
    )
    parser.add_argument(
        "--contents",
        action="store_true",
        help="Also check licenses, manifests and facade feature shape inside archives",
    )
    parser.add_argument(
        "--print-excludes",
        action="store_true",
        help="Print --exclude arguments for unpublished workspace members and exit",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run archive-set regression tests (extra/missing archives)",
    )
    args = parser.parse_args(argv)

    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(ArchiveSetTests)
        result = unittest.TextTestRunner(verbosity=2).run(suite)
        return 0 if result.wasSuccessful() else 1

    packages = workspace_packages()
    if args.print_excludes:
        print(" ".join(package_exclude_args(packages)))
        return 0

    if args.run_package:
        run_workspace_package(packages)

    metadata = cargo_metadata()
    package_dir = args.package_dir or pathlib.Path(metadata["target_directory"]) / "package"
    if args.contents:
        verify_archive_contents(package_dir, packages)
    else:
        found = verify_archive_set(package_dir, packages)
        print("packaged archives:", ", ".join(found))
    return 0


if __name__ == "__main__":
    sys.exit(main())
