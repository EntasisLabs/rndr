"""Audit the final RNDR native-capture matrix, without claiming OS presentation."""
import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parent
binary_hashes = set()
summary = {}
for name in ["light", "dark", "narrow", "200", "200-source", "200-editor"]:
    folder = root / ("complete-" + name)
    result = json.loads((folder / "result.json").read_text())
    assert result["status"] == "captured", (name, result)
    assert result["exit_code"] == 0 and result["normal_shutdown_cleanup"]
    assert result["stale_rejection"] and result["invalid_auth_rejection"]
    assert result["provenance"]["text_engine"] == "parley"
    assert result["provenance"]["parley_feature"] is True
    binary_hashes.add(result["binary_sha256"])
    expected_width = 1000 if name in ["light", "dark"] else 320
    assert result["dimensions"] == [expected_width, 700]
    frame = json.loads((folder / "frame-1.json").read_text())
    editor = next(n for n in frame["nodes"] if n["role"] == "TextInput")
    source = next(n for n in frame["nodes"] if n["role"] == "Button" and n["label"] == "Show source")
    for node in frame["nodes"]:
        if node["focusable"]:
            x, y, w, h = node["bounds"]
            assert x >= -0.1 and x + w <= expected_width + 0.1, (name, node)
    if name.endswith("-source") or name.endswith("-editor"):
        target = source if name.endswith("-source") else editor
        assert target["focused"], (name, target)
        x, y, w, h = target["bounds"]
        assert y >= -0.1 and y + h <= 700.1, (name, target)
    if name in ["light", "dark"]:
        assert editor["bounds"][3] == 76
        assert source["bounds"][2] < 200
    summary[name] = {
        "dimensions": result["dimensions"],
        "editor_bounds": editor["bounds"],
        "source_bounds": source["bounds"],
        "frame_1_sha256": hashlib.sha256((folder / "frame-1.png").read_bytes()).hexdigest(),
        "presented_confirmation": result["presented_confirmation"],
    }
assert len(binary_hashes) == 1
print(json.dumps({"binary_sha256": binary_hashes.pop(), "captures": summary}, indent=2))
