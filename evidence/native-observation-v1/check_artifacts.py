#!/usr/bin/env python3
"""Verify final native PNG payloads and their semantic envelopes, not OS display.
All inputs are known synthetic artifacts within this evidence directory.
"""
from pathlib import Path
import hashlib
import json
import re
import struct
import subprocess
import zlib

ROOT = Path(__file__).resolve().parent
BASE = "ff397cd9ef10678194068107043fcdbb3803d52a"


def png_rgba(png):
    assert png[:8] == b"\x89PNG\r\n\x1a\n"
    cursor = 8
    compressed = bytearray()
    width = height = None
    ended = False
    while cursor < len(png):
        length, = struct.unpack(">I", png[cursor:cursor+4])
        kind = png[cursor+4:cursor+8]
        data = png[cursor+8:cursor+8+length]
        crc, = struct.unpack(">I", png[cursor+8+length:cursor+12+length])
        assert zlib.crc32(kind + data) & 0xffffffff == crc
        if kind == b"IHDR":
            width, height, depth, color, compression, filtering, interlace = struct.unpack(">IIBBBBB", data)
            assert depth == 8 and color == 6 and compression == filtering == interlace == 0
            assert 0 < width <= 4096 and 0 < height <= 4096 and width * height <= 4_000_000
        elif kind == b"IDAT":
            compressed.extend(data)
        elif kind == b"IEND":
            ended = True
        cursor += length + 12
    assert ended and cursor == len(png)
    raw = zlib.decompress(compressed)
    stride = width * 4
    assert len(raw) == (stride + 1) * height
    previous = bytearray(stride)
    rgba = bytearray()
    for row in range(height):
        start = row * (stride + 1)
        method = raw[start]
        assert 0 <= method <= 4
        decoded = bytearray(raw[start+1:start+1+stride])
        for i in range(stride):
            a = decoded[i-4] if i >= 4 else 0
            b = previous[i]
            c = previous[i-4] if i >= 4 else 0
            if method == 0:
                delta = 0
            elif method == 1:
                delta = a
            elif method == 2:
                delta = b
            elif method == 3:
                delta = (a+b)//2
            else:
                p = a+b-c
                distances = [abs(p-a), abs(p-b), abs(p-c)]
                delta = [a,b,c][distances.index(min(distances))]
            decoded[i] = (decoded[i] + delta) & 255
        rgba.extend(decoded)
        previous = decoded
    return width, height, rgba


def main():
    # Pin to the implementation actually compiled/run, not a later evidence-only
    # child. This lets the sealed handoff be verified without rewriting history.
    tested = (ROOT/"final-environment.log").read_text().splitlines()[1]
    subprocess.run(["git", "merge-base", "--is-ancestor", tested, "HEAD"], check=True)
    result = {"tested_implementation": tested, "native_frames": [], "preservation": {}, "test_groups": []}
    for name in ["final-native-light", "final-native-narrow"]:
        run = json.loads((ROOT/name/"result.json").read_bytes())
        assert run["status"] == "captured" and run["normal_shutdown_cleanup"] and run["exit_code"] == 0
        assert run["invalid_auth_rejection"] and run["stale_rejection"]
        for index in [1, 2]:
            prefix = ROOT/name/f"frame-{index}"
            png = prefix.with_suffix(".png").read_bytes()
            metadata_bytes = prefix.with_suffix(".json").read_bytes()
            metadata = json.loads(metadata_bytes)
            width, height, rgba = png_rgba(png)
            assert width == metadata["width"] and height == metadata["height"]
            assert metadata["png_bytes"] == len(png)
            assert metadata["source"] == "live-surface-copy-before-present"
            assert metadata["presented_confirmation"] is False
            assert "error" not in metadata and "capability" not in metadata
            nodes = metadata["nodes"]
            ids = [n["id"] for n in nodes]
            assert len(ids) == len(set(ids)) and metadata["root"] in ids
            assert all(c in ids for n in nodes for c in n["children"])
            assert all(a in ["focus", "activate", "set_value"] for n in nodes for a in n["actions"])
            colors = len({bytes(rgba[i:i+4]) for i in range(0,len(rgba),4)})
            assert colors > 1, "blank/constant native image"
            result["native_frames"].append({"path": str(prefix.relative_to(ROOT)), "width": width, "height": height, "device_scale": metadata["device_scale"], "revision": metadata["frame_revision"], "nodes": len(nodes), "unique_rgba": colors, "png_sha256": hashlib.sha256(png).hexdigest(), "rgba_sha256": hashlib.sha256(rgba).hexdigest(), "metadata_sha256": hashlib.sha256(metadata_bytes).hexdigest()})
    # Accept an evidence-only child, but verify every preexisting historical
    # evidence blob remains identical. Source trees still match as whole trees.
    for path in ["crates/rndr_core", "crates/rndr_ui", "docs/design", "evidence"]:
        old = subprocess.check_output(["git", "rev-parse", BASE+":"+path], text=True).strip()
        current = subprocess.check_output(["git", "rev-parse", "HEAD:"+path], text=True).strip()
        if path == "evidence":
            def entries(revision):
                rows = subprocess.check_output(["git", "ls-tree", "-r", revision, "--", path], text=True).splitlines()
                return {row.split("\t", 1)[1]: row.split("\t", 1)[0] for row in rows}
            before, after = entries(BASE), entries("HEAD")
            assert all(after.get(name) == identity for name, identity in before.items())
            assert all(name in before or name.startswith("evidence/native-observation-v1/") for name in after)
        else:
            assert old == current
        result["preservation"][path] = old
    text = (ROOT/"final-suites.log").read_text()
    for passed, failed, ignored in re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored", text):
        result["test_groups"].append({"passed": int(passed), "failed": int(failed), "ignored": int(ignored)})
    result["cargo_checks_passed"] = sum(g["passed"] for g in result["test_groups"])
    result["cargo_failures"] = sum(g["failed"] for g in result["test_groups"])
    changed = subprocess.check_output(["git", "diff", "--name-only", BASE, tested], text=True).splitlines()
    for path in changed:
        expected = subprocess.check_output(["git", "rev-parse", tested+":"+path], text=True).strip()
        actual = subprocess.check_output(["git", "hash-object", path], text=True).strip()
        assert actual == expected, path
    result["source_inputs"] = [{"path": p, "sha256": hashlib.sha256(Path(p).read_bytes()).hexdigest(), "git_blob": subprocess.check_output(["git", "rev-parse", tested+":"+p], text=True).strip()} for p in changed]
    (ROOT/"artifact-integrity.json").write_text(json.dumps(result, indent=2)+"\n")
    print(f"PASS: {len(result['native_frames'])} native PNGs fully decoded/CRC checked, bound dimensions/revisions/trees; {result['cargo_checks_passed']} Cargo checks, {result['cargo_failures']} failures; unchanged historical trees; {len(changed)} exact source inputs.")


if __name__ == "__main__":
    main()
