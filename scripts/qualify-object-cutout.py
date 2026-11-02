#!/usr/bin/env python3
"""Actual isolated learned cutout; only an owned synthetic transparent PNG."""
import argparse,hashlib,json,os,selectors,signal,struct,subprocess,tempfile,time,zlib
from pathlib import Path
p=argparse.ArgumentParser()
for name in ('executable','model','runtime','artifacts'):p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();root=Path(tempfile.mkdtemp(prefix='object-cutout-',dir=a.artifacts.resolve()));root.chmod(0o700)
w,h=37,23
rgba=bytes(value for n in range(w*h) for value in (n%151,(n*17)%180,(n*31)%199,n%256))
def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
rows=b''.join(b'\0'+rgba[y*w*4:(y+1)*w*4] for y in range(h))
png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(rows))+chunk(b'IEND',b'')
source=root/'synthetic.png';source.write_bytes(png);source.chmod(0o600);output=root/'cutout.png';request=root/'request.json'
payload=dict(schemaVersion=3,model=str(a.model.resolve()),runtimeLibrary=str(a.runtime.resolve()),input=str(source),output=str(output),sourceSha256=hashlib.sha256(png).hexdigest())
def write(value):request.write_text(json.dumps(value));request.chmod(0o600)
def command():return [str(a.executable.resolve()),'--object',str(request)]
def invoke(value):write(value);return subprocess.run(command(),capture_output=True,env={},timeout=120)
def decode(data):
 assert data[:8]==b'\x89PNG\r\n\x1a\n';width,height=struct.unpack('>II',data[16:24]);assert data[24:26]==bytes([8,6]);offset=8;compressed=b''
 while offset<len(data):
  n=struct.unpack('>I',data[offset:offset+4])[0]
  if data[offset+4:offset+8]==b'IDAT':compressed+=data[offset+8:offset+8+n]
  offset+=n+12
 values=zlib.decompress(compressed);stride=width*4;previous=bytearray(stride);pixels=bytearray();offset=0
 for _ in range(height):
  kind=values[offset];offset+=1;row=bytearray(values[offset:offset+stride]);offset+=stride
  for i in range(stride):
   left=row[i-4] if i>=4 else 0;up=previous[i];corner=previous[i-4] if i>=4 else 0
   if kind==0:predict=0
   elif kind==1:predict=left
   elif kind==2:predict=up
   elif kind==3:predict=(left+up)//2
   elif kind==4:
    q=left+up-corner;distance=[abs(q-left),abs(q-up),abs(q-corner)];predict=[left,up,corner][distance.index(min(distance))]
   else:raise AssertionError('invalid PNG filter')
   row[i]=(row[i]+predict)&255
  pixels.extend(row);previous=row
 return width,height,bytes(pixels)
completed=invoke(payload);assert completed.returncode==0,completed.stderr.decode(errors='replace');metadata=json.loads(completed.stdout);encoded=output.read_bytes();width,height,pixels=decode(encoded);assert(width,height)==(w,h)
assert metadata['schemaVersion']==3 and metadata['maskWidth']==metadata['maskHeight']==512 and metadata['blankMask'] is False
assert metadata['sourceHash']==payload['sourceSha256'] and metadata['outputHash']==hashlib.sha256(encoded).hexdigest()
assert metadata['modelHash']==hashlib.sha256(a.model.read_bytes()).hexdigest() and metadata['runtimeHash']==hashlib.sha256(a.runtime.read_bytes()).hexdigest()
progress=[json.loads(line) for line in completed.stderr.splitlines() if line.startswith(b'{')]
assert [item['completed'] for item in progress]==[0,1,2] and all(item['total']==2 for item in progress)
assert output.stat().st_mode&0o777==0o600
for i in range(0,len(rgba),4):assert pixels[i:i+3]==rgba[i:i+3] and pixels[i+3]<=rgba[i+3]
assert any(pixels[i]<rgba[i] for i in range(3,len(rgba),4));assert any(0<pixels[i]<255 for i in range(3,len(rgba),4))
assert invoke(payload).returncode!=0 and output.read_bytes()==encoded
for mode in ('cancel','changed'):
 target=root/(mode+'.png');write({**payload,'output':str(target)})
 process=subprocess.Popen(command(),stdout=subprocess.PIPE,stderr=subprocess.PIPE,env={},start_new_session=True)
 try:
  with selectors.DefaultSelector() as selector:
   selector.register(process.stderr,selectors.EVENT_READ);deadline=time.monotonic()+60
   while True:
    assert time.monotonic()<deadline
    if not selector.select(1):assert process.poll() is None;continue
    line=process.stderr.readline()
    if line.startswith(b'{') and json.loads(line).get('event')=='progress':
     assert json.loads(line)['completed']==0
     if mode=='cancel':os.killpg(process.pid,signal.SIGTERM)
     else:source.write_bytes(png+b'changed during inference')
     process.communicate(timeout=120);assert process.returncode!=0;break
 finally:
  if process.poll() is None:os.killpg(process.pid,signal.SIGKILL);process.communicate(timeout=10)
 source.write_bytes(png);assert not target.exists()
for update in ({'sourceSha256':'0'*64},{'url':'https://invalid'}):
 target=root/'refused.png';assert invoke({**payload,**update,'output':str(target)}).returncode!=0;assert not target.exists()
assert not list(root.glob('.enhance-*'))
report=dict(root=str(root),metadata=metadata,rgbUnchanged=True,originalAlphaMultiplied=True,cancelledNoPublication=True,changedSourceRefused=True,noOverwrite=True)
(root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report))
