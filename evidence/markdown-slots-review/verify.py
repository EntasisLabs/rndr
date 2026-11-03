#!/usr/bin/env python3
"""Verify review hashes and immutable inherited candidate evidence; no writes."""
import hashlib
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
CANDIDATE = "69f1452a9d9fbbaa4c2184e6d501d9e0b6274cc1"
MAIN = "e46a3205bdff534278f5281d63b48d5039f3a9dc"


def git(*args):
    return subprocess.check_output(["git", "-C", str(ROOT), *args])


def entries(data):
    for line in data.decode().splitlines():
        digest, path = line.split("  ", 1)
        yield digest, path


def sha(data):
    return hashlib.sha256(data).hexdigest()


def check_file_manifest(path):
    rows = list(entries(path.read_bytes()))
    for digest, name in rows:
        assert sha((ROOT / name).read_bytes()) == digest, name
    print(f"PASS {path.relative_to(ROOT)}: {len(rows)} files")


assert git("rev-parse", "main").decode().strip() == MAIN, "main moved"
for name in ("source.sha256", "evidence.sha256"):
    check_file_manifest(HERE / name)
parent_manifest = git("show", f"{CANDIDATE}:evidence/markdown-slots/source.sha256")
rows = list(entries(parent_manifest))
for digest, name in rows:
    assert sha(git("show", f"{CANDIDATE}:{name}")) == digest, name
print(f"PASS inherited candidate source manifest: {len(rows)} exact Git blobs")
paths = git("ls-tree", "-r", "--name-only", CANDIDATE, "--", "evidence/markdown-slots").decode().splitlines()
for name in paths:
    assert (ROOT / name).read_bytes() == git("show", f"{CANDIDATE}:{name}"), name
print(f"PASS immutable inherited evidence: {len(paths)} files")
check_file_manifest(ROOT / "evidence/markdown-slots/evidence.sha256")
print(f"PASS pinned main {MAIN}")
