"""Fault-inject reads, never rewrite candidate/history: reject bad evidence."""
from pathlib import Path
from unittest.mock import patch
import contextlib, io, runpy, hashlib
root=Path.cwd()
old=root/'docs/reports/rndr-pr-149-ci-audit.py'
new=root/'docs/reports/rndr-pr-149-ci-correction/verify.py'
oldmanifest=root/'docs/reports/rndr-pr-149-ci-evidence/source-digests.txt'
newmanifest=root/'docs/reports/rndr-pr-149-ci-correction/source-digests.txt'
log=root/'docs/reports/rndr-pr-149-ci-evidence/final-checks/clippy-workspace.txt'
read=Path.read_text
for script,target,kind in [(old,oldmanifest,'digest'),(new,newmanifest,'digest'),(old,log,'compiler body')]:
    before=target.read_bytes(); seen=[]
    def inject(self,*a,**kw):
        text=read(self,*a,**kw)
        if self.resolve()==target:
            seen.append(True)
            return ('0' if text[0]!='0' else '1')+text[1:] if kind=='digest' else text.replace('Finished `dev` profile','NOT A SUCCESS')
        return text
    with patch.object(Path,'read_text',inject), patch.object(Path,'write_text',side_effect=AssertionError('write attempted')), patch.object(Path,'write_bytes',side_effect=AssertionError('write attempted')), contextlib.redirect_stdout(io.StringIO()):
        try: runpy.run_path(str(script))
        except AssertionError as e: assert str(e)!='write attempted'
        else: raise AssertionError('accepted injected corruption')
    assert seen and target.read_bytes()==before
    print('PASS rejected',kind,'in',target.relative_to(root),'with writes forbidden; unchanged SHA256',hashlib.sha256(before).hexdigest())
