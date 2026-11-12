"""Read-only correction provenance/receipt audit. Never generate manifests."""
from pathlib import Path
import csv
import hashlib
import re
import subprocess

SOURCE = 'e228c824618aec25b7f8657d1569472925486b24'
PARENT = 'e89d0bff32ea94941063af11c496162b5fe5a261'
BASE = '43a0e83394ac80a9ac35bcffde479bef4d71cfd1'
OUT = Path('docs/reports/rndr-pr-149-ci-correction')
CHANGED = {
    'crates/rndr_flow/src/lib.rs',
    'crates/rndr_flow/src/topology.rs',
    'crates/rndr_liquid_ui/src/lifecycle.rs',
}
TOOLS = {'docs/reports/rndr-pr-149-ci-audit.py', 'docs/reports/rndr-pr-149-ci-checks.sh'}
def git(*args):
    return subprocess.check_output(['git', *args])
def sha(data):
    return hashlib.sha256(data).hexdigest()

assert not git('diff', SOURCE, '--', ':!docs'), 'current non-doc source differs'
assert not git('ls-files', '--others', '--exclude-standard', '--', 'crates')
assert set(git('diff', '--name-only', PARENT, SOURCE).decode().splitlines()) == CHANGED | TOOLS
for path in CHANGED:
    original = git('show', f'{PARENT}:{path}')
    corrected = git('show', f'{SOURCE}:{path}')
    assert original.count(b'.fetch_update(') == (2 if path.endswith('lifecycle.rs') else 1)
    assert corrected == original.replace(b'.fetch_update(', b'.try_update('), path
# Every previously tracked report/review/receipt remains frozen except the two
# explicitly authorized mutable, newly introduced repair tools.
old = set(git('ls-tree', '-r', '--name-only', PARENT, '--', 'docs/reports').decode().splitlines()) - TOOLS
assert not git('diff', PARENT, '--', *sorted(old))
manifest = OUT / 'source-digests.txt'
seen = set()
changed_baseline = set()
for line in manifest.read_text().splitlines():
    digest, path = line.split('  ', 1)
    assert path not in seen
    seen.add(path)
    data = git('show', f'{SOURCE}:{path}')
    assert sha(data) == digest and Path(path).read_bytes() == data, path
    if data != git('show', f'{BASE}:{path}'):
        changed_baseline.add(path)
expected = {p.decode() for p in git('ls-files', 'crates', 'Cargo.toml', 'Cargo.lock').splitlines() if p.endswith((b'.rs', b'.toml')) or p == b'Cargo.lock'}
assert seen == expected and len(seen) == 589
assert changed_baseline == CHANGED | {
    'crates/rndr_core/src/freeze.rs',
    'crates/rndr_core/src/structured/text_ops.rs',
    'crates/rndr_core/tests/structured_composer.rs',
}
print('SOURCE PASS:', SOURCE, '589 exact fingerprints; 583 unchanged / six changed from original baseline.')
print('DIFF PASS: four API-only renames in three production files; all ordering/closure/result bytes unchanged.')
print('HISTORY PASS: all previously tracked report/review/manifest bytes unchanged except two authorized repair tools.')
# Search all tracked Rust source, not just core or the known crates. Historical
# source snapshots under evidence/docs are not workspace compilation units.
paths = git('ls-files', '*.rs').decode().splitlines()
workspace = [p for p in paths if not p.startswith(('docs/', 'evidence/'))]
assert all('fetch_update' not in Path(p).read_text() for p in workspace)
print('WORKSPACE PASS: no fetch_update token in', len(workspace), 'tracked nonhistorical Rust files.')
q = OUT / 'checks'
rows = list(csv.DictReader((q / 'summary.tsv').open(), delimiter='\t'))
expected_gates = {'environment', 'fmt', 'clippy-workspace', 'clippy-no-default', 'clippy-dialog', 'clippy-wasm', 'clippy-ui', 'clippy-liquid-canvas', 'clippy-tokio', 'flow', 'consumer', 'core-freeze', 'composer', 'historical-audit'}
assert len(rows) == 14 and {r['gate'] for r in rows} == expected_gates
assert SOURCE in (q / 'environment.txt').read_text()
assert 'rustc 1.98.0' in (q / 'environment.txt').read_text()
for row in rows:
    assert row['exit'] == '0'
    assert min(int(row['free_kib_before']), int(row['free_kib_after'])) >= 3145728
    text = (q / (row['gate'] + '.txt')).read_text()
    assert text.endswith('\nexit=0\n')
    if row['gate'].startswith('clippy-'):
        assert '-D warnings' in text.splitlines()[0]
        assert 'Finished `dev` profile' in text and not re.search(r'^error:', text, re.M)
for gate, counts in {'flow': (144, 0), 'consumer': (115, 2), 'core-freeze': (3, 0), 'composer': (9, 0)}.items():
    text = (q / (gate + '.txt')).read_text()
    totals = re.findall(r'test result: ok\. (\d+) passed; 0 failed; (\d+) ignored', text)
    assert totals and 'test result: FAILED' not in text
    actual = tuple(sum(int(t[i]) for t in totals) for i in (0, 1))
    assert actual == counts, (gate, actual)
    print('TEST PASS:', gate, 'passed/ignored=', actual)
flow = (q / 'flow.txt').read_text()
consumer = (q / 'consumer.txt').read_text()
assert 'all_identity_counters_refuse_exhaustion_before_replacing_valid_work ... ok' in flow
assert 'complete_serial_range_reservation_cannot_consume_a_partial_prefix ... ok' in consumer
assert 'checked_serial_allocator_never_aliases_on_exhaustion ... ok' in consumer
for name in ('gate-regressions.txt', 'msrv-api.txt'):
    assert (OUT / name).is_file()
print('RECEIPTS PASS: 14 zero-exit monitored gates, all seven CI Clippy argument sets on local 1.98 only.')
print('LIMIT: no Rust 1.95/1.100 build, remote CI, full core/renderer/probe rerun or platform/GUI certification.')
