import ast, hashlib, pathlib, re, subprocess
p=pathlib.Path('docs/reports/rndr-flow-f4-independent-evidence')
head=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
parent=subprocess.check_output(['git','rev-parse','HEAD^'],text=True).strip()
assert head=='9e09c575f222384d5e3c843511552f64d0cbbf4d'
assert parent=='f16e5572d35450d586dd4f10e00591fa36fb2b04'
print('Exact candidate:',head,'parent:',parent)
recorded=pathlib.Path('docs/reports/rndr-flow-f4-evidence/source-digests.txt').read_text()
count=0
for line in recorded.splitlines():
    m=re.match(r'^([a-f0-9]{64})\s+(.+)$',line)
    if not m: continue
    digest,path=m.groups(); data=pathlib.Path(path).read_bytes()
    assert hashlib.sha256(data).hexdigest()==digest,path
    assert data==subprocess.check_output(['git','show',f'{head}:{path}']),path
    count+=1
assert count==20,count
print('All 20 worker-recorded fingerprints match exact HEAD and current source')
assert not subprocess.check_output(['git','diff','HEAD','--','crates','Cargo.toml','Cargo.lock'])
assert not subprocess.check_output(['git','diff',parent,head,'--','crates/rndr_core','crates/rndr','crates/rndr_liquid_ui/src'])
print('Candidate source unchanged; protected production paths unchanged from F3')
flow=(p/'flow.txt').read_text(); ui=(p/'consumer.txt').read_text()
seen=set()
for seed,cycle,scope,ack in re.findall(r'F4 seed=(\d+) cycle=(\d+) charged=0 handles=0 retired_unack=0 scopes=(\d+) cleanups=0 queued=0 acknowledged=(\d+)',flow):
    seed,cycle,scope,ack=map(int,(seed,cycle,scope,ack))
    assert scope==2 and ack==cycle*2
    assert (seed,cycle) not in seen
    seen.add((seed,cycle))
assert seen=={(s,c) for s in (1,7,42) for c in range(1,121)}
print('Fresh F4: 360 unique complete cycles; 720 cumulative task/feed exit acknowledgments; Counts/WorkCounts equalities verified in test body')
seen=set()
for seed,cycle,vec,base,ack in re.findall(r'F1/F2 teardown seed=(\d+) cycle=(\d+) vector=(\[[^\]]*\]) baseline=(\[[^\]]*\]) external_ack_pending=(\d+)',ui):
    v,b=ast.literal_eval(vec),ast.literal_eval(base)
    assert len(v)==len(b)==19 and v==b and int(ack)==0
    key=(int(seed),int(cycle)); assert key not in seen; seen.add(key)
assert seen=={(s,c) for s in (1,7,42) for c in range(100)}
print('Fresh F1/F2: 300 unique teardowns, all 19 vector elements equal baseline, zero pending external acknowledgments each time')
print('NOTE: fresh 20-gate runner records successful candidate tests; adversarial probes deliberately fail and are not part of that inherited matrix')
