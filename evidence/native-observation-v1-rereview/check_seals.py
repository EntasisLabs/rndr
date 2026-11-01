from pathlib import Path
import hashlib, subprocess, re, json, runpy
sha=lambda b:hashlib.sha256(b).hexdigest()
for name,count,want in [('native-observation-v1',111,'9034ec3ec28db8aa007ccd6faef65ba5139368e9fb0a549e5a4cbe7d2360b30a'),('native-observation-v1-review',30,'12e9fda99c12d431fca79acc745595800d647556ca80401d8dfc46337de89110'),('native-observation-v1-correction',62,'9e067e9daad5262a2f3bc59837a55994edba90b75e992e261b269077c08f094e')]:
 root=Path('evidence')/name; manifest=root/'MANIFEST.sha256'; lines=manifest.read_text().splitlines(); assert len(lines)==count; assert sha(manifest.read_bytes())==want
 covered=set()
 for row in lines:
  digest,file=row.split(None,1); p=Path(file.lstrip('*')); p=p if p.exists() else root/p
  assert sha(p.read_bytes())==digest,p; covered.add(p.resolve())
 assert covered=={p.resolve() for p in root.rglob('*') if p.is_file() and p!=manifest}
 print('ALL PAYLOADS',name,count,'manifest',want)
a='28a0ae307f869ef651d8ebcc5fdcd82a9dc31ce7'; b='57458a7a8b1b164ac4959ce1226b7a19c01c2e0b'; c='dc470507c5e65977ce34bbe6a6a9e4f1376f7983'
assert subprocess.check_output(['git','rev-parse',b+'^'],text=True).strip()==a
assert subprocess.check_output(['git','rev-parse',c+'^'],text=True).strip()==b
assert subprocess.check_output(['git','diff',a,b])==Path('evidence/native-observation-v1-correction/implementation.diff').read_bytes()
paths=subprocess.check_output(['git','diff','--name-only',b,c],text=True).splitlines(); assert len(paths)==63 and all(p.startswith('evidence/native-observation-v1-correction/') for p in paths)
print('EXACT DIFF and evidence-only seal ancestry PASS')
for line in Path('evidence/native-observation-v1-correction/source-inputs.sha256').read_text().splitlines():
 digest,p=line.split(None,1); assert sha(Path(p).read_bytes())==digest
print('ALL nine production/test/doc source hashes PASS')
for root,files in [('native-observation-v1-correction',[('suites-final.log',576),('parley-default-final.log',69),('disabled-final.log',22)]),('native-observation-v1-rereview',[('suites.log',576),('parley.log',69),('disabled.log',22)])]:
 for file,total in files:
  text=(Path('evidence')/root/file).read_text(); groups=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',text)
  assert sum(int(g[0]) for g in groups)==total and all(g[1:] == ('0','0') for g in groups)
  print(root,file,'PASS',total,'groups',len(groups))
print('PASS independent seal/diff/source/result audit')
