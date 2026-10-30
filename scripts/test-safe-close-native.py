#!/usr/bin/env python3
"""Real macOS synthetic-window checks. No live files, no screenshots or visual signoff.
Build safe_close first; run from repo root. Results are retained under evidence/native/.
"""
import pathlib
import subprocess
import time
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / (sys.argv[1] if len(sys.argv) > 1 else 'evidence/native')
BIN = ROOT / 'target' / 'debug' / 'examples' / 'safe_close'
TEXT = 'Synthetic unsaved draft α\nexact bytes, no live notes\n'.encode()
OUT.mkdir(parents=True, exist_ok=True)


def scenario(name, first='native-close', decision='retry', ordinary=False, ax=False, hidden=False):
    scratch = OUT / name
    scratch.mkdir()  # Refuse to overwrite old evidence or reuse nonempty scratch.
    logpath = OUT / (name + '.log')
    with logpath.open('w') as log:
        proc = subprocess.Popen([str(BIN), str(scratch)] + (['--ordinary'] if ordinary else []),
                                stdout=log, stderr=subprocess.STDOUT, cwd=ROOT)
        seq = 0

        def command(action):
            nonlocal seq
            seq += 1
            temporary = scratch / 'command.next'
            temporary.write_text(f'{seq}:{action}')
            temporary.replace(scratch / 'command')
            if not ordinary:
                expected = f'{seq}:{action}'
                ack = scratch / 'ack'
                wait(lambda: ack.exists() and ack.read_text() == expected,
                     f'command acknowledgment {expected}')

        def state():
            p = scratch / 'state'
            return p.read_text() if p.exists() else ''

        def wait(predicate, desc, seconds=10):
            until = time.monotonic() + seconds
            while time.monotonic() < until:
                if predicate():
                    return
                if proc.poll() is not None:
                    raise AssertionError(f'{name}: premature exit {proc.returncode} waiting for {desc}\n{logpath.read_text()}')
                time.sleep(.025)
            raise AssertionError(f'{name}: timeout {desc}\n{logpath.read_text()}')

        try:
            if ordinary:
                wait(lambda: 'probe ready' in logpath.read_text(), 'startup')
                time.sleep(1)
                command('native-close')
            else:
                wait(lambda: state() == 'Idle', 'native window/first update')
                if hidden:
                    command('hide')
                    time.sleep(.3)
                if ax:
                    result = subprocess.run(['/usr/bin/swift', str(ROOT / 'scripts' / 'safe-close-press.swift'), str(proc.pid)],
                                            capture_output=True, text=True, timeout=15)
                    print(result.stdout.strip(), flush=True)
                    assert result.returncode == 0, result.stderr
                else:
                    command(first)
                wait(lambda: state().startswith('Pending'), 'pending native close')
                pending = state()
                command(first)  # duplicate while actual background protection is pending
                if decision == 'cancel':
                    command('cancel')
                    wait(lambda: state() == 'Idle', 'cancel')
                    time.sleep(.9)
                    assert proc.poll() is None and state() == 'Idle'
                    assert 'accepted=false' in logpath.read_text(), 'late real worker result rejected'
                    command(first)
                    wait(lambda: state().startswith('Pending') and state() != pending, 'fresh post-cancel token')
                wait(lambda: state().startswith('Failed'), 'injected real IO failure')
                failed = state()
                assert proc.poll() is None and not (scratch / 'recovery').exists()
                command(first)  # native double-close must not discard or retry failure
                assert proc.poll() is None and state() == failed
                assert 'draft_exact=true' in logpath.read_text()
                command('discard' if decision in ('cancel', 'discard') else 'retry')
            assert proc.wait(timeout=10) == 0, logpath.read_text()
            assert 'run returned' in logpath.read_text()
            if ordinary:
                assert 'unguarded control: recovery failed' in logpath.read_text()
            if decision == 'retry' and not ordinary:
                assert (scratch / 'recovery').read_bytes() == TEXT
            else:
                assert not (scratch / 'recovery').exists()
            description = 'ordinary native close; unguarded failure/loss control reproduced' if ordinary else f'pending/failure retained exact draft; {decision}'
            print(f'PASS {name}: {description}', flush=True)
        finally:
            if proc.poll() is None:
                proc.terminate()  # only this synthetic process, only on harness failure
                proc.wait(timeout=5)


scenario('ax-close-retry', ax=True)
scenario('native-close-discard', decision='discard')
scenario('native-close-cancel', decision='cancel')
scenario('native-quit-retry', first='native-quit')
scenario('hidden-close-retry', hidden=True)
scenario('ordinary-close', ordinary=True)
print('Native behavior checks passed. UI appearance/real-editor integration NOT accepted.', flush=True)
