#!/usr/bin/env python3
"""Subprocess/real-pixel contract tests. Uses only stdlib; no native window.

Usage: python3 scripts/test-component-capture.py /absolute/path/to/rndr-capture
Only this test's temporary files are removed. Never touches Cargo caches.
"""
import copy
import json
import pathlib
import struct
import subprocess
import sys
import tempfile
import zlib


def png_rgba(path):
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    offset, compressed = 8, bytearray()
    while offset < len(data):
        size = struct.unpack(">I", data[offset:offset + 4])[0]
        kind = data[offset + 4:offset + 8]
        body = data[offset + 8:offset + 8 + size]
        if kind == b"IHDR":
            width, height, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", body)
            assert (depth, color, interlace) == (8, 6, 0)
        if kind == b"IDAT":
            compressed.extend(body)
        offset += size + 12
    raw = zlib.decompress(compressed)
    stride, previous, rgba = width * 4, bytearray(width * 4), bytearray()
    for y in range(height):
        start = y * (stride + 1)
        filt, row = raw[start], bytearray(raw[start + 1:start + stride + 1])
        for x in range(stride):
            a, b = (row[x - 4] if x >= 4 else 0), previous[x]
            c = previous[x - 4] if x >= 4 else 0
            if filt == 1:
                extra = a
            elif filt == 2:
                extra = b
            elif filt == 3:
                extra = (a + b) // 2
            elif filt == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                extra = a if pa <= pb and pa <= pc else b if pb <= pc else c
            else:
                assert filt == 0
                extra = 0
            row[x] = (row[x] + extra) & 255
        rgba.extend(row)
        previous = row
    return width, height, rgba


def main():
    binary = str(pathlib.Path(sys.argv[1]).resolve(strict=True))
    base = json.loads(pathlib.Path("examples/component-capture.json").read_text())
    count = 0
    with tempfile.TemporaryDirectory(prefix="rndr-capture-cli-") as directory:
        root = pathlib.Path(directory)

        def check(name, scene=base, target=("--ui-id", "42"), code=0, error=None, raw=None):
            nonlocal count
            source = root / f"{name}.scene.json"
            source.write_text(json.dumps(scene) if raw is None else raw)
            output = root / name
            result = subprocess.run([binary, "--scene", str(source), "--output", str(output), *target], capture_output=True, text=True, timeout=30)
            assert result.returncode == code, (name, result.returncode, result.stdout, result.stderr)
            if error:
                assert error in result.stderr, (name, result.stderr)
            assert "PRIVATE_SENTINEL" not in result.stdout + result.stderr
            png, metadata = pathlib.Path(str(output) + ".png"), pathlib.Path(str(output) + ".json")
            if code:
                assert not png.exists() and not metadata.exists(), name
            else:
                w, h, pixels = png_rgba(png)
                tree = json.loads(metadata.read_text())
                assert (w, h) == tuple(tree["pixel_rect"][2:])
                assert tree["protocol"] == "rndr-component-capture-1"
                assert tree["renderer"] == "headless_offscreen_viewport_composite"
                assert "PRIVATE_SENTINEL" not in metadata.read_text()
                if sys.platform != "win32":
                    assert png.stat().st_mode & 0o777 == 0o600
                    assert metadata.stat().st_mode & 0o777 == 0o600
            count += 1
            print(f"PASS {name}: exit={code}")
            return output

        good = check("ui-id")
        w, h, pixels = png_rgba(pathlib.Path(str(good) + ".png"))
        assert (w, h) == (128, 64)
        center = ((h // 2) * w + w // 2) * 4
        r, g, b, a = pixels[center:center + 4]
        assert g > 180 and r < 50 and 50 < b < 110 and a == 255, (r, g, b, a)
        check("entity", target=("--entity", "1:0"))
        check("parent-scope", target=("--ui-id", "42", "--parent", "0:0"))
        check("missing", target=("--ui-id", "999"), code=3, error="target_not_found")
        check("stale", target=("--entity", "1:1"), code=3, error="stale_identity")
        check("stale-parent", target=("--ui-id", "42", "--parent", "0:1"), code=3, error="stale_identity")
        check("invalid-target", target=("--entity", "NaN"), code=2)
        check("conflicting-targets", target=("--ui-id", "42", "--entity", "1:0"), code=2)
        check("unknown-arg", target=("--target", "42"), code=2)
        denied = copy.deepcopy(base)
        denied["public"] = False
        denied["nodes"][1]["label"] = "PRIVATE_SENTINEL"
        check("denied", scene=denied, code=4, error="sensitive_frame")
        del denied["public"]
        check("missing-attestation", scene=denied, code=4, error="sensitive_frame")
        denied["public"] = True
        denied["nodes"].append({"size": [1, 1], "hidden": True, "sensitive": True, "label": "PRIVATE_SENTINEL"})
        check("sensitive-elsewhere", scene=denied, code=4, error="sensitive_frame")
        ambiguous = copy.deepcopy(base)
        ambiguous["nodes"].append({"id": "42", "size": [8, 8]})
        check("ambiguous", scene=ambiguous, code=3, error="target_ambiguous")
        check("disambiguated", scene=ambiguous, target=("--ui-id", "42", "--parent", "0:0"))
        empty = copy.deepcopy(base)
        empty["nodes"][1]["size"] = [0, 32]
        check("zero-target", scene=empty, code=3, error="empty_target")
        empty["nodes"][1]["size"] = [64, 32]
        empty["nodes"][0]["hidden"] = True
        check("hidden-ancestor", scene=empty, code=3, error="empty_target")
        empty["nodes"][0]["hidden"] = False
        empty["nodes"][0]["padding"] = 100
        check("out-of-bounds", scene=empty, code=3, error="empty_target")
        invalid = copy.deepcopy(base)
        invalid["viewport"][2] = 0
        check("invalid-scale", scene=invalid, code=2)
        invalid["viewport"] = [5000, 5000, 1]
        check("size-cap", scene=invalid, code=3, error="capture_limit")
        check("malformed-json", raw="{", code=2)
        check("oversize-input", raw=" " * 65_537, code=2)
        invalid = copy.deepcopy(base)
        invalid["nodes"][1]["parent"] = 1
        check("cycle", scene=invalid, code=2)
        invalid = copy.deepcopy(base)
        invalid["ignored"] = True
        check("unknown-field", scene=invalid, code=2)
        invalid = copy.deepcopy(base)
        invalid["nodes"][1]["size"] = [-1, 32]
        check("negative-size", scene=invalid, code=2)
        # Existing output never overwritten; metadata collision rolls back only
        # this process's newly created PNG.
        collision = root / "collision"
        sentinel = pathlib.Path(str(collision) + ".json")
        sentinel.write_text("preserve existing output")
        source = root / "ui-id.scene.json"
        result = subprocess.run([binary, "--scene", str(source), "--output", str(collision), "--ui-id", "42"], capture_output=True, text=True, timeout=30)
        assert result.returncode == 1
        assert sentinel.read_text() == "preserve existing output"
        assert not pathlib.Path(str(collision) + ".png").exists()
        count += 1
        print("PASS output-collision: exit=1; existing metadata preserved; no orphan PNG")
        # Output paths: no implicit mkdir, extension replacement or overwrite.
        def output_case(prefix, expected):
            nonlocal count
            result = subprocess.run([binary, "--scene", str(source), "--output", str(prefix), "--ui-id", "42"], capture_output=True, text=True, timeout=30)
            assert result.returncode == expected, (prefix, result.stderr)
            assert "PRIVATE_SENTINEL" not in result.stdout + result.stderr
            count += 1
            print(f"PASS output-path-{prefix.name}: exit={expected}")

        output_case(root / "missing-parent" / "capture", 1)
        assert not (root / "missing-parent").exists()
        spaced = root / "spaces and dots.scene"
        output_case(spaced, 0)
        assert pathlib.Path(str(spaced) + ".png").exists()
        assert pathlib.Path(str(spaced) + ".json").exists()
        assert not spaced.exists()
        saved = pathlib.Path(str(spaced) + ".png").read_bytes()
        output_case(spaced, 1)
        assert pathlib.Path(str(spaced) + ".png").read_bytes() == saved
        blocked = root / "directory-collision"
        pathlib.Path(str(blocked) + ".json").mkdir()
        output_case(blocked, 1)
        assert not pathlib.Path(str(blocked) + ".png").exists()
        assert pathlib.Path(str(blocked) + ".json").is_dir()
        if sys.platform != "win32":
            # create_new rejects final-component symlinks, including dangling ones.
            for suffix in (".png", ".json"):
                for dangling in (False, True):
                    prefix = root / f"symlink-{suffix[1:]}-{dangling}"
                    destination = root / f"sentinel-{suffix[1:]}-{dangling}"
                    if not dangling:
                        destination.write_text("PRIVATE_SENTINEL")
                    link = pathlib.Path(str(prefix) + suffix)
                    link.symlink_to(destination)
                    output_case(prefix, 1)
                    assert link.is_symlink()
                    assert not destination.exists() if dangling else destination.read_text() == "PRIVATE_SENTINEL"
                    other = ".json" if suffix == ".png" else ".png"
                    assert not pathlib.Path(str(prefix) + other).exists()
        result = subprocess.run([binary, "--help"], capture_output=True, text=True, timeout=30)
        assert result.returncode == 0 and "--scene" in result.stdout
        count += 1
        print("PASS help: exit=0")
    print(f"{count} CLI subprocess checks passed; real PNG decoded and pixel-checked")


if __name__ == "__main__":
    main()
