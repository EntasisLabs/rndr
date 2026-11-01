"""Independent narrow RGBA8 noninterlaced PNG decoder and frame binding audit.
No dependency on the author's/reviewer's PNG checker implementations.
"""
from pathlib import Path
import struct,zlib,hashlib,json
sha=lambda b:hashlib.sha256(b).hexdigest()
def decode(p):
 b=p.read_bytes(); assert b[:8]==b'\x89PNG\r\n\x1a\n'; pos=8; compressed=b''; header=None; ended=False
 while pos<len(b):
  size,=struct.unpack_from('>I',b,pos); kind=b[pos+4:pos+8]; data=b[pos+8:pos+8+size]
  crc,=struct.unpack_from('>I',b,pos+8+size); assert zlib.crc32(kind+data)&0xffffffff==crc
  pos+=12+size
  if kind==b'IHDR':
   assert header is None; header=struct.unpack('>IIBBBBB',data)
  elif kind==b'IDAT': compressed+=data
  elif kind==b'IEND': assert not data; ended=True; break
 assert ended and pos==len(b)
 w,h,depth,color,compression,filtering,interlace=header
 assert (depth,color,compression,filtering,interlace)==(8,6,0,0,0)
 raw=zlib.decompress(compressed); stride=w*4; assert len(raw)==h*(stride+1)
 rows=[]; previous=bytearray(stride)
 for y in range(h):
  offset=y*(stride+1); f=raw[offset]; assert f in range(5); row=bytearray(raw[offset+1:offset+stride+1])
  for x in range(stride):
   left=row[x-4] if x>=4 else 0; up=previous[x]; corner=previous[x-4] if x>=4 else 0
   if f==0: predictor=0
   elif f==1: predictor=left
   elif f==2: predictor=up
   elif f==3: predictor=(left+up)//2
   else:
    base=left+up-corner; distances=[abs(base-left),abs(base-up),abs(base-corner)]
    predictor=[left,up,corner][distances.index(min(distances))]
   row[x]=(row[x]+predictor)&255
  rows.append(bytes(row)); previous=row
 return w,h,b''.join(rows)
base=Path('evidence'); binary='ce544034520d1a8cc6d9af538ffa0f3502d0438303f84240e55e8813f3e44e09'
assert sha(Path('target/debug/examples/ui_gallery').read_bytes())==binary
for mode,want_w,want_hash in [('light',1000,'7921fa8dc92de1336b06eca2e8c79773fd48a99efd5113c289a83121991427d3'),('narrow',320,'c704670129dac30925b25e51d15ed038b4e44696aa9b13d3762d0799ecd3b209')]:
 for folder,stem in [('native-observation-v1-correction','native-final-'),('native-observation-v1-rereview','native-')]:
  root=base/folder/(stem+mode); result=json.loads((root/'result.json').read_bytes())
  assert result['binary_sha256']==binary and result['status']=='captured'
  assert result['exit_code']==0 and result['normal_shutdown_cleanup'] and result['invalid_auth_rejection'] and result['stale_rejection']
  revisions=[]
  for i in [1,2]:
   png=root/('frame-'+str(i)+'.png'); meta=json.loads(png.with_suffix('.json').read_bytes()); w,h,rgba=decode(png)
   assert (w,h)==(want_w,700)==(meta['width'],meta['height'])
   assert sha(rgba)==want_hash; assert meta['png_bytes']==len(png.read_bytes())
   assert meta['source']=='live-surface-copy-before-present' and meta['presented_confirmation'] is False
   assert meta['device_scale']==1 and meta['render_provenance']['text_engine']=='builtin_atlas'
   assert meta['render_provenance']['path']=='quads' and meta['render_provenance']['parley_feature'] is False
   nodes=meta['nodes']; ids={n['id'] for n in nodes}; assert len(ids)==len(nodes)==28 and meta['root'] in ids
   for n in nodes:
    assert set(n['children'])<=ids; assert set(n['actions'])<={'focus','activate','set_value'}
   revisions.append(int(meta['frame_revision']))
   print('CRC/DECODE/TREE/BINDING PASS',png,'raw',sha(rgba))
  assert revisions[1]>revisions[0] and [str(n) for n in revisions]==result['revisions']
print('PASS eight sealed/fresh PNGs; exact binary hash verified. NO compositor or visual quality inference.')
