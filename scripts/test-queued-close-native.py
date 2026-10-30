#!/usr/bin/env python3
"""Real native queued-open regression; no manual slots or visual acceptance."""
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / (sys.argv[1] if len(sys.argv) > 1 else 'evidence/queued-native')
OUT.mkdir(parents=True, exist_ok=False)
BIN = ROOT / 'target/debug/examples/queued_close'
TEXT = 'synthetic unsaved late window bytes α\n'.encode()
for visibility in ('visible', 'hidden'):
    for mode in ('leaf', 'recursive-build', 'recursive-frame'):
        for decision in ('protect', 'discard'):
            name = f'{visibility}-{mode}-{decision}'
            scratch = OUT / name
            scratch.mkdir()
            result = subprocess.run([str(BIN), str(scratch), visibility, mode, decision],
                                    cwd=ROOT, capture_output=True, text=True, timeout=20)
            (OUT / (name + '.log')).write_text(result.stdout + result.stderr)
            assert result.returncode == 0, (name, result.returncode, result.stderr)
            assert 'PASS queued native' in result.stderr
            assert 'FAILED_RETAINED exact=true' in result.stderr
            assert 'authorized=false' not in result.stderr
            protected = list(scratch.glob('protected-*'))
            count = 2 if mode == 'recursive-frame' else 1
            assert len(protected) == (0 if decision == 'discard' else count)
            assert all(path.read_bytes() == TEXT for path in protected)
            print(f'PASS {name}: exact draft retained through failure, then {decision}', flush=True)
print('12 real event-loop queued-open scenarios passed; no visual acceptance.', flush=True)
