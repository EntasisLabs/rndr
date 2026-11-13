"""Read-only independent static/provenance checks; no builds or source edits."""
from pathlib import Path
import subprocess, hashlib, csv, re, html
BASE='43a0e83394ac80a9ac35bcffde479bef4d71cfd1'
SOURCE='6698b63cdf11ab7054f431be5bdebe9b6028edc9'
CANDIDATE='9961f763b74c24927d8be7b7c652f5269948f768'
def git(*a): return subprocess.check_output(['git',*a])
allowed={'crates/rndr_core/src/freeze.rs','crates/rndr_core/src/structured/text_ops.rs','crates/rndr_core/tests/structured_composer.rs'}
assert set(git('diff','--name-only',BASE,CANDIDATE,'--',':!docs').decode().splitlines()) == allowed
assert not git('diff',SOURCE,CANDIDATE,'--',':!docs')
assert not git('diff',CANDIDATE,'--',':!docs')
old=git('ls-tree','-r','--name-only',BASE,'--','docs/reports').decode().splitlines()
assert not git('diff',BASE,'--',*old)
changes=[]
manifest=Path('docs/reports/rndr-flow-f5-independent-seam-review-evidence/qualification/source-digests.txt')
seen=set()
for l in manifest.read_text().splitlines():
 digest,p=l.split('  ',1)
 assert p not in seen; seen.add(p)
 assert hashlib.sha256(git('show',f'{BASE}:{p}')).hexdigest()==digest
 assert Path(p).read_bytes()==git('show',f'{SOURCE}:{p}')
 if hashlib.sha256(Path(p).read_bytes()).hexdigest()!=digest: changes.append(p)
assert len(seen)==589 and set(changes)==allowed
print('PASS: candidate/source production equality, historical reports unchanged, 589 historical fingerprints valid; 586 current unchanged, exactly 3 changed.')
e=Path('docs/reports/rndr-pr-149-ci-evidence')
for directory,n in [('final-checks',16),('qualification',17)]:
 rows=list(csv.DictReader((e/directory/'summary.tsv').open(),delimiter='\t'))
 assert len(rows)==n
 assert SOURCE in (e/directory/'environment.txt').read_text()
 for r in rows:
  s=(e/directory/(r['gate']+'.txt')).read_text()
  assert s.endswith('\nexit='+r['exit']+'\n')
  if r['gate'].startswith('clippy-'):
   assert r['exit']=='0' and '-D warnings' in s.splitlines()[0]
   assert 'Finished `dev` profile' in s and not re.search(r'^error:',s,re.M)
 blocked=[r['gate'] for r in rows if r['exit']!='0']
 assert blocked == ([] if directory=='final-checks' else ['preserved-probes'])
 print(directory,': per-log statuses agree with summary; all seven local Clippy logs successful; nonzero gates=',blocked)
p=Path.home()/'.rustup/toolchains/stable-aarch64-apple-darwin/share/doc/rust/html/src/core/sync/atomic.rs.html'
lines=html.unescape(re.sub('<[^>]+>','',p.read_text())).splitlines()
print('Installed integer-atomic source:',p)
for i,l in enumerate(lines):
 if 'pub fn fetch_update<F>' in l and '$int_type' in ''.join(lines[i:i+5]):
  print('\n'.join(lines[i-13:i+8]))
 if 'pub fn try_update(' in l and '$int_type' in ''.join(lines[i:i+8]):
  print('\n'.join(lines[i-5:i+15]))
remaining=[]
for p in sorted(Path('crates').rglob('*.rs')):
 for n,l in enumerate(p.read_text().splitlines(),1):
  if '.fetch_update(' in l: remaining.append((str(p),n,l.strip()))
assert len(remaining)==4
print('BLOCKER: remaining deprecated integer-atomic call sites in workspace members:')
for p,n,l in remaining: print(f'{p}:{n}: {l}')
print('Static Rust 1.100 CI failure prediction, NOT a locally executed Rust 1.100 build.')
