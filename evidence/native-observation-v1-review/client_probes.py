"""Bounded independent client probes. Synthetic files only, no app secrets."""
import sys, os, subprocess, tempfile, time, json
from pathlib import Path
sys.path.insert(0, str(Path('tools').resolve()))
from observe_client import save_frame

with tempfile.TemporaryDirectory(prefix='rndr-review-fifo-') as tmp:
    root=Path(tmp); os.chmod(root,0o700)
    fifo=root/'session.json'; os.mkfifo(fifo,0o600)
    command=[sys.executable,'-c',"import sys;sys.path.insert(0,'tools');from observe_client import observe;observe(sys.argv[1],timeout=.05)",str(fifo)]
    start=time.monotonic()
    try:
        subprocess.run(command,check=True,timeout=.8,env={**os.environ,'PYTHONDONTWRITEBYTECODE':'1'})
        raise AssertionError('unexpected successful FIFO observation')
    except subprocess.TimeoutExpired:
        print('REPRODUCED: descriptor FIFO blocks beyond timeout=.05; harness killed at .8s; elapsed',round(time.monotonic()-start,3))
with tempfile.TemporaryDirectory(prefix='rndr-review-output-') as tmp:
    p=Path(tmp)/'frame'; (p.with_suffix('.json')).write_text('prior metadata')
    try: save_frame(p,{'new':True},b'new PNG placeholder')
    except FileExistsError: pass
    else: raise AssertionError('overwrite was not refused')
    assert p.with_suffix('.png').read_bytes()==b'new PNG placeholder'
    assert p.with_suffix('.json').read_text()=='prior metadata'
    print('REPRODUCED: second-file collision leaves newly-created PNG beside preexisting JSON; call errors, never overwrites')
