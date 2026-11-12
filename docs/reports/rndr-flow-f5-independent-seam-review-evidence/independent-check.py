"""Review-owned checks, separate from the implementation auditor. No builds."""
from pathlib import Path
import subprocess,hashlib,csv,re,ast
out=Path('docs/reports/rndr-flow-f5-independent-seam-review-evidence');q=out/'qualification'
base='8c337328ea733cda18a0f7215bfe379b986ea646';prior='9721657f352bac8e22796a6e608ba2d293bbe498'
def git(*a):return subprocess.check_output(['git',*a])
assert git('rev-parse','main').decode().strip()=='bf56c67fd5b67ddf993ec5c02e5af4ef7ef507d5'
assert git('rev-parse',base+'^').decode().strip()=='da364e3acbf17192ab9a1041609453d0c8fc77f1'
assert git('merge-base','--is-ancestor',prior,base)==b''
paths=[p for p in git('ls-tree','-r','--name-only',base,'--','crates','Cargo.toml','Cargo.lock').decode().splitlines() if p.endswith(('.rs','.toml')) or p=='Cargo.lock']
recorded={p:d for d,p in (line.split('  ',1) for line in (q/'source-digests.txt').read_text().splitlines())}
assert len(paths)==len(set(paths))==len(recorded)==589 and set(paths)==set(recorded)
for p in paths:
    b=Path(p).read_bytes();assert hashlib.sha256(b).hexdigest()==recorded[p]
    assert b==git('show',f'{base}:{p}')==git('show',f'HEAD:{p}'),p
assert not git('diff',base,'--','crates','Cargo.toml','Cargo.lock')
assert not git('ls-files','--others','--exclude-standard','--','crates')
changed=set(git('diff','--name-only',prior,base,'--','crates').decode().splitlines())
assert changed=={'crates/rndr_render/src/lib.rs','crates/rndr_render/src/close.rs','crates/rndr_render/src/close_boundary_tests.rs','crates/rndr_render/tests/close_validator_api.rs','crates/rndr_flow/tests/f5_session_boundary.rs'}
for prefix in ('docs/reports/rndr-flow-f5-independent-evidence','docs/reports/rndr-flow-f5-rereview-evidence'):
    assert not git('diff',prior,'--',prefix)
expected=set('environment fmt whitespace flow host-adapter reviewer-probes preserved-f5-probes renderer renderer-api renderer-default lint-renderer core liquid consumer lint-flow lint-core lint-consumer feature-off facade-off facade-liquid compatibility-off compatibility-liquid flow-dependencies reverse-dependencies source-clean source-fingerprints audit'.split())
rows=list(csv.DictReader((q/'summary.tsv').open(),delimiter='\t'))
assert len(rows)==27 and {r['gate'] for r in rows}==expected and all(r['exit']=='0' for r in rows)
minimum=min(int(r[k]) for r in rows for k in ('free_kib_before','free_kib_after'))
assert minimum>=4456448
assert not (q/'disk-blocker.txt').exists() # one-second monitor records only a breach, not continuous samples
print('PASS exact reviewed candidate ancestry, main fixed, 589 complete source fingerprints, source unchanged, all original failed receipts/source bytes untouched')
print('PASS 27 unique expected zero-exit gates; endpoint minimum free_kib=',minimum,' (endpoints only; continuous samples not persisted)')
consumer=(q/'consumer.txt').read_text();flow=(q/'flow.txt').read_text()
rows=re.findall(r'F1/F2 teardown seed=(\d+) cycle=(\d+) vector=(\[[^\]]*\]) baseline=(\[[^\]]*\]) external_ack_pending=(\d+)',consumer)
assert len(rows)==300
for s,c,v,b,p in rows:
    v,b=ast.literal_eval(v),ast.literal_eval(b);assert len(v)==19 and v==b and int(p)==0
assert {(int(s),int(c)) for s,c,*_ in rows}=={(s,c) for s in (1,7,42) for c in range(100)}
for marker,count in [('F5 baseline',360),('F4 seed=',360),('F5-SEAM matrix',90)]:assert flow.count(marker)==count,(marker,flow.count(marker))
assert len(re.findall(r'^test budget_tests::\S+ \.\.\. ok$',flow,re.M))==9
assert 'acknowledged=240' in flow
probe=(q/'preserved-f5-probes.txt').read_text()
assert re.findall(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored',probe)==['13','4','9']
a=(out/'adversarial.txt').read_text()
assert 'test result: ok. 14 passed; 0 failed; 0 ignored' in a and 'exit=0' in a
assert hashlib.sha256((out/'adversarial.rs').read_bytes()).hexdigest() in a
for needle in ('coordinates=128','leaked_arc_owners=4','resource_destructors_during_removal=2'):assert needle in a
print('PASS independent raw F1/F2 full-vector 300 coordinates, F3 nine budgets, F4 360/720-ack and F5 360 baseline/90 seam rows; preserved 13/4/9; new7+original7 helper probes14')
print('KNOWN resource exception reproduced: four repeated opaque secondary panic payloads retain four Arc owners; no unconditional no-leak claim')
print('UNVERIFIED optional eighth real-native extra probe: below-floor preflight refusal after reviewer-only import fix; excluded from PASS totals')
