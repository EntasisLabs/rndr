#!/usr/bin/env bash
# Run after the documented build; no builds, cache cleanup, or OS automation.
set -euo pipefail
cd "$(dirname "$0")/../../.."
source scripts/data-gallery-env.sh
out="${1:?Supply a fresh output directory}"
mkdir -p "$out"
python3 - "$out" <<'PY'
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

out = Path(sys.argv[1])
binary = Path(os.environ['CARGO_TARGET_DIR']) / 'debug/examples/data_gallery'
states = [('light', []), ('dark', ['--dark']), ('narrow', ['--dark', '--narrow']),
          ('tree-focus', ['--tree-focus']), ('disabled', ['--disabled']),
          ('empty', ['--empty']), ('filtered', ['--filtered']),
          ('compact-hc-200', ['--dark', '--compact', '--hc', '--200']), ('large', ['--large'])]
assert not (out / 'binary.json').exists(), 'Use a fresh output directory'
assert all(not (out / ('native-' + name)).exists() for name, _ in states)
(out / 'binary.json').write_text(json.dumps({
    'path': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
    'build_log': 'verification/gallery-build.log', 'source_binding': 'source.sha256',
    'note': 'Current correction worktree build in documented compatible cache; not the historical binary.'
}, indent=2) + '\n')
for name, flags in states:
    command = ['python3', 'tools/validate_observation_native.py', str(binary), str(out / ('native-' + name)), '--', *flags]
    with (out / ('capture-' + name + '.log')).open('w') as log:
        log.write('$ ' + repr(command) + '\n')
        log.flush()
        result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT)
    print(name, result.returncode, flush=True)
    if result.returncode:
        raise SystemExit(result.returncode)
PY
