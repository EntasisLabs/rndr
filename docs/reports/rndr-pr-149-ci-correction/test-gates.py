"""Regression checks for qualification evidence failures without editing history."""
from pathlib import Path
import contextlib
import hashlib
import io
import os
import runpy
import shlex
import subprocess
import tempfile
from unittest.mock import patch

root = Path.cwd()
driver = root / 'docs/reports/rndr-pr-149-ci-checks.sh'
line = next(l for l in driver.read_text().splitlines() if l.startswith('run environment '))
args = shlex.split(line)[2:]
assert args[:2] == ['sh', '-ec']
env = dict(os.environ, CARGO_BUILD_JOBS='1')
# All scratch Git changes are confined to a disposable test fixture. The real
# candidate and its immutable historical artifacts are never dirtied.
with tempfile.TemporaryDirectory(prefix='gate-fixture-', dir=root / 'docs/reports/rndr-pr-149-ci-correction') as d:
    p = Path(d)
    def git(*a):
        return subprocess.run(['git', *a], cwd=p, check=True, capture_output=True)
    git('init', '-q')
    git('config', 'user.name', 'qualification fixture')
    git('config', 'user.email', 'fixture@invalid')
    (p / 'crates').mkdir()
    f = p / 'crates/input.rs'
    f.write_text('// baseline\n')
    git('add', '.')
    git('commit', '-qm', 'fixture')
    clean = subprocess.run(args, cwd=p, env=env, capture_output=True, text=True)
    assert clean.returncode == 0 and 'rustc 1.98.0' in clean.stdout
    for staged in (False, True):
        f.write_text('// dirty\n')
        if staged:
            git('add', '.')
        dirty = subprocess.run(args, cwd=p, env=env, capture_output=True, text=True)
        assert dirty.returncode != 0
        assert 'rustc ' not in dirty.stdout and 'Filesystem' not in dirty.stdout
        assert 'diff --git' in dirty.stdout
        print('PASS: dirty', 'staged' if staged else 'unstaged', 'source stops actual environment gate before later commands; exit=', dirty.returncode)
    print('PASS: clean fixture passes actual environment gate.')

manifest = root / 'docs/reports/rndr-pr-149-ci-evidence/source-digests.txt'
before = manifest.read_bytes()
original_read = Path.read_text
seen = False
def corrupt_read(self, *a, **kw):
    global seen
    text = original_read(self, *a, **kw)
    if self.resolve() == manifest:
        seen = True
        return ('0' if text[0] != '0' else '1') + text[1:]
    return text
# Inject a corrupt read only; no historical bytes are changed even temporarily.
with patch.object(Path, 'read_text', corrupt_read), contextlib.redirect_stdout(io.StringIO()):
    try:
        runpy.run_path(str(root / 'docs/reports/rndr-pr-149-ci-audit.py'))
    except AssertionError as e:
        assert str(e) == 'recorded manifest mismatch'
    else:
        raise AssertionError('auditor accepted corrupt recorded manifest')
assert seen and manifest.read_bytes() == before
print('PASS: original repair auditor rejects injected recorded-manifest corruption and never writes it.')
print('Recorded manifest SHA256:', hashlib.sha256(before).hexdigest())
