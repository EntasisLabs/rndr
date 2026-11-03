"""Read-only independent preservation/source/capture audit; writes only its result."""
import hashlib, json, pathlib, re, subprocess
P=pathlib.Path
out=P('evidence/selection-controls/independent-review')
candidate='9146cbdccbcb78daa36510f9a975364cf7bea052'
baseline='c9536acba3914a3be39c53c6bffe42a9b9c40c42'
source='19f553c0d57339ee666d9752dedca861331209f1'
def git(*args):return subprocess.check_output(['git',*args])
def tree(ref, prefix):
    ans={}
    for entry in git('ls-tree','-rz',ref,'--',prefix).split(b'\0'):
        if entry:
            meta,path=entry.split(b'\t',1); ans[path.decode()]=meta.split()[2].decode()
    return ans

def sha(path):
    h=hashlib.sha256()
    with open(path,'rb') as f:
        for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
    return h.hexdigest()

def oid(path):
    b=P(path).read_bytes();return hashlib.sha1(b'blob '+str(len(b)).encode()+b'\0'+b).hexdigest()
r={'candidate':candidate,'baseline':baseline,'source':source}
old=tree(baseline,'evidence');now=tree(candidate,'evidence')
r['baseline_evidence_files']=len(old)
r['baseline_evidence_changed_or_removed']=[p for p,h in old.items() if now.get(p)!=h]
r['candidate_evidence_files']=len(now)
r['candidate_evidence_disk_mismatches']=[p for p,h in now.items() if not P(p).is_file() or oid(p)!=h]
base=P('evidence/selection-controls/native')
m=base/'source.sha256.json';manifest=json.loads(m.read_text())
r['source_manifest_files']=len(manifest)
r['source_manifest_disk_mismatches']=[p for p,h in manifest.items() if sha(p)!=h]
r['source_manifest_source_commit_mismatches']=[p for p,h in manifest.items() if hashlib.sha256(git('show',f'{source}:{p}')).hexdigest()!=h]
receipt=json.loads((base/'receipt.json').read_text())
r['receipt_manifest_sha_matches']=sha(m)==receipt['source_manifest_sha256']
r['receipt_binary_sha_matches_current_binary']=sha(receipt['binary'])==receipt['binary_sha256']
r['receipt_source_head_matches']=receipt['source_head']==source
errors=[];frames=0
for run in receipt['runs']:
    d=base/run['name'];result=json.loads((d/'result.json').read_text())
    if not (run['exit']==0 and result['exit_code']==0 and result['status']=='captured' and result['binary_sha256']==receipt['binary_sha256']):errors.append(str(d)+': result')
    for index,f in enumerate(run['frames'],1):
        png=d/f'frame-{index}.png';meta=d/f'frame-{index}.json'
        if sha(png)!=f['png_sha256']:errors.append(str(png)+': hash')
        if sha(meta)!=f['metadata_sha256']:errors.append(str(meta)+': hash')
        md=json.loads(meta.read_text())
        if str(md['frame_revision'])!=f['revision'] or md['source']!=f['source']:errors.append(str(meta)+': revision/source')
        if md.get('presented_confirmation') is not False:errors.append(str(meta)+': unexpected presentation claim')
        frames+=1
r['receipt_states']=len(receipt['runs']);r['receipt_frames']=frames;r['receipt_errors']=errors
r['gate_counts']={}
for name in ['core-paint','ui-focused','native-observe']:
    text=(out/f'{name}.log').read_text()
    matches=re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored',text)
    r['gate_counts'][name]={'passed_executions':sum(int(m[1]) for m in matches),'failed':sum(int(m[2]) for m in matches),'ignored':sum(int(m[3]) for m in matches),'summaries':len(matches),'exit_zero':text.rstrip().endswith('EXIT=0')}
r['counts_caveat']='Execution counts, including doctests. Not unique-test totals; overlap original implementation runs and own probe reruns. Gallery matrix includes 24 cases within one test.'
r['audit_ok']=not any(r[k] for k in ['baseline_evidence_changed_or_removed','candidate_evidence_disk_mismatches','source_manifest_disk_mismatches','source_manifest_source_commit_mismatches','receipt_errors']) and all(r[k] for k in ['receipt_manifest_sha_matches','receipt_binary_sha_matches_current_binary','receipt_source_head_matches'])
(out/'audit.json').write_text(json.dumps(r,indent=2)+'\n')
print(json.dumps(r,indent=2))
assert r['audit_ok']
