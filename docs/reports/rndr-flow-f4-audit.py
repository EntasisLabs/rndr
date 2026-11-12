"""Audit committed F4 and unchanged F1/F2 repetition evidence, not just exits."""
import ast
import collections
import pathlib
import re

root = pathlib.Path("docs/reports/rndr-flow-f4-evidence")
flow = (root / "flow.txt").read_text()
consumer = (root / "consumer.txt").read_text()
rows = re.findall(r"F4 seed=(\d+) cycle=(\d+) charged=0 handles=0 retired_unack=0 scopes=(\d+) cleanups=0 queued=0 acknowledged=(\d+)", flow)
assert len(rows) == 360, len(rows)
by_seed = collections.defaultdict(set)
for seed, cycle, scopes, acknowledged in rows:
    seed, cycle, scopes, acknowledged = map(int, (seed, cycle, scopes, acknowledged))
    assert scopes == 2
    assert acknowledged == 2 * cycle
    assert cycle not in by_seed[seed]
    by_seed[seed].add(cycle)
assert set(by_seed) == {1, 7, 42}
assert all(v == set(range(1, 121)) for v in by_seed.values())
print("F4 PASS: 360/360 cycles; full Counts/WorkCounts assertions; 720 exit acknowledgments")
print("F4 receipt capacity and full pending-delivery equality are asserted in compiled tests, not inferred from quiet handles")

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
print("F1/F2 PASS: 300/300 teardowns; all 19 counters equal per-World baseline; external_ack_pending=0 each time")
for name in ("flow", "core", "liquid", "consumer"):
    text = (root / f"{name}.txt").read_text()
    counts = re.findall(r"test result: ok\. (\d+) passed; 0 failed; (\d+) ignored", text)
    assert counts and "test result: FAILED" not in text
    print(f"{name}: {sum(int(x) for x, _ in counts)} passed; {sum(int(x) for _, x in counts)} ignored across unit/integration/doctest summaries")
