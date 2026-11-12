"""R3 audit of new executions; inherited receipts are never counted as fresh."""
import ast
import csv
import pathlib
import re
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
subprocess.run([sys.executable, 'docs/reports/rndr-flow-f4-correction-audit.py', str(root)], check=True)
flow = (root / 'flow.txt').read_text()
ui = (root / 'consumer.txt').read_text()
# Require every coordinate once, not just matching scalar totals.
track = re.findall(r'R3-track mode=(\d+) future_panic=(true|false) output_panic=(true|false) exactly_once=true actual_exit=true reclaimed=true', flow)
task = re.findall(r'R3-task path=(\d+) mode=(\d+) future_panic=(true|false) output_panic=(true|false) exactly_once=true receipt_preserved=true actual_exit=true reclaimed=true', flow)
expected_track = {(str(m), f, o) for m in range(5) for f in ('false', 'true') for o in ('false', 'true') if o == 'false' or (m == 0 and f == 'true')}
expected_task = {(str(p), str(m), f, o) for p in range(4) for m in range(5) for f in ('false', 'true') for o in ('false', 'true') if o == 'false' or (m == 0 and (f == 'true' or p == 3))}
assert len(track) == len(set(track)) == 11 and set(track) == expected_track
assert len(task) == len(set(task)) == 45 and set(task) == expected_task
assert 'output_panic=true normalized=true' in flow
assert 'output_panic=false normalized=true' in flow
print('R3 PASS: 11 tracked future branches + 45 local/background/custom task cases; promoted channel-controlled panic/control and native-identity timer probes all pass')
assert len(re.findall(r'^test budget_tests::\S+ \.\.\. ok$', flow, re.M)) == 9
baseline_by_seed = {}
for seed, actual, baseline, pending in re.findall(r'F1/F2 teardown seed=(\d+) cycle=\d+ vector=(\[[^\]]*\]) baseline=(\[[^\]]*\]) external_ack_pending=(\d+)', ui):
    actual, baseline = ast.literal_eval(actual), ast.literal_eval(baseline)
    assert actual == baseline == baseline_by_seed.setdefault(seed, baseline)
    assert len(baseline) == 19 and int(pending) == 0
    assert baseline[12] == 1 and all(n == 0 for i, n in enumerate(baseline) if i != 12)
print('F3 PASS: nine unchanged budget tests; F1/F2 full per-World baselines stable, global listener=1, all other counters=0')

def git(*args):
    return subprocess.check_output(['git', *args])

base = '5b648bb723263064f1096bfc2aeeb31f14813d4f'
f3 = 'f16e5572d35450d586dd4f10e00591fa36fb2b04'
assert git('rev-parse', 'main').decode().strip() == f3
# Preserve ALL prior evidence and reports/runners/auditors, not only first FAIL.
old_reports = git('ls-tree', '-r', '--name-only', base, '--', 'docs/reports').decode().splitlines()
assert not git('diff', base, '--', *old_reports)
assert not git('diff', f3, '--', 'docs/design/rndr-flow-f0-contract.md', 'docs/design/rndr-flow-epic.md', 'crates/rndr_flow/src/budget_tests.rs', 'crates/rndr_flow/tests/reviewer_probes.rs')
assert not git('diff', base, '--', 'crates/rndr_core', 'crates/rndr/src', 'crates/rndr/Cargo.toml', 'crates/rndr_liquid_ui', 'Cargo.toml', 'Cargo.lock', 'crates/rndr_flow/Cargo.toml')
assert not git('ls-files', '--others', '--exclude-standard', '--', 'crates')
print('BOUNDARY PASS: main=accepted F3; F0/epic/F3 budget sources and ALL historical reports/evidence unchanged; protected production/manifests/lockfile unchanged; no disposable probes remain')
for name, expected in [('flow', (78, 0)), ('core', (831, 7)), ('liquid', (16, 0)), ('consumer', (115, 2)), ('compatibility-off', (2, 0)), ('compatibility-liquid', (2, 0))]:
    path = root / (name + '.txt')
    if not path.exists():
        continue  # first matrix audit precedes compatibility execution
    text = path.read_text()
    rows = re.findall(r'test result: ok\. (\d+) passed; 0 failed; (\d+) ignored', text)
    assert rows and 'test result: FAILED' not in text
    actual = tuple(sum(int(r[i]) for r in rows) for i in range(2))
    assert actual == expected, (name, actual)
    print(name, 'passed/ignored', actual)
probe = (root / 'reviewer-probes.txt').read_text()
assert len(re.findall(r'test result: ok\. 3 passed; 0 failed; 0 ignored', probe)) == 2
assert 'output_panic=true normalized=true' in probe
assert 'test result: FAILED' not in probe
print('REVIEWER PROBES PASS: both unchanged saved suites freshly executed, 3+3=6 passed')
summary = list(csv.DictReader((root / 'summary.tsv').open(), delimiter='\t'))
assert len({r['gate'] for r in summary}) == len(summary)
for row in summary:
    assert row['exit'] == '0', row
    assert min(int(row['free_kib_before']), int(row['free_kib_after'])) >= 4456448
print('All', len(summary), 'completed gates zero exit; recorded disk readings above 4.25 GiB')
if (root / 'source-digests.txt').exists():
    assert len(re.findall(r'^[a-f0-9]{64}  ', (root / 'source-digests.txt').read_text(), re.M)) == 25
    # The shared auditor above checks exact HEAD blobs and bytes for every path.
    assert len(summary) >= 21
    print('R3 source audit PASS: 25 refreshed fingerprints; final audit may precede its own summary row')
