"""Validate preserved seals and fresh correction artifacts; no capability reads.
Read-only except stdout. Uses the already-sealed PNG decoder without its main().
"""
from pathlib import Path
import hashlib
import json
import re
import runpy
import subprocess

ROOT = Path('evidence/native-observation-v1-correction')
BASE = '28a0ae307f869ef651d8ebcc5fdcd82a9dc31ce7'
sha = lambda data: hashlib.sha256(data).hexdigest()
for folder, expected in [('native-observation-v1', 111), ('native-observation-v1-review', 30)]:
    root = Path('evidence') / folder
    manifest = root / 'MANIFEST.sha256'
    rows = manifest.read_text().splitlines()
    assert len(rows) == expected
    for line in rows:
        digest, name = line.split(None, 1)
        name = name.lstrip('*')
        path = Path(name)
        if not path.exists():
            path = root / name
        assert sha(path.read_bytes()) == digest, path
    print('PRESERVED', expected, 'payloads', manifest, sha(manifest.read_bytes()))
assert not subprocess.check_output(['git', 'diff', BASE, '--',
    'evidence/native-observation-v1', 'evidence/native-observation-v1-review',
    'crates/rndr_ui', 'docs/design'])
print('PRESERVED prior native/review evidence, UI and design paths')

png_rgba = runpy.run_path('evidence/native-observation-v1/check_artifacts.py')['png_rgba']
for mode in ['light', 'narrow']:
    result = json.loads((ROOT / ('native-final-' + mode) / 'result.json').read_text())
    assert result['status'] == 'captured' and result['normal_shutdown_cleanup']
    assert result['exit_code'] == 0 and result['stale_rejection'] and result['invalid_auth_rejection']
    assert result['binary_sha256'] == 'ce544034520d1a8cc6d9af538ffa0f3502d0438303f84240e55e8813f3e44e09'
    for number in [1, 2]:
        prefix = ROOT / ('native-final-' + mode) / ('frame-' + str(number))
        png = prefix.with_suffix('.png').read_bytes()
        meta = json.loads(prefix.with_suffix('.json').read_bytes())
        width, height, rgba = png_rgba(png)
        assert (width, height) == (meta['width'], meta['height'])
        assert len(png) == meta['png_bytes']
        assert meta['source'] == 'live-surface-copy-before-present'
        assert meta['presented_confirmation'] is False
        assert meta['render_provenance']['text_engine'] == 'builtin_atlas'
        assert meta['render_provenance']['path'] == 'quads' and meta['device_scale'] == 1
        ids = {node['id'] for node in meta['nodes']}
        assert len(ids) == len(meta['nodes']) == 28 and meta['root'] in ids
        for node in meta['nodes']:
            assert set(node['children']) <= ids
            assert set(node['actions']) <= {'focus', 'activate', 'set_value'}
        old = Path('evidence/native-observation-v1-review') / ('native-' + mode) / ('frame-' + str(number) + '.png')
        assert png_rgba(old.read_bytes()) == (width, height, rgba)
        print('PNG CRC/unfilter/tree PASS', prefix, width, height, 'raw_sha256', sha(rgba),
              'matches old native pixels; NOT compositor/visual certification')
for name, expected in [('suites-final.log', 576), ('parley-default-final.log', 69), ('disabled-final.log', 22)]:
    groups = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', (ROOT / name).read_text())
    assert sum(int(g[0]) for g in groups) == expected
    assert all(g[1:] == ('0', '0') for g in groups)
    print('TESTS', name, expected, 'passing; overlap between commands is not additive')
assert 'Ran 20 tests' in (ROOT / 'python-sealed.log').read_text()
assert '\nOK\n' in (ROOT / 'python-sealed.log').read_text()
source = (ROOT / 'source-regressions-sealed.log').read_text()
assert '22 passed; 0 failed' in source
assert source.count('EXPECTED RED correction_') == 3
assert source.count('exit 101') == 3
print('PASS final20Python, exactobserver/freshpaint22, three expected baseline red tests')
for line in (ROOT / 'source-inputs.sha256').read_text().splitlines():
    digest, name = line.split(None, 1)
    assert sha(Path(name).read_bytes()) == digest, name
print('PASS final source-input hashes')
