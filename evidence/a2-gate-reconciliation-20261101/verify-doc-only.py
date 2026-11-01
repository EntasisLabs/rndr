#!/usr/bin/env python3
"""Read-only validation of this bounded doc/evidence delta against exact admitted parent."""
import hashlib
import pathlib
import re
import subprocess

root = pathlib.Path(__file__).resolve().parents[2]
parent = 'f427b7474dad99a983115edfad14c5084c9cb253'
prefix = 'evidence/a2-gate-reconciliation-20261101/'
tracking = {'docs/design/A0-A1-checkpoint.md', 'docs/design/A2-components.md', 'docs/design/implementation-tracking.md'}
new_doc = 'docs/design/A2-remaining-gates.md'
def git(*args):
    return subprocess.check_output(['git', '-C', str(root), *args])
assert git('merge-base', '--is-ancestor', parent, 'HEAD') == b''
changed = git('diff', '--name-only', '-z', parent).decode().split('\0')
untracked = git('ls-files', '--others', '--exclude-standard', '-z').decode().split('\0')
paths = sorted(set(changed + untracked) - {''})
assert paths
assert all(p in tracking or p == new_doc or p.startswith(prefix) for p in paths), paths
for p in tracking:
    before = git('show', f'{parent}:{p}')
    assert (root / p).read_bytes().startswith(before), p  # historical text exactly preserved
# Every already-tracked nontracking file must have no delta, including all old evidence.
check = ['git', '-C', str(root), 'diff', '--exit-code', parent, '--', '.', *(':(exclude)' + p for p in tracking), ':(exclude)' + new_doc, ':(exclude)' + prefix]
subprocess.run(check, check=True, stdout=subprocess.PIPE)
for p, expected in {
    'docs/design/UI Library Council — Specification.md': 'c4e6d10b0b44073631555ea663c4700df980ffd6ca8f7361c4d27d0c8322601b',
    'docs/design/UI Library Council — Review Guide.md': '8dc077adecc90ff0a8a7ad7284b7350744d45fe076370371038d0905312e35b0'
}.items():
    assert hashlib.sha256((root / p).read_bytes()).hexdigest() == expected
for path in [root/new_doc, root/prefix/'REPORT.md', root/prefix/'COMMANDS.md']:
    for target in re.findall(r'\]\(([^)]+)\)', path.read_text()):
        if '://' not in target and not target.startswith('#'):
            assert (path.parent / target.split('#')[0]).exists(), (path, target)
for path in (root/prefix).rglob('*'):
    if path.is_file():
        data = path.read_bytes()
        assert not data.startswith((b'\xcf\xfa\xed\xfe', b'\xfe\xed\xfa\xcf', b'\xca\xfe\xba\xbe', b'\x7fELF', b'!<arch>')), path
print('PASS: allowed doc/evidence paths only; historical tracking prefixes preserved; all other parent-tracked bytes unchanged')
print('PASS: canonical digests, new relative links, compiled binary exclusion')
print('Parent:', parent)
print('Unchanged implementation tree:', git('rev-parse', parent + ':crates').decode().strip())
print('Unchanged Cargo.toml blob:', git('rev-parse', parent + ':Cargo.toml').decode().strip())
print('Unchanged Cargo.lock blob:', git('rev-parse', parent + ':Cargo.lock').decode().strip())
print('Changed/new path count at validation:', len(paths))
