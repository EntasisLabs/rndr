#!/usr/bin/env python3
"""Independent polish evidence checks. Run from repository root, no dependencies."""
import hashlib
import importlib.util
import pathlib
import re
import subprocess
import sys

sys.dont_write_bytecode = True
root = pathlib.Path("evidence/hands-on-polish")
review = root / "review"
for name, revision in [("baseline-source.sha256", "8ecd94c42036986a204330223514e5909d700702"),
                       ("after-source.sha256", "4ea2ac49d900724d5809b2d781c0108e220c87ba")]:
    for line in (root / name).read_text().splitlines():
        expected, path = line.split(None, 1)
        path = path.strip()
        if name.startswith("baseline") and path.endswith("/ui_polish.rs"):
            data = (root / "baseline-fixtures/ui_polish.rs").read_bytes()
        elif name.startswith("baseline") and path.endswith("/hands_on_polish.rs"):
            data = (root / "baseline-fixtures/hands_on_polish.rs").read_bytes()
        else:
            data = subprocess.check_output(["git", "show", revision + ":" + path])
        assert hashlib.sha256(data).hexdigest() == expected, (name, path)
    print(name, "matches exact Git objects / disclosed fixtures")
for name in [root / "artifact-sha256.txt", review / "reviewed-source.sha256"]:
    for line in name.read_text().splitlines():
        expected, path = line.split(None, 1)
        assert hashlib.sha256(pathlib.Path(path.strip()).read_bytes()).hexdigest() == expected, path
    print(name, "all checksums match")
spec = importlib.util.spec_from_file_location("png", "scripts/test-component-capture.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
for folder in ["baseline-captures", "captures"]:
    width, height, pixels = module.png_rgba(review / folder / "selection-light-normal-1x.png")
    mass = xmass = ymass = 0.
    for y in range(18):
        for x in range(18):
            pos = ((272 + y) * width + 38 + x) * 4
            ink = max(0., min(pixels[pos:pos+3]) - 180.)
            mass += ink
            xmass += x * ink
            ymass += y * ink
    assert mass > 0
    center = (xmass / mass, ymass / mass)
    print(folder, "actual decoded PNG ink centroid", center)
    if folder == "captures":
        assert abs(center[0]-8.5) < .15 and abs(center[1]-8.5) < .15
    files = list((review / folder).glob("*.png"))
    assert len(files) == 13
    for file in files:
        width, height, pixels = module.png_rgba(file)
        assert len(pixels) == width * height * 4
        assert file.with_suffix(".json").exists()
    print(folder, "13 decoded PNG / same-frame JSON pairs")
total = 0
for name, expected in [("core",415), ("paint",64), ("ui",212), ("render",96), ("gpu",2), ("privacy",1)]:
    text = (review / (name + ".log")).read_text()
    results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored", text)
    passed = sum(int(p) for p, _, _ in results)
    assert passed == expected and all(f == "0" and i == "0" for _, f, i in results), name
    total += passed
    print(name, passed, "passed, 0 failed/ignored")
assert total == 790
print("790 fresh passing executions in non-overlapping primary suite set")
