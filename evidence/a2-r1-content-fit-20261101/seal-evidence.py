from pathlib import Path
import subprocess,hashlib,json,datetime
p=Path(__file__).resolve().parent
assert not (p/'MANIFEST.sha256').exists(),'Already sealed; do not overwrite evidence'
expected='91c008bdc1054f42f67dcd5d45729290ddb4ffa4';start='92e98004d886a9ccfc3d2d05eaf090b591e10cc9'
subprocess.run(['git','diff','--exit-code',expected,'--','crates','Cargo.toml','Cargo.lock','docs/design'],check=True,stdout=subprocess.DEVNULL)
def sha(b):return hashlib.sha256(b).hexdigest()
def check_rows(receipt,root=None):
 count=0
 for line in receipt.read_text().splitlines():
  h,n=line.split(maxsplit=1);f=(root/n) if root else Path(n)
  assert f.is_file() and sha(f.read_bytes())==h,n;count+=1
 return count
assert (p/'exact-ui-final/ui-source-before-sha256.txt').read_bytes()==(p/'exact-ui-final/ui-source-after-sha256.txt').read_bytes()
source_count=check_rows(p/'exact-ui-final/ui-source-after-sha256.txt')
history_count=check_rows(p/'history-before.sha256');assert history_count==961
artifact_count=check_rows(p/'all-build-artifacts.sha256');assert artifact_count==66
# Entire initial source snapshot is exact clean starting Git, not current bytes.
for line in (p/'source-before.sha256').read_text().splitlines():
 h,n=line.split(maxsplit=1);assert sha(subprocess.check_output(['git','show',start+':'+n]))==h,n
original=Path('/tmp/rndr-a2-fresh-review-4TfleE')
assert sha((original/'REVIEW.md').read_bytes())=='d617301815a616cc3ea53f5b688e9f75debe5b3495a6d82d46837d85ecceb95d'
assert sha((original/'MANIFEST.sha256').read_bytes())=='85ff2e5f84ad365c93a336095f902d9e04f8f465fc3e1c9c0ef4b50707e4e039'
assert check_rows(original/'MANIFEST.sha256',original)==216
for n in ['REVIEW.md','COMMANDS.md','fresh-probes.rs','fractional-survey.rs','MANIFEST.sha256']:
 assert (p/'independent-review'/n).read_bytes()==(original/n).read_bytes(),n
for d in ['fresh-baseline-final','fresh-final-final']:
 check_rows(p/d/'inputs.sha256');check_rows(p/d/'artifacts.sha256')
assert (p/'fresh-baseline-final/content-tests.exit').read_text().strip()=='101'
assert '0 passed; 4 failed;' in (p/'fresh-baseline-final/content-tests.log').read_text()
assert '2 passed; 2 failed;' in (p/'fresh-baseline-final/fresh-probes.log').read_text()
for n in ['content-tests','fresh-probes','fractional-survey']:
 assert (p/'fresh-final-final'/(n+'.exit')).read_text().strip()=='0'
assert '4 passed; 0 failed;' in (p/'fresh-final-final/content-tests.log').read_text()
assert (p/'fresh-baseline-final/fractional-survey.log').read_text().count('MISMATCH')==13
assert (p/'fresh-final-final/fractional-survey.log').read_text().count('MISMATCH')==0
counts=json.loads((p/'suite-counts-final.json').read_text())
assert sum(v[0] for v in counts['exact-ui-final/logs'].values())==41
assert counts['rest-final/logs']['cargo-libraries.log']==[524,0,0]
assert counts['rest-final/logs']['cargo-render.log']==[24,0,0]
assert all(v[1:]==[0,0] for group in counts.values() for v in group.values())
galleries=json.loads((p/'gallery-reconciliation.json').read_text());assert len(galleries)==8
for n in galleries:
 for suffix in ['.display-list.txt','.semantics-geometry.txt']:
  f=n+suffix;data=(p/'exact-ui-final/gallery-one'/f).read_bytes()
  assert data==(p/'exact-ui-final/gallery-two'/f).read_bytes()==(original/'exact-ui/gallery-one'/f).read_bytes(),f
for suffix in ['manifest.txt']:
 # The generators include their inventory; compare entire path sets below.
 pass
for f in (p/'exact-ui-final/gallery-one').iterdir():
 assert f.read_bytes()==(p/'exact-ui-final/gallery-two'/f.name).read_bytes()==(original/'exact-ui/gallery-one'/f.name).read_bytes(),f.name
check_rows(p/'exact-ui-final/handoff-rlibs-sha256.txt')
validation={'timestamp_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'tested_implementation':expected,'clean_start':start,'tested_ui_source_rows':source_count,'historical_evidence_files_unchanged':history_count,'retained_build_artifacts_verified':artifact_count,'original_review_manifest_verified':216,'new_fixture_baseline':'0/4 pass, runtime exit101','new_fixture_final':'4/4 pass, exit0','unchanged_fresh_probes':'2/4->4/4','fractional_survey':'diagnostic175 configs,13->0 mismatches','galleries':'8 cases, two processes, all17 files identical to92e98004','no_source_change_after_testing':True,'acceptance':'A2 unaccepted; owner fresh independent review next','native':'UNVERIFIED'}
(p/'seal-validation.json').write_text(json.dumps(validation,indent=2)+'\n')
(p/'SEAL.txt').write_text('A2 R1 continuation correction evidence seal\nTested implementation '+expected+'\nClean start '+start+'\nSource unchanged after final verification. No acceptance/integration/native certification.\nMANIFEST.sha256 covers every file in this directory except itself, including nested original inventories as preserved data.\nOriginal artifacts retained at /tmp/rndr-a2-content-fit-UJYjbC; build artifact hashes verified before seal.\nVerify from repository root: python3 evidence/a2-r1-content-fit-20261101/check-seal.py\n')
files=sorted(f for f in p.rglob('*') if f.is_file() and f!=p/'MANIFEST.sha256')
(p/'MANIFEST.sha256').write_text(''.join(sha(f.read_bytes())+'  '+str(f.relative_to(p))+'\n' for f in files))
print('SEALED',len(files),'entries; MANIFEST SHA256',sha((p/'MANIFEST.sha256').read_bytes()))
