import pathlib,subprocess,hashlib,json,re,sys
out=pathlib.Path(sys.argv[1]);sha=lambda b:hashlib.sha256(b).hexdigest()
def git(*a):return subprocess.check_output(['git',*a])
assert git('rev-parse','HEAD').decode().strip()=='f427b7474dad99a983115edfad14c5084c9cb253'
assert not git('status','--porcelain')
paths=git('ls-tree','-r','--name-only','92e98004','--','evidence').decode().splitlines()
for n in paths:assert git('show','92e98004:'+n)==pathlib.Path(n).read_bytes(),n
source=git('ls-files','crates','Cargo.toml','Cargo.lock').decode().splitlines()
for n in source:assert git('show','HEAD:'+n)==pathlib.Path(n).read_bytes(),n
baseline_test_paths=git('ls-tree','-r','--name-only','31006f0d','--','crates/rndr_ui/tests').decode().splitlines();count=0
for n in baseline_test_paths:
 data=pathlib.Path(n).read_bytes();assert git('show','31006f0d:'+n)==data,n;count+=len(re.findall(rb'#\[test\]',data))
assert count==32,count
assert not git('diff','91c008bd','HEAD','--','crates','docs','Cargo.toml','Cargo.lock')
a=out/'exact-ui/gallery-one';b=pathlib.Path('/tmp/rndr-a2-fresh-review-4TfleE/exact-ui/gallery-one');worker=pathlib.Path('/tmp/rndr-a2-content-fit-UJYjbC/exact-ui-final/gallery-one')
for f in a.iterdir():assert f.read_bytes()==(b/f.name).read_bytes()==(worker/f.name).read_bytes()==(out/'exact-ui/gallery-two'/f.name).read_bytes(),f
p=out/'rest/passive-gallery';gold=pathlib.Path('evidence/ui-gallery')
for f in p.iterdir():assert f.read_bytes()==(gold/f.name).read_bytes(),f
result={'historical_evidence_blobs_unchanged':len(paths),'all_current_source_blob_matches':len(source),'original_UI_tests_unchanged':count,'original_UI_test_files_unchanged':len(baseline_test_paths),'galleries_start_worker_and_two_fresh_runs_identical_files':len(list(a.iterdir())),'passive_identical_files':len(list(p.iterdir()))}
(out/'audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
