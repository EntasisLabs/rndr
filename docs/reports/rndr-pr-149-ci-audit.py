"""Read-only audit of original PR149 receipts at SOURCE, not later corrections.

Frozen F0-F5 artifacts and the original repair manifest are never rebased.
"""
import ast
import collections
import csv
import hashlib
import pathlib
import re
import subprocess

BASE = '43a0e83394ac80a9ac35bcffde479bef4d71cfd1'
SOURCE = '6698b63'
OUT = pathlib.Path('docs/reports/rndr-pr-149-ci-evidence')
Q = OUT / 'final-checks'
REPEAT = OUT / 'qualification'
ALLOWED = {
    'crates/rndr_core/src/freeze.rs',
    'crates/rndr_core/src/structured/text_ops.rs',
    'crates/rndr_core/tests/structured_composer.rs',
}
def git(*args):
    return subprocess.check_output(['git', *args])

def sha(data):
    return hashlib.sha256(data).hexdigest()

assert set(git('diff', '--name-only', BASE, SOURCE, '--', 'crates', 'Cargo.toml', 'Cargo.lock').decode().splitlines()) == ALLOWED
assert not git('ls-files', '--others', '--exclude-standard', '--', 'crates')
old = git('ls-tree', '-r', '--name-only', BASE, '--', 'docs/reports').decode().splitlines()
assert not git('diff', BASE, '--', *old)
manifest = pathlib.Path('docs/reports/rndr-flow-f5-independent-seam-review-evidence/qualification/source-digests.txt')
paths = set()
changed = set()
current = []
for line in manifest.read_text().splitlines():
    digest, path = line.split('  ', 1)
    assert path not in paths
    paths.add(path)
    assert sha(git('show', f'{BASE}:{path}')) == digest, path
    data = git('show', f'{SOURCE}:{path}')
    if sha(data) != digest:
        changed.add(path)
    current.append(f'{sha(data)}  {path}')
assert len(paths) == 589 and changed == ALLOWED
expected = {p.decode() for p in git('ls-files', 'crates', 'Cargo.toml', 'Cargo.lock').splitlines() if p.endswith((b'.rs', b'.toml')) or p == b'Cargo.lock'}
assert paths == expected
assert (OUT / 'source-digests.txt').read_text() == '\n'.join(current) + '\n', 'recorded manifest mismatch'
print('HISTORY PASS: all 589 frozen fingerprints match original baseline; all old reports byte-unchanged.')
print('ORIGINAL REPAIR SOURCE PASS (not current HEAD): 589 fingerprints; 586 unchanged, exactly three changed:')
for path in sorted(changed):
    print(path)
rows = list(csv.DictReader((Q / 'summary.tsv').open(), delimiter='\t'))
assert len(rows) == 16 and len({r['gate'] for r in rows}) == 16
assert all(r['exit'] == '0' and min(int(r['free_kib_before']), int(r['free_kib_after'])) >= 3145728 for r in rows)
for directory in (Q, REPEAT):
    for row in csv.DictReader((directory / 'summary.tsv').open(), delimiter='\t'):
        text = (directory / (row['gate'] + '.txt')).read_text()
        assert text.endswith('\nexit=' + row['exit'] + '\n'), row['gate']
        if row['gate'].startswith('clippy-'):
            assert row['exit'] == '0' and '-D warnings' in text.splitlines()[0]
            assert 'Finished `dev` profile' in text and not re.search(r'^error:', text, re.M)
for name, counts in {'core': (838, 7), 'flow': (144, 0), 'consumer': (115, 2), 'renderer-close': (60, 0), 'renderer-api': (1, 0), 'reviewer-probes': (8, 0), 'preserved-probes': (26, 0), 'independent-seam': (14, 0)}.items():
    directory = REPEAT if name == 'independent-seam' else Q
    text = (directory / (name + '.txt')).read_text()
    totals = re.findall(r'test result: ok\. (\d+) passed; 0 failed; (\d+) ignored', text)
    assert totals and 'test result: FAILED' not in text
    actual = tuple(sum(int(r[i]) for r in totals) for i in (0, 1))
    assert actual == counts, (name, actual)
    print(name, 'passed/ignored=', actual)
    for ignored in re.findall(r'^test (\S+) \.\.\. ignored(.*)$', text, re.M):
        print('IGNORED', name, *ignored)

def coordinates(rows, count, start=0):
    grouped = collections.defaultdict(set)
    for seed, cycle in rows:
        assert int(cycle) not in grouped[int(seed)]
        grouped[int(seed)].add(int(cycle))
    assert set(grouped) == {1, 7, 42}
    assert all(v == set(range(start, start + count)) for v in grouped.values())

consumer = (Q / 'consumer.txt').read_text()
rows = re.findall(r'F1/F2 teardown seed=(\d+) cycle=(\d+) vector=(\[[^\]]*\]) baseline=(\[[^\]]*\]) external_ack_pending=(\d+)', consumer)
assert len(rows) == 300
for seed, cycle, actual, baseline, pending in rows:
    actual, baseline = ast.literal_eval(actual), ast.literal_eval(baseline)
    assert actual == baseline and len(actual) == 19 and int(pending) == 0
    assert baseline[12] == 1 and all(x == 0 for i, x in enumerate(baseline) if i != 12)
coordinates([(s, c) for s, c, *_ in rows], 100)
flow = (Q / 'flow.txt').read_text()
rows = re.findall(r'F4 seed=(\d+) cycle=(\d+) charged=0 handles=0 retired_unack=0 scopes=(\d+) cleanups=0 queued=0 acknowledged=(\d+)', flow)
assert len(rows) == 360
assert all(int(scopes) == 2 and int(ack) == 2 * int(cycle) for _, cycle, scopes, ack in rows)
coordinates([(s, c) for s, c, *_ in rows], 120, 1)
rows = re.findall(r'F5 baseline seed=(\d+) cycle=(\d+) counts=(.*?) work=(.*?) history=(.*)', flow)
assert len(rows) == 360
coordinates([(s, c) for s, c, *_ in rows], 120)
baselines = {}
for seed, cycle, counts, work, history in rows:
    assert (counts, work, history) == baselines.setdefault(seed, (counts, work, history))
    assert 'charged: 0' in work and 'pending_deliveries: 0' in work
    assert 'entries: 0' in history and 'pending: 0' in history and 'queued: 0' in history and 'outcomes: 64' in history
assert set(re.findall(r'F5 windows seed=(\d+) cycles=120 exact_baseline=true shared_document_live=true exactly_once=120', flow)) == {'1', '7', '42'}
print('LIFECYCLE PASS: F1/F2 300 full 19-counter teardowns; F4 360 cycles/720 exit acknowledgments; F5 360 full-resource and 360 terminal-window cycles.')
repeat = list(csv.DictReader((REPEAT / 'summary.tsv').open(), delimiter='\t'))
assert len(repeat) == 17
blocked = [r for r in repeat if r['exit'] != '0']
assert len(blocked) == 1 and blocked[0]['gate'] == 'preserved-probes'
assert int(blocked[0]['free_kib_before']) < 4456448
assert all(min(int(r['free_kib_before']), int(r['free_kib_after'])) >= 3145728 for r in repeat)
assert (REPEAT / 'preserved-probes.txt').read_text().endswith('\nexit=1\n')
print('SELECTED RECEIPTS PASS: 16 zero-exit exact-source gates plus 14 later independent seam tests; all seven CI Clippy configurations pass.')
print('REPEAT LIMIT: aggregate driver correctly exits 1 because unchanged preserved-probe harness refuses its inherited 4.25-GiB disk floor. Those exact probes already passed in final-checks and preserved-postcommit; no floor or assertion was weakened.')
print('Disk: all new-driver monitored gates retained 3 GiB; inherited higher refusal remains intact.')
print('SOURCE', git('rev-parse', SOURCE).decode().strip())
