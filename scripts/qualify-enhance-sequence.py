#!/usr/bin/env python3
"""Actual streaming learned sequence versus independent still-frame outputs."""
import argparse, hashlib, json, os, selectors, signal, struct, subprocess, tempfile, time, zlib
from pathlib import Path
p=argparse.ArgumentParser()
for name in ('executable','model','runtime','artifacts'): p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();root=Path(tempfile.mkdtemp(prefix='enhance-sequence-',dir=a.artifacts.resolve()));root.chmod(0o700)
w,h=257,9
frames=[bytes(value for y in range(h) for x in range(w) for value in ((x*13+y*7+ordinal*71)%256,(x*9+y*17+ordinal*37)%256,80+ordinal*50,255)) for ordinal in range(2)]
raw=root/'input.rgba';raw.write_bytes(b''.join(frames));raw.chmod(0o600)
def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
def png(rgba):
 rows=b''.join(b'\0'+rgba[y*w*4:(y+1)*w*4] for y in range(h))
 return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(rows))+chunk(b'IEND',b'')
def decode(data):
 width,height=struct.unpack('>II',data[16:24]);compressed=b'';offset=8
 while offset<len(data):
  n=struct.unpack('>I',data[offset:offset+4])[0];kind=data[offset+4:offset+8]
  if kind==b'IDAT':compressed+=data[offset+8:offset+8+n]
  offset+=12+n
 source=zlib.decompress(compressed);stride=width*4;previous=bytearray(stride);result=bytearray();offset=0
 for _ in range(height):
  kind=source[offset];offset+=1;row=bytearray(source[offset:offset+stride]);offset+=stride
  for i in range(stride):
   left=row[i-4] if i>=4 else 0;up=previous[i];corner=previous[i-4] if i>=4 else 0
   if kind==0:predict=0
   elif kind==1:predict=left
   elif kind==2:predict=up
   elif kind==3:predict=(left+up)//2
   elif kind==4:
    q=left+up-corner;dist=[abs(q-left),abs(q-up),abs(q-corner)];predict=[left,up,corner][dist.index(min(dist))]
   else:raise AssertionError('unsupported PNG filter')
   row[i]=(row[i]+predict)&255
  result.extend(row);previous=row
 return width,height,bytes(result)
request=root/'request.json'
base=dict(schemaVersion=2,model=str(a.model.resolve()),runtimeLibrary=str(a.runtime.resolve()),input=str(raw),sourceSha256=hashlib.sha256(raw.read_bytes()).hexdigest(),width=w,height=h,frames=2)
def invoke(payload,mode='--sequence',timeout=120):
 request.write_text(json.dumps(payload));request.chmod(0o600)
 return subprocess.run([str(a.executable.resolve()),mode,str(request)],env={},capture_output=True,timeout=timeout)
reports=[]
for factor in (1,2):
 output=root/f'output-{factor}.rgba';payload={**base,'factor':factor,'output':str(output)}
 completed=invoke(payload);assert completed.returncode==0,completed.stderr.decode(errors='replace')
 metadata=json.loads(completed.stdout);actual=output.read_bytes();ow=(w*factor)&~1;oh=(h*factor)&~1
 assert metadata['width']==ow and metadata['height']==oh and metadata['frames']==2
 assert metadata['bytes']==len(actual)==ow*oh*4*2 and metadata['outputHash']==hashlib.sha256(actual).hexdigest()
 assert metadata['sourceHash']==base['sourceSha256'] and metadata['modelHash']==hashlib.sha256(a.model.read_bytes()).hexdigest()
 assert metadata['runtimeHash']==hashlib.sha256(a.runtime.read_bytes()).hexdigest()
 expected=b''
 for ordinal,rgba in enumerate(frames):
  source=root/f'frame-{factor}-{ordinal}.png';source.write_bytes(png(rgba));target=root/f'still-{factor}-{ordinal}.png'
  still=invoke(dict(schemaVersion=1,model=base['model'],runtimeLibrary=base['runtimeLibrary'],input=str(source),output=str(target),sourceSha256=hashlib.sha256(source.read_bytes()).hexdigest(),factor=factor),'--request')
  assert still.returncode==0,still.stderr.decode(errors='replace')
  iw,ih,pixels=decode(target.read_bytes());assert (iw,ih)==(w*factor,h*factor)
  expected+=b''.join(pixels[y*iw*4:y*iw*4+ow*4] for y in range(oh))
 assert actual==expected,'streaming frames differ from independent still inference and top-left even crop'
 assert actual[:len(actual)//2]!=actual[len(actual)//2:],'fixture must distinguish frame order'
 progress=[json.loads(line) for line in completed.stderr.splitlines() if line.startswith(b'{')]
 assert [v['completed'] for v in progress]==[1,2,3,4] and all(v['total']==4 for v in progress)
 assert output.stat().st_mode&0o777==0o600
 assert invoke(payload).returncode!=0 and output.read_bytes()==actual
 reports.append(metadata)
# Cancel after a real first tile; parent owns crash staging cleanup after reap.
cancelled=root/'cancelled.rgba';payload={**base,'factor':2,'output':str(cancelled)};request.write_text(json.dumps(payload))
process=subprocess.Popen([str(a.executable.resolve()),'--sequence',str(request)],env={},stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
try:
 with selectors.DefaultSelector() as selector:
  selector.register(process.stderr,selectors.EVENT_READ);deadline=time.monotonic()+60
  while True:
   assert time.monotonic()<deadline
   if not selector.select(1):assert process.poll() is None;continue
   line=process.stderr.readline()
   if line.startswith(b'{') and json.loads(line).get('event')=='progress':
    os.killpg(process.pid,signal.SIGTERM);process.communicate(timeout=10);assert process.returncode!=0;break
finally:
 if process.poll() is None:os.killpg(process.pid,signal.SIGKILL);process.communicate(timeout=10)
assert not cancelled.exists()
for staged in root.glob(f'.enhance-sequence-{process.pid}-*.rgba'):staged.unlink()
for update in ({'sourceSha256':'0'*64},{'frames':3},{'url':'https://invalid'}):
 target=root/'refused.rgba';assert invoke({**payload,**update,'output':str(target)}).returncode!=0;assert not target.exists()
assert not list(root.glob('.enhance-*'))
report=dict(root=str(root),framesCompared=4,factors=[1,2],exactIndependentRgba=True,cancelledWithoutPublication=True,metadata=reports)
(root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report))
