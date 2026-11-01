import pathlib,hashlib,sys
p=pathlib.Path(sys.argv[1]).resolve();m=p/'MANIFEST.sha256'
assert not m.exists(), 'refuse to overwrite an existing seal'
files=sorted(f for f in p.rglob('*') if f.is_file())
m.write_text(''.join(hashlib.sha256(f.read_bytes()).hexdigest()+'  '+str(f.relative_to(p))+'\n' for f in files))
for row in m.read_text().splitlines():
 h,n=row.split('  ',1);assert hashlib.sha256((p/n).read_bytes()).hexdigest()==h,n
print('rows',len(files));print('manifest_sha256',hashlib.sha256(m.read_bytes()).hexdigest())
for n in ['REVIEW.md','COMMANDS.md']:print(n,hashlib.sha256((p/n).read_bytes()).hexdigest())
