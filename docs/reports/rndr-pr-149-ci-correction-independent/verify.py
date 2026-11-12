"""Independent read-only exact candidate, complete history and receipt check."""
from pathlib import Path
import subprocess, hashlib, csv, re, html
C='2b38dd72cbce034d15fcf5600d37761e38fb2e97'
S='e228c824618aec25b7f8657d1569472925486b24'
P='e89d0bff32ea94941063af11c496162b5fe5a261'
B='43a0e83394ac80a9ac35bcffde479bef4d71cfd1'
O=Path('docs/reports/rndr-pr-149-ci-correction')
def git(*a): return subprocess.check_output(['git',*a])
def blob(rev,p): return git('show',rev+':'+p)
def tree(rev):
    return {p.decode(): (mode,kind,oid) for line in git('ls-tree','-rz',rev).split(b'\0') if line for meta,p in [line.split(b'\t',1)] for mode,kind,oid in [meta.split()]}
tp,ts,tc=map(tree,(P,S,C))
assert git('rev-parse',C+'^').decode().strip()==S
assert git('rev-parse',S+'^').decode().strip()==P
assert {p for p in tc if tc[p]!=ts.get(p)} and all(p.startswith(str(O)+'/') and p not in ts for p in tc if tc[p]!=ts.get(p))
assert ts.keys()<=tc.keys()
renames={'crates/rndr_flow/src/lib.rs':1,'crates/rndr_flow/src/topology.rs':1,'crates/rndr_liquid_ui/src/lifecycle.rs':2}
tools={'docs/reports/rndr-pr-149-ci-audit.py','docs/reports/rndr-pr-149-ci-checks.sh'}
changed={p for p in tp if tp[p]!=tc.get(p)}
assert changed==set(renames)|tools
for p,n in renames.items():
    data=blob(P,p)
    assert data.count(b'.fetch_update(')==n
    assert data.replace(b'.fetch_update(',b'.try_update(')==blob(C,p)
assert not git('diff',C,'--',':!docs')
print('PASS exact ancestry; evidence-only candidate; ALL previously tracked paths preserved except 3 rename files / 2 authorized tools.')
for manifest,rev in [('docs/reports/rndr-flow-f5-independent-seam-review-evidence/qualification/source-digests.txt',B),(str(O/'source-digests.txt'),S)]:
    lines=Path(manifest).read_text().splitlines(); seen=set(); delta=[]
    for line in lines:
        digest,p=line.split('  ',1); assert p not in seen; seen.add(p)
        assert hashlib.sha256(blob(rev,p)).hexdigest()==digest
        if blob(B,p)!=blob(C,p): delta.append(p)
    assert len(seen)==589 and len(delta)==6
    print('PASS manifest',manifest, '589 Git matches; 583 baseline bytes unchanged / 6 changed.')
rs=[p for p in tc if p.endswith('.rs') and not p.startswith(('docs/','evidence/'))]
assert len(rs)==598 and all(b'fetch_update' not in blob(C,p) for p in rs)
print('PASS all 598 nonhistorical tracked Rust files: no fetch_update token.')
# Read EVERY correction receipt byte, including panic/lifecycle output; verify
# command/suffix/suite totals rather than infer success from summary alone.
rows=list(csv.DictReader((O/'checks/summary.tsv').open(),delimiter='\t'))
assert len(rows)==14 and len({r['gate'] for r in rows})==14
for r in rows:
    raw=(O/'checks'/ (r['gate']+'.txt')).read_bytes(); text=raw.decode()
    assert raw==blob(C,str(O/'checks'/(r['gate']+'.txt')))
    assert r['exit']=='0' and text.endswith('\nexit=0\n')
    assert min(int(r[k]) for k in ('free_kib_before','free_kib_after'))>=3145728
    assert not re.search(r'^error(?:\[|:)|^test result: FAILED',text,re.M)
    if r['gate'].startswith('clippy-'):
        assert text.splitlines()[0].endswith('-- -D warnings') and 'Finished `dev` profile' in text
    totals=re.findall(r'test result: ok\. (\d+) passed; 0 failed; (\d+) ignored',text)
    actual=tuple(sum(int(t[i]) for t in totals) for i in (0,1))
    expected={'flow':(144,0),'consumer':(115,2),'core-freeze':(3,0),'composer':(9,0)}
    if r['gate'] in expected: assert actual==expected[r['gate']]
    print('PASS complete receipt',r['gate'],len(raw),'bytes',actual if totals else '')
# Independently decode installed std integer macro, not the supplied excerpt.
doc=Path.home()/'.rustup/toolchains/stable-aarch64-apple-darwin/share/doc/rust/html/src/core/sync/atomic.rs.html'
raw=doc.read_bytes(); text=html.unescape(re.sub('<[^>]*>','',raw.decode()))
a=next(m.start() for m in re.finditer(r'pub fn try_update\(',text) if '$int_type' in text[m.start():m.start()+450]); excerpt=text[a-400:a+800]
assert '#[stable(feature = "atomic_try_update", since = "1.95.0")]' in excerpt
f=next(m.start() for m in re.finditer(r'pub fn fetch_update<F>',text) if '$int_type' in text[m.start():m.start()+450]); forward=text[f:f+450]
assert 'self.try_update(set_order, fetch_order, f)' in forward
assert 'compare_exchange_weak(prev, next, set_order, fetch_order)' in excerpt
print('PASS actual integer try_update stable 1.95.0; fetch_update forwards; checked CAS body.')
print('Installed doc SHA256',hashlib.sha256(raw).hexdigest())
print(excerpt); print(forward)
print('LIMIT metadata + local 1.98 only; not Rust 1.95/1.100 execution or remote CI.')
