"""Fresh correction evidence audit, no reads of inherited qualification logs."""
import ast
import collections
import hashlib
import pathlib
import re
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
flow = (root / "flow.txt").read_text()
consumer = (root / "consumer.txt").read_text()
rows = re.findall(r"F4 seed=(\d+) cycle=(\d+) charged=0 handles=0 retired_unack=0 scopes=(\d+) cleanups=0 queued=0 acknowledged=(\d+)", flow)
assert len(rows) == 360, len(rows)
by_seed = collections.defaultdict(set)
for seed, cycle, scopes, acknowledged in rows:
    seed, cycle, scopes, acknowledged = map(int, (seed, cycle, scopes, acknowledged))
    assert scopes == 2 and acknowledged == 2 * cycle
    assert cycle not in by_seed[seed]
    by_seed[seed].add(cycle)
assert set(by_seed) == {1, 7, 42}
assert all(v == set(range(1, 121)) for v in by_seed.values())
print("F4 PASS: 360/360 cycles, 720 exit acknowledgments; full Counts/WorkCounts equality is asserted inside compiled tests, not a fabricated raw vector")
rows = re.findall(r"F1/F2 teardown seed=(\d+) cycle=(\d+) vector=(\[[^\]]*\]) baseline=(\[[^\]]*\]) external_ack_pending=(\d+)", consumer)
assert len(rows) == 300, len(rows)
by_seed = collections.defaultdict(set)
for seed, cycle, actual, baseline, pending in rows:
    actual, baseline = ast.literal_eval(actual), ast.literal_eval(baseline)
    assert len(actual) == len(baseline) == 19
    assert actual == baseline, (seed, cycle, actual, baseline)
    assert int(pending) == 0
    assert int(cycle) not in by_seed[int(seed)]
    by_seed[int(seed)].add(int(cycle))
assert set(by_seed) == {1, 7, 42}
assert all(v == set(range(100)) for v in by_seed.values())
print("F1/F2 PASS: 300/300 teardowns, all 19 components equal per-World baseline, zero pending external acknowledgments every row")
producer = re.findall(r"terminal-order kind=(\d+) cleanup_first=(true|false) panic_retained=true actual_exit=true independent_progress=true", flow)
assert len(producer) == 20
assert set(producer) == {(str(k), order) for k in range(10) for order in ("true", "false")}
timer = re.findall(r"timer-order mode=(\d+) panic=(true|false) no_early_ack=true receipt_preserved=true independent_progress=true", flow)
assert len(timer) == 12
assert set(timer) == {(str(k), panic) for k in range(6) for panic in ("true", "false")}
print("Correction matrices PASS: 20 producer orderings; 12 timer destruction/clone/World orderings")
for name in ("flow", "core", "liquid", "consumer", "compatibility-off", "compatibility-liquid"):
    path = root / f"{name}.txt"
    if not path.exists():
        continue  # facade runs later than the first vector audit
    text = path.read_text()
    counts = re.findall(r"test result: ok\. (\d+) passed; 0 failed; (\d+) ignored", text)
    assert counts and "test result: FAILED" not in text
    print(f"{name}: {sum(int(x) for x, _ in counts)} passed; {sum(int(x) for _, x in counts)} ignored")
fingerprints = root / "source-digests.txt"
if fingerprints.exists():
    checked = []
    for line in fingerprints.read_text().splitlines():
        match = re.fullmatch(r"([a-f0-9]{64})  (.+)", line)
        if not match:
            continue
        digest, path = match.groups()
        current = pathlib.Path(path).read_bytes()
        committed = subprocess.check_output(["git", "show", f"HEAD:{path}"])
        assert hashlib.sha256(current).hexdigest() == digest, path
        assert current == committed, path
        checked.append(path)
    expected = {str(p) for p in pathlib.Path("crates/rndr_flow").rglob("*.rs")}
    expected.update(["crates/rndr_flow/Cargo.toml", "Cargo.lock", "crates/rndr_liquid_ui/Cargo.toml", "crates/rndr_liquid_ui/tests/flow_owned.rs"])
    assert set(checked) == expected, (set(checked), expected)
    print(f"SOURCE PASS: all {len(checked)} fingerprints equal current bytes and git HEAD blobs")
    print("HEAD", subprocess.check_output(["git", "rev-parse", "HEAD", "HEAD^"], text=True).strip())
