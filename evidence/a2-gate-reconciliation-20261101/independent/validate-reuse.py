from pathlib import Path
import hashlib,subprocess,json,sys
out=Path(sys.argv[1]);b=Path('/tmp/rndr-a2-independent-SQwoYo/exact');r=Path('/tmp/rndr-a2-fresh-review-4TfleE');u=r/'exact-ui'
def sha(x):return hashlib.sha256(x).hexdigest()
def git(n):return subprocess.check_output(['git','show','92e98004:'+n])
assert sha((r/'REVIEW.md').read_bytes())=='d617301815a616cc3ea53f5b688e9f75debe5b3495a6d82d46837d85ecceb95d'
assert sha((r/'MANIFEST.sha256').read_bytes())=='85ff2e5f84ad365c93a336095f902d9e04f8f465fc3e1c9c0ef4b50707e4e039'
original=0
for line in (r/'MANIFEST.sha256').read_text().splitlines():
 h,n=line.split(maxsplit=1);assert sha((r/n).read_bytes())==h,n;original+=1
assert original==216
counts={'fresh_original_manifest_rows':original};details=[]
for receipt in [b/'source-before-sha256.txt',b/'binary-identities-sha256.txt',b/'current-rlibs-sha256.txt',b/'dependency-rlibs-sha256.txt',u/'ui-source-after-sha256.txt',u/'handoff-rlibs-sha256.txt']:
 count=0
 for row in receipt.read_text().splitlines():
  h,n=row.split(maxsplit=1)
  if receipt.name=='source-before-sha256.txt' and n.startswith('crates/rndr_ui/'):continue
  data=git(n) if receipt.name=='ui-source-after-sha256.txt' else Path(n).read_bytes()
  assert sha(data)==h,n
  if receipt.name=='source-before-sha256.txt':assert data==git(n)==Path(n).read_bytes(),n
  count+=1;details.append({'receipt':str(receipt),'path':n,'sha256':h})
 counts[str(receipt)]=count
assert counts[str(b/'source-before-sha256.txt')]==147
(out/'reuse-validation-final.json').write_text(json.dumps({'counts':counts,'validated_rows':details},indent=2)+'\n')
print(json.dumps(counts,indent=2))
