#!/usr/bin/env python3
"""Read-only correction audit. Run from checkout root; stdout is the receipt."""
import hashlib, json, pathlib, re, struct, subprocess
D = pathlib.Path('evidence/data-components-next/review-corrections')
BASE = '39b049feb32b50a7000bcb60ead62e16ee4fe8e2'
SOURCE = 'd9d9c3c998e67c8da332167cc7f179e4d373508b'
sha = lambda p: hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
git = lambda *args: subprocess.check_output(['git', *args], text=True).strip()
preserved = json.loads((D / 'preserved-evidence.json').read_text())
assert all(sha(p) == digest for p, digest in preserved.items()), 'Old evidence changed/missing'
tracked = {}
for line in git('ls-tree', '-r', BASE, '--', 'evidence').splitlines():
    fields, path = line.split('\t', 1)
    tracked[path] = fields.split()[2]
paths = list(tracked)
blobs = subprocess.check_output(['git', 'hash-object', '--stdin-paths'], input='\n'.join(paths)+'\n', text=True).splitlines()
assert all(tracked[p] == oid for p, oid in zip(paths, blobs)), 'Historical Git evidence changed'
assert len(paths) == len(blobs)
changed = git('diff', '--name-only', BASE, SOURCE).splitlines()
source_hashes = {p: sha(p) for p in changed}
for p in changed:
    expected = subprocess.check_output(['git', 'show', f'{SOURCE}:{p}'])
    assert pathlib.Path(p).read_bytes() == expected, ('Changed source no longer matches source commit', p)
def command_log(p):
    text = p.read_text()
    exits = re.findall(r'^EXIT=(\d+)$', text, re.M)
    assert exits and exits[-1] == '0', p
    return {'command': text.splitlines()[0], 'exit': int(exits[-1]),
            'passed_executions': sum(map(int, re.findall(r'test result: ok\. (\d+) passed', text))),
            'log_sha256': sha(p)}
commands = {p.name: command_log(p) for p in sorted((D/'verification').glob('*.log'))}
assert len(commands) == 10
focused = {p.name: command_log(p) for p in sorted((D/'focused-final').glob('*.log'))}
assert len(focused) == 3
binary_paths = set()
for directory in ['verification', 'focused-final']:
    for log in (D/directory).glob('*.log'):
        binary_paths.update(re.findall(r'^\s*Running .*\((/.*)\)\s*$', log.read_text(), re.M))
test_binaries = {p: sha(p) for p in sorted(binary_paths)}
native = D/'native'
receipt = json.loads((native/'receipt.json').read_text())
assert receipt['head_before_candidate_commit'] == SOURCE
assert sha(receipt['binary']) == receipt['binary_sha256']
assert sha(native/'source.sha256.json') == receipt['source_manifest_sha256']
manifest = json.loads((native/'source.sha256.json').read_text())
assert all(sha(p) == v for p, v in manifest.items())
assert len(receipt['runs']) == 12
for run in receipt['runs']:
    assert run['exit'] == 0 and run['presented_confirmation'] is False
    for n, frame in enumerate(run['frames'], 1):
        png = native/run['name']/f'frame-{n}.png'
        metadata = native/run['name']/f'frame-{n}.json'
        assert sha(png) == frame['png_sha256'] and sha(metadata) == frame['metadata_sha256']
        m = json.loads(metadata.read_text())
        assert m['source'] == 'live-surface-copy-before-present' and not m['presented_confirmation']
        assert list(struct.unpack('>II', png.read_bytes()[16:24])) == [m['width'], m['height']]
result = {'baseline': BASE, 'source_commit': SOURCE, 'source_sha256': source_hashes,
          'preserved_all_evidence_files': len(preserved),
          'preserved_all_evidence_bytes': sum(pathlib.Path(p).stat().st_size for p in preserved),
          'preserved_tracked_evidence_files': len(tracked),
          'preserved_tracked_evidence_bytes': sum(pathlib.Path(p).stat().st_size for p in tracked),
          'preserved_untracked_evidence_files': len(set(preserved)-set(tracked)),
          'commands': commands, 'gate_passed_executions': sum(v['passed_executions'] for v in commands.values()),
          'focused': focused, 'focused_passed_executions': sum(v['passed_executions'] for v in focused.values()),
          'test_binaries_sha256': test_binaries,
          'native_receipt_sha256': sha(native/'receipt.json'),
          'native_source_file_count': len(manifest), 'native_states': 12, 'native_frames': 24,
          'native_binary': receipt['binary'], 'native_binary_sha256': receipt['binary_sha256'],
          'pixel_inspection': 'NOT PERFORMED: image-view returned metadata only, not inspectable pixels',
          'independent_visual_signoff': 'OUTSTANDING for follow-on owner re-review'}
print(json.dumps(result, indent=2))
