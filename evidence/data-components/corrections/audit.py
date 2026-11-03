#!/usr/bin/env python3
"""Correction evidence integrity, not independent acceptance or manual certification."""
import hashlib
import json
import re
from pathlib import Path
import struct
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
BASE = "b4eddfc9b22439209ab274f237fb3f1bce3ecedb"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def git_blob(path):
    return subprocess.check_output(["git", "show", BASE + ":" + path], cwd=ROOT)


def check_manifest(path, historical=False):
    count = 0
    for line in path.read_text().splitlines():
        digest, relative = line.split("  ", 1)
        target = ROOT / relative
        assert target.resolve().is_relative_to(ROOT), relative
        data = git_blob(relative) if historical else target.read_bytes()
        assert sha(data) == digest, relative
        count += 1
    return count


source_count = check_manifest(HERE / "source.sha256")
evidence_count = check_manifest(HERE / "MANIFEST.sha256")
historical_source_count = check_manifest(HERE.parent / "source.sha256", True)
historical_evidence_count = check_manifest(HERE.parent / "MANIFEST.sha256", True)
# All pre-correction implementation and independent-review evidence must remain
# byte-for-byte on disk. New correction evidence is deliberately separate.
original_paths = subprocess.check_output(
    ["git", "ls-tree", "-r", "--name-only", BASE, "--", "evidence/data-components"],
    cwd=ROOT, text=True).splitlines()
for relative in original_paths:
    assert (ROOT / relative).read_bytes() == git_blob(relative), relative

binary = json.loads((HERE / "binary.json").read_text())
subprocess.run(["git", "merge-base", "--is-ancestor", binary["source_commit"], "HEAD"], cwd=ROOT, check=True)
for line in (HERE / "source.sha256").read_text().splitlines():
    digest, relative = line.split("  ", 1)
    assert sha(subprocess.check_output(["git", "show", binary["source_commit"] + ":" + relative], cwd=ROOT)) == digest, relative
# Compare all six preserved reviewer cases, not selected assertion substrings.
fixture = (HERE.parent / "independent-review/adversarial.rs").read_text()
normalized = subprocess.check_output(["rustfmt", "--edition", "2021", "--emit", "stdout"], input=fixture, text=True)
assert (ROOT / "crates/rndr_ui/tests/collection_chronology.rs").read_text().startswith(normalized.rstrip() + "\n")
for log, expected in [("core-paint-ui", 570), ("native-observe", 73), ("fallback-renderer", 25), ("facade", 1), ("facade-fallback", 1), ("collection-metrics", 17)]:
    text = (HERE / "verification" / (log + ".log")).read_text()
    assert "EXIT=0" in text, log
    assert sum(map(int, re.findall(r"test result: ok\. (\d+) passed", text))) == expected, log
for log in ["formatting", "python", "gallery-build", "gallery-fallback-check"]:
    assert "EXIT=0" in (HERE / "verification" / (log + ".log")).read_text(), log
assert "6 passed; 0 failed" in (HERE / "adversarial-six.log").read_text()
assert "EXIT=0" in (HERE / "adversarial-six.log").read_text()
inspection = json.loads((HERE / "screenshot-inspection.json").read_text())
states = ["light", "dark", "narrow", "tree-focus", "disabled", "empty", "filtered", "compact-hc-200", "large"]
assert [r["state"] for r in inspection] == states
for name in states:
    folder = HERE / ("native-" + name)
    result = json.loads((folder / "result.json").read_text())
    assert result["status"] == "captured" and result["exit_code"] == 0, name
    assert all(result[k] for k in ["stale_rejection", "invalid_auth_rejection", "normal_shutdown_cleanup"]), name
    assert result["binary_sha256"] == binary["sha256"], name
    first, second = [(folder / ("frame-" + str(i) + ".png")).read_bytes() for i in [1, 2]]
    assert first == second, name
    assert second[:8] == b"\x89PNG\r\n\x1a\n", name
    assert list(struct.unpack(">II", second[16:24])) == result["dimensions"], name
    assert result["provenance"]["backend"] == "Metal", name
    assert result["provenance"]["text_engine"] == "parley", name
    metadata = json.loads((folder / "frame-2.json").read_text())
    assert metadata["render_provenance"] == result["provenance"], name
    record = next(r for r in inspection if r["state"] == name)
    assert record["sha256"] == sha(second) and record["inspected_with"] == "cognition_image_view", name

print(json.dumps({"current_source_files": source_count, "correction_evidence_files": evidence_count,
                  "historical_source_files": historical_source_count,
                  "historical_evidence_files": historical_evidence_count,
                  "preserved_original_and_review_files": len(original_paths),
                  "binary_sha256": binary["sha256"], "native_states": states,
                  "note": "Current bindings are separate from historical manifests. Pixel inspection is a recorded human-facing model assessment, not inferred from hashes. Physical input/platform checks are not certified."}, indent=2))
