"""Independent reviewer adversarial local-client checks; synthetic data only."""
import sys, os, tempfile, time, json, threading
from pathlib import Path
from unittest import mock
sys.path.insert(0,'tools')
import observe_client as c
with tempfile.TemporaryDirectory() as t:
 root=Path(t); os.chmod(root,0o700); path=root/'session.json'
 def write(data):
  path.write_bytes(data); os.chmod(path,0o600)
 write(b'{"old":true}')
 read=os.read
 def replace(fd,n):
  path.unlink(); write(b'{"new":true}'); return read(fd,n)
 # Pinned fd must read old opened inode, not the replacement at the same name.
 with mock.patch.object(c.os,'read',side_effect=replace):
  try: result=c._descriptor(path,time.monotonic()+1)
  except ValueError: pass  # unlink changes old fd ctime on this filesystem: conservative denial
  else: assert result=={'old':True},result
 print('PASS descriptor path replacement cannot redirect opened fd')
 write(b'{"old":true}')
 def rewrite(fd,n):
  data=read(fd,n)
  path.write_bytes(b'{"new":true}')
  now=time.time_ns()+1000000000; os.utime(path,ns=(now,now))
  return data
 with mock.patch.object(c.os,'read',side_effect=rewrite):
  try: c._descriptor(path,time.monotonic()+1)
  except ValueError: pass
  else: raise AssertionError('same-size mutation accepted')
 print('PASS same-size fd mutation detected')
 write(b'{}')
 def late(fd,n):
  time.sleep(.015); return read(fd,n)
 with mock.patch.object(c.os,'read',side_effect=late), mock.patch.object(c.socket,'socket') as sock:
  try: c.observe(path,timeout=.002)
  except TimeoutError: pass
  else: raise AssertionError('late read admitted')
  sock.assert_not_called()
 print('PASS expired descriptor never starts socket')
 link=root/'dir-link'; link.symlink_to(root,target_is_directory=True)
 try: c._descriptor(link/'session.json',time.monotonic()+1)
 except OSError: pass
 else: raise AssertionError('final directory symlink followed')
 print('PASS final directory symlink denied')
 for i in range(20):
  prefix=root/('pair'+str(i)); barrier=threading.Barrier(4); wins=[]; errors=[]
  def publish(v):
   barrier.wait()
   try: c.save_frame(prefix,{'id':v},str(v).encode()); wins.append(v)
   except FileExistsError: pass
   except BaseException as e: errors.append(repr(e))
  threads=[threading.Thread(target=publish,args=(v,)) for v in range(4)]
  for th in threads: th.start()
  for th in threads: th.join(2); assert not th.is_alive()
  assert not errors and len(wins)==1,(errors,wins)
  assert prefix.with_suffix('.png').read_bytes()==str(wins[0]).encode()
  assert json.loads(prefix.with_suffix('.json').read_bytes())=={'id':wins[0]}
 assert not list(root.glob('.rndr-frame-*'))
 print('PASS 20 four-way same-prefix publication races; exactly one coherent winner each, no staging leftovers')
print('PASS 5 independent client probe groups')
