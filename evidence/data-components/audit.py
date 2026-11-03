#!/usr/bin/env python3
"""Verify this local evidence bundle, not independent acceptance or OS input."""
import hashlib
import json
from pathlib import Path
import struct

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def check_manifest(name):
    count = 0
    for line in (HERE / name).read_text().splitlines():
        digest, relative = line.split("  ", 1)
        path = (ROOT / relative).resolve()
        assert path.is_relative_to(ROOT), relative
        assert path.is_file(), relative
        assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, relative
        count += 1
    return count


source_files = check_manifest("source.sha256")
evidence_files = check_manifest("MANIFEST.sha256")
states = []
for name in ["light", "dark", "narrow", "tree-focus", "disabled", "empty", "filtered", "compact-hc-200", "large"]:
    folder = HERE / ("final-" + name)
    result = json.loads((folder / "result.json").read_text())
    assert result["status"] == "captured" and result["exit_code"] == 0, name
    assert all(result[k] for k in ["stale_rejection", "invalid_auth_rejection", "normal_shutdown_cleanup"]), name
    first, second = [(folder / ("frame-" + str(i) + ".png")).read_bytes() for i in [1, 2]]
    assert first == second, name + ": idle pixels differ"
    assert second[:8] == b"\x89PNG\r\n\x1a\n", name
    assert list(struct.unpack(">II", second[16:24])) == result["dimensions"], name
    assert result["provenance"]["text_engine"] == "parley", name
    assert result["provenance"]["backend"] == "Metal", name
    metadata = json.loads((folder / "frame-2.json").read_text())
    assert metadata["render_provenance"] == result["provenance"], name
    states.append(name)
print(json.dumps({"source_files_verified": source_files, "evidence_files_verified": evidence_files,
                  "native_states_verified": states, "idle_png_identity": True,
                  "auth_stale_rejection_and_cleanup": True,
                  "note": "Pixel inspection is recorded separately in REPORT.md; hashes alone are not aesthetic or platform certification."}, indent=2))
