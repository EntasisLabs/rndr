"""Reviewer audit of fresh raw logs and exact candidate, separate from worker auditor."""
import ast
import csv
import hashlib
import pathlib
import re
import subprocess

here = pathlib.Path(__file__).parent / 'qualified'
candidate = '2abc7d32c9029587ac74bea599428545efa8a1e6'
parent = '896ed5e4c294caa83550257226541cda245825cf'
f3 = 'f16e5572d35450d586dd4f10e00591fa36fb2b04'
old_review = 'acaa7ede97f01791f19fe4629ad3e3157011aee4'
def git(*args):
    return subprocess.check_output(['git', *args])
assert git('rev-parse', candidate+'^').decode().strip() == parent
assert git('rev-parse', 'main').decode().strip() == f3
assert candidate in (here/'environment.txt').read_text()
assert parent in (here/'environment.txt').read_text()
assert not git('diff', candidate, '--', 'crates', 'Cargo.toml', 'Cargo.lock')
assert not git('ls-files', '--others', '--exclude-standard', '--', 'crates')
assert not git('diff', f3, '--', 'crates/rndr_core', 'crates/rndr/src', 'crates/rndr/Cargo.toml', 'crates/rndr_liquid_ui/src', 'docs/design/rndr-flow-f0-contract.md', 'docs/design/rndr-flow-epic.md', 'crates/rndr_flow/src/budget_tests.rs', 'crates/rndr_flow/tests/reviewer_probes.rs')
assert not git('diff', old_review, '--', 'docs/reports/rndr-flow-f4-evidence', 'docs/reports/rndr-flow-f4-independent-evidence', 'docs/reports/rndr-flow-f4-independent-review.md')
print('Exact candidate', candidate, 'parent', parent, '; main remains accepted F3')
print('Qualified source, F0/epic, F3 budget proofs and historical FAIL/evidence unchanged')
rows = list(csv.DictReader((here/'summary.tsv').open(), delimiter='\t'))
assert len(rows) == 22 and len({r['gate'] for r in rows}) == 22
for r in rows:
    assert r['exit'] == '0', r
    assert min(int(r['free_kib_before']), int(r['free_kib_after'])) >= 4456448
    assert '\nexit=0\n' in (here/(r['gate']+'.txt')).read_text()
print('22 unique green gates; every recorded before/after free-space reading >4.25 GiB')
seen = set()
for line in (here/'source-digests.txt').read_text().splitlines():
    m = re.fullmatch(r'([a-f0-9]{64})  (.+)', line)
    if not m:
        continue
    digest, name = m.groups()
    assert name not in seen
    seen.add(name)
    current = pathlib.Path(name).read_bytes()
    assert current == git('show', f'{candidate}:{name}')
    assert hashlib.sha256(current).hexdigest() == digest
expected = {str(p) for p in pathlib.Path('crates/rndr_flow').rglob('*.rs')}
expected |= {'crates/rndr_flow/Cargo.toml','Cargo.lock','crates/rndr_liquid_ui/Cargo.toml','crates/rndr_liquid_ui/tests/flow_owned.rs'}
assert seen == expected and len(seen) == 25
print('All 25 fresh fingerprints match current bytes and exact candidate blobs')
flow = (here/'flow.txt').read_text()
ui = (here/'consumer.txt').read_text()
seen = set()
for s,c,n,a in re.findall(r'F4 seed=(\d+) cycle=(\d+) charged=0 handles=0 retired_unack=0 scopes=(\d+) cleanups=0 queued=0 acknowledged=(\d+)', flow):
    s,c,n,a = map(int,(s,c,n,a))
    assert (s,c) not in seen and n == 2 and a == 2*c
    seen.add((s,c))
assert seen == {(s,c) for s in (1,7,42) for c in range(1,121)}
print('360 unique F4 cycles / 720 cumulative actual-exit acknowledgments; full Counts equality is a compiled-test assertion, not raw-vector logging')
seen = set()
baselines = {}
for s,c,v,b,a in re.findall(r'F1/F2 teardown seed=(\d+) cycle=(\d+) vector=(\[[^\]]*\]) baseline=(\[[^\]]*\]) external_ack_pending=(\d+)', ui):
    s,c = int(s),int(c)
    v,b = ast.literal_eval(v),ast.literal_eval(b)
    assert (s,c) not in seen and len(v) == len(b) == 19 and v == b and int(a) == 0
    assert b == baselines.setdefault(s,b), (s,c,b)
    assert all(n == 0 for i,n in enumerate(b) if i != 12) and b[12] == 1
    seen.add((s,c))
assert seen == {(s,c) for s in (1,7,42) for c in range(100)}
print('300 unique F1/F2 rows: complete 19-counter vectors, fixed per-World baselines (global listener=1, all others=0), external ack=0 every row')
assert len(re.findall(r'^test budget_tests::\S+ \.\.\. ok$', flow, re.M)) == 9
assert set(re.findall(r'terminal-order kind=(\d+) cleanup_first=(true|false) panic_retained=true actual_exit=true independent_progress=true', flow)) == {(str(k),o) for k in range(10) for o in ('true','false')}
assert set(re.findall(r'timer-order mode=(\d+) panic=(true|false) no_early_ack=true receipt_preserved=true independent_progress=true', flow)) == {(str(k),o) for k in range(6) for o in ('true','false')}
print('Nine unchanged F3 budget tests green; all 20 producer / 12 timer case coordinates present')
for name, expected in [('flow',(78,0)),('core',(831,7)),('liquid',(16,0)),('consumer',(115,2)),('compatibility-off',(2,0)),('compatibility-liquid',(2,0))]:
    text = (here/(name+'.txt')).read_text()
    result = re.findall(r'test result: ok\. (\d+) passed; 0 failed; (\d+) ignored',text)
    assert 'test result: FAILED' not in text and result
    actual = tuple(sum(int(r[i]) for r in result) for i in range(2))
    assert actual == expected, (name,actual)
    print(name, 'passed/ignored', actual)

assert git('rev-parse', parent+'^').decode().strip() == '5b648bb723263064f1096bfc2aeeb31f14813d4f'
# Check all candidate bytes, not merely protected production paths.
paths = git('ls-tree', '-r', '--name-only', candidate).decode().splitlines()
assert not git('diff', candidate, '--', *paths)
prior = git('ls-tree', '-r', '--name-only', '5b648bb', '--', 'docs/reports').decode().splitlines()
assert not git('diff', '5b648bb', '--', *prior)
assert all(p.startswith('docs/reports/') for p in git('diff-tree', '--no-commit-id', '--name-only', '-r', candidate).decode().splitlines())
production = git('diff', '5b648bb', parent, '--', 'crates/rndr_flow/src').decode()
assert production.count('\n+') == 6 # +++ file line plus five added lines
assert '+            let _ = catch_unwind(AssertUnwindSafe(|| drop(value)));' in production
print('Sole production delta is five added lines; candidate child evidence-only; ALL candidate and historical report bytes unchanged')
probes = (here/'reviewer-probes.txt').read_text()
assert len(re.findall(r'test result: ok\. 3 passed; 0 failed; 0 ignored', probes)) == 2
assert 'output_panic=true normalized=true' in probes and 'output_panic=false normalized=true' in probes
track_rows = re.findall(r'R3-track mode=(\d+) future_panic=(true|false) output_panic=(true|false) exactly_once=true actual_exit=true reclaimed=true', flow)
task_rows = re.findall(r'R3-task path=(\d+) mode=(\d+) future_panic=(true|false) output_panic=(true|false) exactly_once=true receipt_preserved=true actual_exit=true reclaimed=true', flow)
expected_track, expected_task = set(), set()
for mode in range(5):
    for future in ('false', 'true'):
        expected_track.add((str(mode),future,'false'))
        if mode == 0 and future == 'true':
            expected_track.add((str(mode),future,'true'))
        for path in range(4):
            expected_task.add((str(path),str(mode),future,'false'))
            if mode == 0 and (future == 'true' or path == 3):
                expected_task.add((str(path),str(mode),future,'true'))
assert len(track_rows) == len(set(track_rows)) == 11 and set(track_rows) == expected_track
assert len(task_rows) == len(set(task_rows)) == 45 and set(task_rows) == expected_task
print('Both byte-unchanged reviewer suites freshly green 3+3; all 56 unique R3 branch coordinates match independent enumeration')
for name in ('independent-second.txt', 'independent-repeat.txt'):
    text = (here.parent/name).read_text()
    assert 'test result: ok. 1 passed; 0 failed; 0 ignored' in text
    rows = re.findall(r'independent-paused startup_panic=(true|false) future_panic=(true|false) output_panic=(true|false) future_ack_false=true output_user_ack_true=true reserved_until_final_clone=true no_write=true exactly_once=true', text)
    expected = {(s,f,o) for s,f in [('false','true'),('true','false'),('true','true')] for o in ('false','true')}
    assert len(rows) == len(set(rows)) == 6 and set(rows) == expected
print('Independent paused-destructor probe: 6/6 exact coordinates pass twice, including output panic and nonpanic controls; user-exit/receipt distinction asserted, stale writer calls=0')
print('PASS for exact candidate; independent review deliverable only, NOT sender acceptance/approval/integration')
