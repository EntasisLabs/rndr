#!/usr/bin/env python3
"""Verify candidate bindings without rebuilding or modifying historical evidence."""
import hashlib,json,pathlib,re,subprocess,sys
ROOT=pathlib.Path(__file__).resolve().parents[3]
import os
os.chdir(ROOT)
OUT=pathlib.Path('evidence/data-components-next/combined-candidate')
BASE='132258c535b7c75dcc7256b1eea89e913eaaed15'
NATIVE=OUT/'native-final-4'
VERIFY=OUT/'verification-final-3'
def sha(p):return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def write(p,data):p.write_text(json.dumps(data,indent=2,sort_keys=True)+'\n')
def owners():
    return {str(p):{'sha256':sha(p),'bytes':p.stat().st_size} for d in ['owner-editor-gap-check','owner-editor-gap-check-retry'] for p in sorted((OUT.parent/d).rglob('*')) if p.is_file()}
def tests():
    paths=subprocess.check_output(['git','ls-files','crates/rndr_core/tests','crates/rndr_paint/tests','crates/rndr_ui/tests'],text=True).splitlines()
    paths+=['crates/rndr_ui/tests/data_editors.rs','crates/rndr_ui/tests/data_editors_gallery.rs','crates/rndr_ui/tests/field_transactions.rs','evidence/data-components/independent-review/adversarial.rs','evidence/data-components/independent-rereview/activation-policy.rs','evidence/data-components-next/blockers/baseline-probes.rs']
    return {p:sha(p) for p in sorted(set(paths)) if pathlib.Path(p).is_file()}
if '--record' in sys.argv:
    assert not (OUT/'preserved-owner-snapshot.json').exists(),'Refusing snapshot overwrite'
    write(OUT/'preserved-owner-snapshot.json',owners())
    write(OUT/'test-source.sha256.json',tests())
assert json.loads((OUT/'preserved-owner-snapshot.json').read_text())==owners(),'Owner reproduction changed after final snapshot'
assert json.loads((OUT/'test-source.sha256.json').read_text())==tests(),'Verified test source changed'
# All historical tracked evidence, including the blocker audit/experiment, stays byte-identical.
old=subprocess.check_output(['git','ls-tree','-r','--name-only',BASE,'--','evidence'],text=True).splitlines()
old_total=0
for path in old:
    blob=subprocess.check_output(['git','cat-file','blob',BASE+':'+path])
    assert pathlib.Path(path).read_bytes()==blob,('Historical evidence changed',path)
    old_total+=len(blob)
# Exact source and executable used across the successful twelve-state matrix.
source=json.loads((NATIVE/'source.sha256.json').read_text())
for p,digest in source.items():assert sha(p)==digest,('Captured source changed',p)
r=json.loads((NATIVE/'receipt.json').read_text())
assert sha(r['binary'])==r['binary_sha256']
assert sha(NATIVE/'source.sha256.json')==r['source_manifest_sha256']
assert len(r['runs'])==12 and all(x['exit']==0 for x in r['runs'])
for run in r['runs']:
    assert not run['presented_confirmation']
    assert len(run['frames'])==2
    for index,f in enumerate(run['frames'],1):
        png=NATIVE/run['name']/f'frame-{index}.png';meta=NATIVE/run['name']/f'frame-{index}.json'
        assert sha(png)==f['png_sha256'] and sha(meta)==f['metadata_sha256']
        m=json.loads(meta.read_text());assert m['source']=='live-surface-copy-before-present' and not m['presented_confirmation']
        assert m['png_bytes']==png.stat().st_size and m['render_provenance']['backend']=='Metal' and m['render_provenance']['text_engine']=='parley'
    result=json.loads((NATIVE/run['name']/'result.json').read_text())
    assert result['status']=='captured' and result['normal_shutdown_cleanup']
# Exact passing commands and extant compiled test executables. Doctest temporary
# binaries are not retained; their source/command/exits remain bound separately.
command_results={};binaries={}
for path in sorted(VERIFY.glob('*.log')):
    text=path.read_text();assert re.search(r'\nEXIT=0\s*$',text),('Failed verification command',str(path))
    command_results[path.name]={'command':text.splitlines()[0],'exit':0,'passed_executions':sum(map(int,re.findall(r'test result: ok\. (\d+) passed',text))),'ignored_executions':sum(map(int,re.findall(r'; (\d+) ignored;',text))),'log_sha256':sha(path)}
    for binary in re.findall(r'Running [^\n]*\(([^\n)]+)\)',text):
        p=pathlib.Path(binary)
        if p.is_file():binaries[str(p)]={'sha256':sha(p),'bytes':p.stat().st_size}
receipt={'base':BASE,'historical_evidence_files':len(old),'historical_evidence_bytes':old_total,'owner_reproduction_files_preserved':len(owners()),'owner_snapshot_note':'End-of-implementation preservation snapshot; not claimed as a start-of-turn snapshot. These untracked owner directories were never written by this assignment.','source_files_bound':len(source),'native_states':len(r['runs']),'native_frames':sum(len(x['frames']) for x in r['runs']),'binary_sha256':r['binary_sha256'],'test_source_files_bound':len(tests()),'compiled_test_executables':binaries,'commands':command_results,'total_passing_test_executions':sum(x['passed_executions'] for x in command_results.values()),'note':'Execution count includes repeated suites in native/fallback configurations, not unique tests. No presented/compositor, physical input, real IME or live AT proof.'}
write(OUT/'audit-receipt.json',receipt)
print(json.dumps({k:v for k,v in receipt.items() if k not in ['compiled_test_executables','commands']},indent=2))
print('COMMAND COUNTS',json.dumps({k:v['passed_executions'] for k,v in command_results.items()}))
