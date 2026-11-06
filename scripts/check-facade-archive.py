#!/usr/bin/env python3
"""Facade manifest normalization before 0.4.0 dependencies are on the index.

`cargo package` would otherwise write a lockfile that resolves optional `ui`
and the `rndr_ui` dev-dependency from crates.io. This check excludes that
lockfile and skips the rebuild. Ordered registry verification is
`scripts/publish-crates.sh` once those versions are published.
"""
import argparse
import json
import pathlib
import subprocess
import tarfile
import tomllib


def check(allow_dirty=False):
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--offline", "--locked", "--no-deps",
        "--format-version", "1",
    ]))
    packages = {p["name"]: p for p in metadata["packages"]}
    facade = packages["rndr"]
    for name in ("rndr", "rndr_ui", "rndr_app", "rndr_core", "rndr_paint", "rndr_std", "rndr_term", "rndr_render"):
        assert packages[name]["publish"] is None, f"{name} must be publishable"
    for name in ("desktop", "source-session"):
        assert packages[name]["publish"] == [], f"{name} must stay unpublished"
    command = ["cargo", "package", "--offline", "--locked", "-p", "rndr",
               "--no-verify", "--exclude-lockfile"]
    if allow_dirty:
        command.append("--allow-dirty")
    subprocess.run(command, check=True)
    version = facade["version"]
    prefix = f"rndr-{version}/"
    archive = pathlib.Path(metadata["target_directory"]) / "package" / f"rndr-{version}.crate"
    source = pathlib.Path(facade["manifest_path"]).parent
    with tarfile.open(archive, "r:gz") as tar:
        names = tar.getnames()
        assert len(names) == len(set(names)), "duplicate archive entries"
        assert all(n.startswith(prefix) and ".." not in pathlib.PurePosixPath(n).parts for n in names)
        assert prefix + "Cargo.lock" not in names, "archive-only boundary changed"
        assert not any(n.endswith(".snap.new") for n in names)
        def read(name):
            return tar.extractfile(prefix + name).read()
        manifest = tomllib.loads(read("Cargo.toml").decode())
        assert manifest["package"]["name"] == "rndr"
        assert manifest["package"]["version"] == version
        assert manifest["package"]["rust-version"] == facade["rust_version"]
        assert manifest["features"]["ui"] == ["dep:rndr_ui"]
        assert "observe" in manifest["features"]
        assert "ui" not in manifest["features"]["default"]
        assert "observe" not in manifest["features"]["default"]
        ui = manifest["dependencies"]["rndr_ui"]
        assert ui["optional"] is True and ui["version"] == packages["rndr_ui"]["version"]
        dev_ui = manifest["dev-dependencies"]["rndr_ui"]
        assert dev_ui["version"] == packages["rndr_ui"]["version"]
        assert "path" not in dev_ui
        for table in ("dependencies", "dev-dependencies", "build-dependencies"):
            assert all("path" not in d for d in manifest.get(table, {}).values() if isinstance(d, dict))
        for name in ("src/lib.rs", "README.md", "Cargo.toml.orig"):
            original = "Cargo.toml" if name == "Cargo.toml.orig" else name
            assert read(name) == (source / original).read_bytes(), f"archive source mismatch: {name}"
    print(f"PASS: {archive} normalized archive/source checks; registry rebuild is the ordered publish dry-run")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow-dirty", action="store_true", help="local review only; CI uses clean sources")
    check(parser.parse_args().allow_dirty)
