from pathlib import Path
import hashlib,subprocess,sys,json
p=Path(__file__).resolve().parent
expected='91c008bdc1054f42f67dcd5d45729290ddb4ffa4'
subprocess.run(['git','diff','--exit-code',expected,'--','crates','Cargo.toml','Cargo.lock','docs/design'],check=True,stdout=subprocess.DEVNULL)
rows=(p/'MANIFEST.sha256').read_text().splitlines();seen=set()
for line in rows:
 h,n=line.split(maxsplit=1)
 assert n not in seen and not Path(n).is_absolute() and '..' not in Path(n).parts,n
 seen.add(n);f=p/n;assert f.is_file() and hashlib.sha256(f.read_bytes()).hexdigest()==h,n
actual={str(f.relative_to(p)) for f in p.rglob('*') if f.is_file() and f.name!='MANIFEST.sha256'}
# Nested original MANIFEST files are data and ARE included in outer manifest.
actual={str(f.relative_to(p)) for f in p.rglob('*') if f.is_file() and f!=p/'MANIFEST.sha256'}
assert actual==seen,(actual-seen,seen-actual)
for line in (p/'exact-ui-final/ui-source-after-sha256.txt').read_text().splitlines():
 h,n=line.split(maxsplit=1);assert hashlib.sha256(Path(n).read_bytes()).hexdigest()==h,n
for line in (p/'history-before.sha256').read_text().splitlines():
 h,n=line.split(maxsplit=1);assert hashlib.sha256(Path(n).read_bytes()).hexdigest()==h,n
print(json.dumps({'manifest_entries_verified':len(rows),'manifest_sha256':hashlib.sha256((p/'MANIFEST.sha256').read_bytes()).hexdigest(),'report_sha256':hashlib.sha256((p/'REPORT.md').read_bytes()).hexdigest(),'commands_sha256':hashlib.sha256((p/'COMMANDS.md').read_bytes()).hexdigest(),'tested_implementation':expected,'tested_source_unchanged':True,'historical_evidence_unchanged':True},indent=2))
