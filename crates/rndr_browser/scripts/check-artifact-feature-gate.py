#!/usr/bin/env python3
"""macOS release regression; run from workspace root. Requires Aqua + Xcode CLI."""
import pathlib
import hashlib
import shutil
import re
import subprocess
import struct
import sys

if sys.platform != "darwin":
    sys.exit("Mach-O gate regression requires macOS (not a skipped success)")
root = pathlib.Path(__file__).resolve().parents[3]
base = root / "crates/rndr_browser"
evidence = root / "target/artifact-evidence/feature-gate"
evidence.mkdir(parents=True, exist_ok=True)
# Inventory derives from actual selector references, including macro arguments.
# Scan the entire macOS backend, not just the isolated module.
selectors = set()
for source in (base / "src").glob("macos*.rs"):
    text = source.read_text()
    selectors.update(re.findall(r"sel!\(\s*(_\w+:?)\s*\)", text))
    selectors.update(re.findall(r"msg_send!\[[^,]+,\s*(_\w+:?)", text))
    selectors.update(re.findall(r"#\[method\((_[\w:]+)\)\]", text))
    for setter, getter in re.findall(r"disable!\(\s*(_\w+)\s*,\s*(_\w+)", text):
        selectors.update((setter + ":", getter))
assert len(selectors) >= 13, selectors
(evidence / "selectors.txt").write_text("\n".join(sorted(selectors)) + "\n")
print("Private selector inventory:", *sorted(selectors), sep="\n", flush=True)
for label, features, enabled in [("off", [], False), ("harness-only-off", ["native-artifact-harness"], False), ("on", ["macos-private-webkit-artifacts"], True)]:
    command = ["cargo", "build", "-p", "rndr_browser", "--release", "--example", "artifact_build_probe"]
    if features:
        command += ["--features", ",".join(features)]
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=root, check=True)
    executable = root / "target/release/examples/artifact_build_probe"
    subprocess.run(["file", str(executable)], check=True)
    # Inspect ALL file-backed Mach-O sections, not the linker symbol table:
    # libc's imported `_close` symbol is unrelated to the WebKit `_close` selector.
    saved = evidence / f"{label}-artifact_build_probe"
    shutil.copy2(executable, saved)
    binary = saved.read_bytes()
    (evidence / f"{label}-sha256.txt").write_text(hashlib.sha256(binary).hexdigest() + "\n")
    assert struct.unpack_from("<I", binary)[0] == 0xfeedfacf, "expected thin 64-bit Mach-O"
    ncmds = struct.unpack_from("<I", binary, 16)[0]
    cursor = 32
    section_strings = set()
    for _ in range(ncmds):
        cmd, size = struct.unpack_from("<II", binary, cursor)
        if cmd == 0x19:  # LC_SEGMENT_64
            nsects = struct.unpack_from("<I", binary, cursor + 64)[0]
            for index in range(nsects):
                section = cursor + 72 + 80 * index
                length, offset = struct.unpack_from("<QI", binary, section + 40)
                flags = struct.unpack_from("<I", binary, section + 64)[0]
                if flags & 0xff not in (1, 0xc, 0x12):  # no file bytes for zero-fill
                    section_strings.update(binary[offset:offset + length].split(b"\0"))
        cursor += size
    present = {s for s in selectors if s.encode() in section_strings}
    if enabled:
        assert present == selectors, ("positive control absent", selectors - present)
    else:
        assert not present, ("private selectors leaked", present)
    with (evidence / f"{label}-objc-methname.txt").open("w") as log:
        subprocess.run(["otool", "-v", "-s", "__TEXT", "__objc_methname", str(executable)], stdout=log, check=True)
    result = subprocess.run([str(executable)], cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (evidence / f"{label}-run.log").write_text(result.stdout)
    print(result.stdout, flush=True)
    result.check_returncode()
    print(f"PASS feature-{label}: {len(present)}/{len(selectors)} private selectors", flush=True)
