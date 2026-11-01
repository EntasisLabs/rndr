from pathlib import Path
import hashlib,subprocess,json,re,sys
p=Path(sys.argv[1]);old=Path('/tmp/rndr-a2-fresh-review-4TfleE/exact-ui/gallery-one');new=p/'exact-ui-v2/gallery-one';repeated=p/'exact-ui-v2/gallery-two'
rows={}
for f in sorted(old.glob('*.semantics-geometry.txt')):
 case=f.name.split('.semantics-geometry')[0];s=f.read_bytes();t=(new/f.name).read_bytes()
 assert s==t==(repeated/f.name).read_bytes(),f.name
 d=case+'.display-list.txt';assert (old/d).read_bytes()==(new/d).read_bytes()==(repeated/d).read_bytes(),d
 text=t.decode();rows[case]={'semantics_geometry_identical':True,'display_list_identical':True,'independent_process_repeat_identical':True,'access_node_count':sum(x.startswith('access AccessNode') for x in text.splitlines()),'label_count':sum(x.startswith('label Entity') for x in text.splitlines()),'sha256':{n:hashlib.sha256((new/n).read_bytes()).hexdigest() for n in [f.name,d]}}
assert len(rows)==8
(p/'gallery-reconciliation.json').write_text(json.dumps(rows,indent=2)+'\n')
# Preserve original32 tests (all original files other than old5 allocation).
files=subprocess.check_output(['git','ls-tree','-r','--name-only','92e98004','crates/rndr_ui/tests']).decode().splitlines()
unchanged=[]
for f in files:
 if f.endswith('/a2_toolbar_allocation.rs'):continue
 assert Path(f).read_bytes()==subprocess.check_output(['git','show','92e98004:'+f]),f
 unchanged.append(f)
# Every previously tracked evidence file is byte-for-byte preserved.
count=0
for line in (p/'history-before.sha256').read_text().splitlines():
 h,n=line.split(maxsplit=1);assert hashlib.sha256(Path(n).read_bytes()).hexdigest()==h,n;count+=1
# Full UI source identity unchanged since tested direct build, including new file.
a=(p/'exact-ui-v2/ui-source-before-sha256.txt').read_bytes();b=(p/'exact-ui-v2/ui-source-after-sha256.txt').read_bytes();assert a==b
for line in b.decode().splitlines():
 h,n=line.split(maxsplit=1);assert hashlib.sha256(Path(n).read_bytes()).hexdigest()==h,n
(p/'preservation-audit.json').write_text(json.dumps({'original32_test_files_unchanged':unchanged,'historical_evidence_files_unchanged':count,'all8_gallery_cases_unchanged_from92e98004':True,'direct_source_unchanged_after_tests':True},indent=2)+'\n')
print('PASS: eight galleries/two processes identical to exact start; original32 and all historical evidence preserved; tested source unchanged')
