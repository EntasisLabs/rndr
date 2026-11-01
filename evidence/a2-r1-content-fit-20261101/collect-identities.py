from pathlib import Path
import subprocess,hashlib,json,sys,re
p=Path(sys.argv[1]);start='92e98004d886a9ccfc3d2d05eaf090b591e10cc9';final=subprocess.check_output(['git','rev-parse','HEAD']).decode().strip()
def sha(b):return hashlib.sha256(b).hexdigest()
paths=subprocess.check_output(['git','diff','--name-only',start,final]).decode().splitlines();changes={}
for n in paths:
 try:
  old=subprocess.check_output(['git','show',start+':'+n],stderr=subprocess.DEVNULL)
  before={'git_blob':subprocess.check_output(['git','rev-parse',start+':'+n]).decode().strip(),'sha256':sha(old)}
 except subprocess.CalledProcessError:before={'absent':True}
 changes[n]={'before':before,'after':{'git_blob':subprocess.check_output(['git','rev-parse',final+':'+n]).decode().strip(),'sha256':sha(Path(n).read_bytes())}}
(p/'changed-source-blob-identities.json').write_text(json.dumps({'start':start,'implementation':final,'changes':changes},indent=2)+'\n')
for ref,suffix in [(start,'before'),(final,'after')]:
 (p/('all-relevant-git-blobs-'+suffix+'.txt')).write_bytes(subprocess.check_output(['git','ls-tree','-r',ref,'crates','Cargo.toml','Cargo.lock','docs/design']))
# Rehash every retained build artifact, including all failed/intermediate runs.
art=[]
for f in sorted(p.rglob('*')):
 if f.is_file() and (f.suffix=='.rlib' or f.read_bytes()[:4]==b'\xcf\xfa\xed\xfe'):
  art.append(f'{sha(f.read_bytes())}  {f}')
(p/'all-build-artifacts.sha256').write_text('\n'.join(art)+'\n')
results={}
for d in ['exact-ui-final/logs','rest-final/logs']:
 rows={}
 for f in sorted((p/d).glob('*.log')):
  a=[tuple(map(int,x)) for x in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',f.read_text())]
  if a:rows[f.name]=[sum(x[i] for x in a) for i in range(3)]
 results[d]=rows
(p/'suite-counts-final.json').write_text(json.dumps(results,indent=2)+'\n')
# Exact unchanged fixture input for both sides of final negative control.
a=(p/'fresh-baseline-final/inputs.sha256').read_text().splitlines();b=(p/'fresh-final-final/inputs.sha256').read_text().splitlines()
assert a[2:]==b[2:]
print('source/blobs',len(changes),'build artifacts',len(art),'finalUI',sum(v[0] for v in results['exact-ui-final/logs'].values()))
