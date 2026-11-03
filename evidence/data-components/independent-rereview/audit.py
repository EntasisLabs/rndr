#!/usr/bin/env python3
"""Reproduce this review's integrity checks, not owner acceptance or OS/AT certification."""
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
DELIVERY = '1957e1ece7cd4a3777e069e4899873d14073bee3'
SOURCE = '17d16f1f7143a2cb4b23176702fe5ed7ae5e1d06'
PRIOR = 'b4eddfc9b22439209ab274f237fb3f1bce3ecedb'
STATES = ['light', 'dark', 'narrow', 'tree-focus', 'disabled', 'empty', 'filtered', 'compact-hc-200', 'large']
def sha(data): return hashlib.sha256(data).hexdigest()
def git(*args): return subprocess.check_output(['git', *args], cwd=ROOT)

def preserved(revision):
    paths = git('ls-tree', '-r', '--name-only', revision, '--', 'evidence/data-components').decode().splitlines()
    for path in paths:
        assert (ROOT / path).read_bytes() == git('show', revision + ':' + path), path
    return len(paths)

original_count = preserved(PRIOR)
delivery_evidence_count = preserved(DELIVERY)
assert original_count == 264
subprocess.run(['git', 'merge-base', '--is-ancestor', DELIVERY, 'HEAD'], cwd=ROOT, check=True)
# All tracked material outside this new evidence folder is unchanged, not just selected source files.
changes = git('diff', '--name-only', DELIVERY, '--', '.').decode().splitlines()
assert all(p.startswith('evidence/data-components/independent-rereview/') for p in changes), changes
source_paths = []
for line in (HERE / 'source.sha256').read_text().splitlines():
    digest, path = line.split('  ', 1)
    assert sha((ROOT / path).read_bytes()) == digest, path
    assert sha(git('show', SOURCE + ':' + path)) == digest, path
    source_paths.append(path)
assert len(source_paths) == 16
fixture = (HERE.parent / 'independent-review/adversarial.rs').read_text()
normalized = subprocess.check_output(['rustfmt', '--edition', '2021', '--emit', 'stdout'], input=fixture, text=True)
assert (ROOT / 'crates/rndr_ui/tests/collection_chronology.rs').read_text().startswith(normalized.rstrip() + '\n')
counts = {}
for log, expected in [('core-paint-ui',570),('collection-metrics',17),('native-observe',73),('fallback-renderer',25),('facade',1),('facade-fallback',1)]:
    text = (HERE / 'verification' / (log + '.log')).read_text()
    assert 'EXIT=0' in text
    counts[log] = sum(map(int,re.findall(r'test result: ok\. (\d+) passed',text)))
    assert counts[log] == expected, (log, counts[log])
for log in ['formatting','python','gallery-build','gallery-fallback-check']:
    assert 'EXIT=0' in (HERE / 'verification' / (log + '.log')).read_text(), log
assert re.search(r'Ran 20 tests', (HERE / 'verification/python.log').read_text())
assert '12 passed; 0 failed' in (HERE / 'verification/core-paint-ui.log').read_text()
assert '6 passed; 0 failed' in (HERE / 'adversarial-six.log').read_text()
assert '2 passed; 0 failed' in (HERE / 'activation-policy.log').read_text()

binary = json.loads((HERE / 'binary.json').read_text())
correction_binary = json.loads((HERE.parent / 'corrections/binary.json').read_text())
assert binary['source_commit'] == DELIVERY
assert binary['sha256'] == correction_binary['sha256']
assert sha(Path(binary['path']).read_bytes()) == binary['sha256']
inspection = json.loads((HERE / 'screenshot-inspection.json').read_text())
images = []
for state in STATES:
    folder = HERE / ('native-' + state)
    result = json.loads((folder / 'result.json').read_text())
    assert result['status'] == 'captured' and result['exit_code'] == 0, state
    assert all(result[k] for k in ['stale_rejection','invalid_auth_rejection','normal_shutdown_cleanup'])
    assert result['binary_sha256'] == binary['sha256']
    assert result['provenance']['backend'] == 'Metal'
    assert result['provenance']['text_engine'] == 'parley'
    first, second = [(folder / ('frame-' + str(i) + '.png')).read_bytes() for i in [1,2]]
    assert first == second, state
    assert second == (HERE.parent / 'corrections' / ('native-' + state) / 'frame-2.png').read_bytes(), state
    assert list(struct.unpack('>II', second[16:24])) == result['dimensions']
    metadata = json.loads((folder / 'frame-2.json').read_text())
    assert metadata['render_provenance'] == result['provenance']
    observed = next(r for r in inspection if r['state'] == state)
    assert observed['file_sha256'] == sha(second)
    assert observed['inspected_with'] == 'cognition_image_view'
    images.append({'state':state,'file_sha256':sha(second),'byte_identical_to_corrected_delivery':True})
print(json.dumps({'reviewed_delivery':DELIVERY,'correction_source':SOURCE,'preserved_original_files':original_count,
    'preserved_delivery_evidence_files':delivery_evidence_count,'bound_source_files':len(source_paths),
    'verification_counts':counts,'unchanged_original_cases_passed':6,'maintained_chronology_cases_passed':12,
    'additional_reentrancy_cases_passed':2,'binary_sha256':binary['sha256'],'native_images':images,
    'limits':'Independent scoped review only; no owner approval/integration, physical input, AT/IME or Windows/Linux certification.'}, indent=2))
