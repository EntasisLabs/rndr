"""F5 correction exact-source audit; inherited receipts never count as fresh.
Original auditor retained unchanged. Flow adds 27 permanent test executions
(13 distinct correction tests, 14 repeated unchanged runner tests), from 108 to 135.
All original vector/cycle/probe/source protections below remain unchanged.
"""
import ast
import collections
import csv
import hashlib
import pathlib
import re
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
def git(*args):
    return subprocess.check_output(['git', *args])
base = 'bf56c67fd5b67ddf993ec5c02e5af4ef7ef507d5'
flow = (root / 'flow.txt').read_text()
consumer = (root / 'consumer.txt').read_text()
def coordinates(rows, cycles, start=0):
    grouped = collections.defaultdict(set)
    for seed, cycle in rows:
        assert int(cycle) not in grouped[int(seed)]
        grouped[int(seed)].add(int(cycle))
    assert set(grouped) == {1, 7, 42}
    assert all(c == set(range(start, start + cycles)) for c in grouped.values())
rows = re.findall(r'F1/F2 teardown seed=(\d+) cycle=(\d+) vector=(\[[^\]]*\]) baseline=(\[[^\]]*\]) external_ack_pending=(\d+)', consumer)
assert len(rows) == 300
baselines = {}
for seed, cycle, actual, baseline, pending in rows:
    actual, baseline = ast.literal_eval(actual), ast.literal_eval(baseline)
    assert actual == baseline == baselines.setdefault(seed, baseline)
    assert len(actual) == 19 and int(pending) == 0
    assert baseline[12] == 1 and all(x == 0 for i, x in enumerate(baseline) if i != 12)
coordinates([(s, c) for s, c, *_ in rows], 100)
print('F1/F2 PASS: 300 fresh teardown cycles, all 19 counters equal per-World baselines; only global listener=1, all other counters=0; external pending=0')
rows = re.findall(r'F4 seed=(\d+) cycle=(\d+) charged=0 handles=0 retired_unack=0 scopes=(\d+) cleanups=0 queued=0 acknowledged=(\d+)', flow)
assert len(rows) == 360
for seed, cycle, scopes, ack in rows:
    assert int(scopes) == 2 and int(ack) == 2 * int(cycle)
coordinates([(s,c) for s,c,*_ in rows],120,1)
assert len(re.findall(r'^test budget_tests::\S+ \.\.\. ok$', flow, re.M)) == 9
print('F3/F4 PASS: all nine budget proofs; 360 lifecycle cycles/720 actual acknowledgments; compiled full Counts/WorkCounts equality preserved')
producer = re.findall(r'terminal-order kind=(\d+) cleanup_first=(true|false) panic_retained=true actual_exit=true independent_progress=true', flow)
timer = re.findall(r'timer-order mode=(\d+) panic=(true|false) no_early_ack=true receipt_preserved=true independent_progress=true', flow)
assert len(producer) == 20 and set(producer) == {(str(k),o) for k in range(10) for o in ('true','false')}
assert len(timer) == 12 and set(timer) == {(str(k),o) for k in range(6) for o in ('true','false')}
track = re.findall(r'R3-track mode=(\d+) future_panic=(true|false) output_panic=(true|false) exactly_once=true actual_exit=true reclaimed=true', flow)
task = re.findall(r'R3-task path=(\d+) mode=(\d+) future_panic=(true|false) output_panic=(true|false) exactly_once=true receipt_preserved=true actual_exit=true reclaimed=true', flow)
expected_track = {(str(m), f, o) for m in range(5) for f in ('false','true') for o in ('false','true') if o == 'false' or (m == 0 and f == 'true')}
expected_task = {(str(p),str(m),f,o) for p in range(4) for m in range(5) for f in ('false','true') for o in ('false','true') if o == 'false' or (m == 0 and (f == 'true' or p == 3))}
assert len(track) == len(set(track)) == 11 and set(track) == expected_track
assert len(task) == len(set(task)) == 45 and set(task) == expected_task
print('F4 panic matrices PASS: 20 producer + 12 timer + 11 tracked/45 task branch coordinates')
rows = re.findall(r'F5 baseline seed=(\d+) cycle=(\d+) counts=(.*?) work=(.*?) history=(.*)', flow)
assert len(rows) == 360
coordinates([(s,c) for s,c,*_ in rows],120)
baselines = {}
for seed, cycle, counts, work, history in rows:
    vector = (counts,work,history)
    assert vector == baselines.setdefault(seed,vector)
    assert 'charged: 0' in work and 'pending_deliveries: 0' in work
    assert 'entries: 0' in history and 'pending: 0' in history and 'queued: 0' in history and 'outcomes: 64' in history
assert set(re.findall(r'F5 windows seed=(\d+) cycles=120 exact_baseline=true shared_document_live=true exactly_once=120', flow)) == {'1','7','42'}
print('F5 PASS: 360 stable full Counts/WorkCounts/HistoryCounts rows after bounded warm-up; 360 terminal-window admission/reclamation cycles; compiled tests also assert final root/World teardown to zero')
probe = (root/'reviewer-probes.txt').read_text()
assert len(re.findall(r'test result: ok\. 3 passed; 0 failed; 0 ignored', probe)) == 2
assert len(re.findall(r'test result: ok\. 1 passed; 0 failed; 0 ignored', probe)) == 2
assert 'output_panic=true normalized=true' in probe and 'test result: FAILED' not in probe
print('Accepted F4 reviewer probes PASS: 3+3 plus six-case paused-destructor test twice, byte-unchanged saved sources')
for name in ('flow','host-adapter','core','liquid','consumer','compatibility-off','compatibility-liquid'):
    text=(root/(name+'.txt')).read_text()
    totals=re.findall(r'test result: ok\. (\d+) passed; 0 failed; (\d+) ignored',text)
    assert totals and 'test result: FAILED' not in text
    passed=sum(int(p) for p,_ in totals);ignored=sum(int(i) for _,i in totals)
    print(name,'passed=',passed,'ignored=',ignored)
    for test in re.findall(r'^test (\S+) \.\.\. ignored(.*)$',text,re.M):print('IGNORED',name,*test)
    if name in ('flow','host-adapter','liquid','consumer','core'):
        assert (passed,ignored)=={'flow':(135,0),'host-adapter':(14,0),'liquid':(16,0),'consumer':(115,2),'core':(834,7)}[name]
summary=list(csv.DictReader((root/'summary.tsv').open(),delimiter='\t'))
assert len(summary)>=21 and len({r['gate'] for r in summary})==len(summary)
assert all(r['exit']=='0' and min(int(r['free_kib_before']),int(r['free_kib_after']))>=4456448 for r in summary)
expected={p.decode() for p in git('ls-files','crates','Cargo.toml','Cargo.lock').splitlines() if p.endswith((b'.rs',b'.toml')) or p==b'Cargo.lock'}
checked=set()
for line in (root/'source-digests.txt').read_text().splitlines():
    match=re.fullmatch(r'([a-f0-9]{64})  (.+)',line)
    assert match,line
    digest,path=match.groups();assert path not in checked;checked.add(path)
    current=pathlib.Path(path).read_bytes();assert hashlib.sha256(current).hexdigest()==digest
    assert current==git('show',f'HEAD:{path}'),path
assert checked==expected,(checked^expected)
assert not git('diff',base,'--','Cargo.toml','Cargo.lock','crates/rndr_flow/Cargo.toml','crates/rndr_core/Cargo.toml','crates/rndr/src','crates/rndr/Cargo.toml','crates/rndr_liquid_ui','crates/rndr_render','docs/design/rndr-flow-epic.md','docs/design/rndr-flow-f0-contract.md','crates/rndr_flow/src/work.rs','crates/rndr_flow/src/request.rs','crates/rndr_flow/src/budget_tests.rs','crates/rndr_flow/tests/reviewer_probes.rs')
old_reports=git('ls-tree','-r','--name-only',base,'--','docs/reports').decode().splitlines()
assert not git('diff',base,'--',*old_reports)
assert not git('ls-files','--others','--exclude-standard','--','crates')
print('SOURCE PASS:',len(checked),'fingerprints equal bytes and exact HEAD blobs; F0/F1/F2/F3/F4 protected contracts, old evidence, manifests, runtime/SDK/render sources unchanged')
print('HEAD',git('rev-parse','HEAD','HEAD^').decode().strip())
print('Qualification PASS:',len(summary),'completed zero-exit gates (audit row is appended after this process returns); no native/GUI certification')
