"""Independent, stdlib-only seal/PNG/tree checks. No capability reads."""
from pathlib import Path
import hashlib, struct, zlib, json, subprocess, re
base=Path('evidence/native-observation-v1'); review=Path('evidence/native-observation-v1-review')
sha=lambda b:hashlib.sha256(b).hexdigest()
m=base/'MANIFEST.sha256'; rows=m.read_text().splitlines()
assert len(rows)==111 and sha(m.read_bytes())=='9034ec3ec28db8aa007ccd6faef65ba5139368e9fb0a549e5a4cbe7d2360b30a'
for line in rows:
    h,name=line.split(None,1); name=name.lstrip('*'); p=Path(name)
    if not p.exists(): p=base/name
    assert sha(p.read_bytes())==h, name
assert subprocess.check_output(['git','diff','ff397cd9','f6648f16'])==(base/'implementation.diff').read_bytes()
assert not subprocess.check_output(['git','diff','f6648f16','1079de03','--',*['Cargo.lock','crates','tools','docs']])
print('PASS: exact 111 payload manifest, full implementation.diff, evidence-only seal')
for root in [base/'final-native-light',base/'final-native-narrow',review/'native-light',review/'native-narrow']:
    for p in sorted(root.glob('frame-*.png')):
        data=p.read_bytes(); assert data[:8]==b'\x89PNG\r\n\x1a\n'
        pos=8; comp=b''; header=None; ended=False
        while pos<len(data):
            n=struct.unpack_from('>I',data,pos)[0]; kind=data[pos+4:pos+8]; content=data[pos+8:pos+8+n]
            assert len(content)==n and zlib.crc32(kind+content)==struct.unpack_from('>I',data,pos+8+n)[0]
            pos+=n+12
            if kind==b'IHDR': header=struct.unpack('>IIBBBBB',content)
            elif kind==b'IDAT': comp+=content
            elif kind==b'IEND': ended=True; break
        assert ended and pos==len(data)
        w,h,depth,color,compress,filtermethod,interlace=header
        assert (depth,color,compress,filtermethod,interlace)==(8,6,0,0,0)
        inflated=zlib.decompress(comp); stride=w*4
        assert len(inflated)==(stride+1)*h
        prior=bytearray(stride); rgba=bytearray()
        for y in range(h):
            at=y*(stride+1); kind=inflated[at]; row=bytearray(inflated[at+1:at+1+stride]); assert kind in range(5)
            for i in range(stride):
                a=row[i-4] if i>=4 else 0; b=prior[i]; c=prior[i-4] if i>=4 else 0
                if kind==0: v=0
                elif kind==1: v=a
                elif kind==2: v=b
                elif kind==3: v=(a+b)//2
                else:
                    q=a+b-c; da,db,dc=abs(q-a),abs(q-b),abs(q-c)
                    v=a if da<=db and da<=dc else b if db<=dc else c
                row[i]=(row[i]+v)&255
            rgba+=row; prior=row
        meta=json.loads(p.with_suffix('.json').read_bytes())
        assert (w,h)==(meta['width'],meta['height']) and len(data)==meta['png_bytes']
        assert meta['source']=='live-surface-copy-before-present' and meta['presented_confirmation'] is False
        assert meta['device_scale']==1 and meta['render_provenance']['path']=='quads'
        ids={n['id'] for n in meta['nodes']}; assert len(ids)==len(meta['nodes'])==28 and meta['root'] in ids
        for n in meta['nodes']:
            assert set(n['children'])<=ids
            assert set(n['actions'])<={'focus','activate','set_value'}
        print('PNG',str(p),w,h,'nodes',len(ids),'unique_rgba',len({bytes(rgba[i:i+4]) for i in range(0,len(rgba),4)}),'raw_sha256',sha(rgba))
for name in ['suites.log','python.log','disabled.log']:
    text=(review/name).read_text()
    if name!='python.log':
        groups=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',text)
        assert groups
        print(name,'groups',len(groups),'passed_failed_ignored',[sum(int(g[i]) for g in groups) for i in range(3)])
    else: assert 'Ran 8 tests' in text and '\nOK\n' in text; print(name,'8 tests PASS')
for root in [review/'native-light',review/'native-narrow']:
    r=json.loads((root/'result.json').read_bytes())
    assert r['status']=='captured' and r['exit_code']==0 and r['normal_shutdown_cleanup']
    assert r['invalid_auth_rejection'] and r['stale_rejection']
    assert r['binary_sha256']=='b96be3081726d73afb683466fa10e02555370c3ebb159311e8fa900c025c9263'
    print(root,'PASS auth/stale/cleanup; executable matches seal')
