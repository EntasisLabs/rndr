"""Reviewer audit of fresh raw logs and exact candidate, separate from worker auditor."""
import ast
import csv
import hashlib
import pathlib
import re
import subprocess

here = pathlib.Path(__file__).parent
candidate = '218e06c9998296a87f6393e182f03f9e40e97b92'
parent = 'bda61046daef65661d697bd90e4665d6827c9417'
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
assert len(rows) == 21 and len({r['gate'] for r in rows}) == 21
for r in rows:
    assert r['exit'] == '0', r
    assert min(int(r['free_kib_before']), int(r['free_kib_after'])) >= 4456448
    assert '\nexit=0\n' in (here/(r['gate']+'.txt')).read_text()
print('21 unique green gates; every recorded before/after free-space reading >4.25 GiB')
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
assert seen == expected and len(seen) == 23
print('All 23 fresh fingerprints match current bytes and exact candidate blobs')
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
for name, expected in [('flow',(73,0)),('core',(831,7)),('liquid',(16,0)),('consumer',(115,2)),('compatibility-off',(2,0)),('compatibility-liquid',(2,0))]:
    text = (here/(name+'.txt')).read_text()
    result = re.findall(r'test result: ok\. (\d+) passed; 0 failed; (\d+) ignored',text)
    assert 'test result: FAILED' not in text and result
    actual = tuple(sum(int(r[i]) for r in result) for i in range(2))
    assert actual == expected, (name,actual)
    print(name, 'passed/ignored', actual)
assert '3 passed; 0 failed' in (here/'original-probes.txt').read_text()
assert '2 passed; 1 failed' in (here/'adversarial.txt').read_text()
assert 'output_panic=true normalized=false' in (here/'adversarial.txt').read_text()
print('Original reviewer probes fresh 3/3 green. NEW adversarial 2 pass / 1 fail: candidate FAIL, not acceptance.')
