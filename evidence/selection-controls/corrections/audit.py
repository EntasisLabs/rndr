"""Read-only preservation/source/capture audit; writes only its new JSON result."""
import hashlib,json,pathlib,re,struct,subprocess
root=pathlib.Path(__file__).resolve().parents[3]
out=root/'evidence/selection-controls/corrections'
sha=lambda p:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
git=lambda *a:subprocess.check_output(['git',*a],cwd=root)
base='97091b8c8c0baf0db0dc8817727e24ab2e244230'
source='586c6e732f50aefc07ffae98b05a5b6423e41a81'
# Every historical tracked evidence blob, not just this increment.
preserved=0
for line in git('ls-tree','-r',base,'evidence').decode().splitlines():
    meta,p=line.split('\t');oid=meta.split()[2]
    assert (root/p).read_bytes()==git('cat-file','blob',oid),p
    preserved+=1
probe='crates/rndr_ui/tests/selection_independent_review.rs'
assert (root/probe).read_bytes()==git('show',base+':'+probe),probe
native=out/'native';receipt=json.loads((native/'receipt.json').read_text())
assert receipt['source_head']==source
assert sha(native/'source.sha256.json')==receipt['source_manifest_sha256']
manifest=json.loads((native/'source.sha256.json').read_text())
for p,h in manifest.items():
    assert sha(root/p)==h,p
    assert hashlib.sha256(git('show',source+':'+p)).hexdigest()==h,p
assert sha(receipt['binary'])==receipt['binary_sha256']
frames=0
for run in receipt['runs']:
    assert run['exit']==0,run['name']
    directory=native/run['name'];result=json.loads((directory/'result.json').read_text())
    assert result['status']=='captured' and result['normal_shutdown_cleanup']
    assert result['binary_sha256']==receipt['binary_sha256']
    revisions=[]
    for i,frame in enumerate(run['frames'],1):
        png=directory/f'frame-{i}.png';metadata=directory/f'frame-{i}.json'
        assert sha(png)==frame['png_sha256'] and sha(metadata)==frame['metadata_sha256']
        m=json.loads(metadata.read_text());data=png.read_bytes()
        assert m['source']=='live-surface-copy-before-present' and not m['presented_confirmation']
        assert m['frame_revision']==frame['revision']
        assert m['png_bytes']==len(data) and list(struct.unpack('>II',data[16:24]))==[m['width'],m['height']]
        revisions.append(int(m['frame_revision']));frames+=1
    assert revisions[1]>revisions[0]
counts={}
for name in ['probes','core-paint','ui-focused','native-observe']:
    text=(out/(name+'.log')).read_text()
    assert text.rstrip().endswith('EXIT=0'),name
    matches=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed;',text)
    assert matches and all(int(f)==0 for _,f in matches),name
    counts[name]=sum(int(p) for p,_ in matches)
source_manifest={p:sha(root/p) for p in git('ls-files','crates','Cargo.toml','Cargo.lock','rust-toolchain.toml','docs/selection-controls.md','scripts','tools').decode().splitlines() if (root/p).is_file()}
assert source_manifest==manifest
result={'baseline':base,'corrected_source':source,'historical_evidence_files_unchanged':preserved,'original_independent_probes_unchanged':True,'source_files_bound':len(manifest),'native_presets':len(receipt['runs']),'native_frames':frames,'binary_sha256':receipt['binary_sha256'],'test_executions':counts,'total_gate_executions':sum(counts.values()),'counts_caveat':'Executions, not unique tests; earlier fixture runs overlap and are excluded; gallery cases are inside one test.','visual_acceptance':False,'visual_note':'One direct correction image call delivered metadata only; appearance claims were immediately retracted. Historical dark-single owner claim also retracted. Old DataTable screenshots are not selection-control evidence.','old_capture_status':'Historical native captures bind old source only; new affected subset is in corrections/native.','disk_free_bytes':__import__('shutil').disk_usage(root).free}
assert result['disk_free_bytes']>=2*1024**3
(out/'audit.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
