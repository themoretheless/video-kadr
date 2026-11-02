#!/usr/bin/env python3
"""Cached correction against retained source/mask; executable only, empty environment."""
import argparse,hashlib,json,os,selectors,signal,struct,subprocess,tempfile,time,zlib
from pathlib import Path
p=argparse.ArgumentParser()
for name in ('executable','source','mask','artifacts'):p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();root=Path(tempfile.mkdtemp(prefix='cached-mask-',dir=a.artifacts.resolve()));root.chmod(0o700)
source=root/'source.png';source.write_bytes(a.source.read_bytes());source.chmod(0o600)
mask=root/'mask.png';mask.write_bytes(a.mask.read_bytes());mask.chmod(0o600)
source_bytes=source.read_bytes();mask_bytes=mask.read_bytes();request=root/'request.json'
def decode(data):
 assert data[:8]==b'\x89PNG\r\n\x1a\n';width,height=struct.unpack('>II',data[16:24]);assert data[24]==8 and data[25] in (0,6);channels=1 if data[25]==0 else 4;offset=8;compressed=b''
 while offset<len(data):
  n=struct.unpack('>I',data[offset:offset+4])[0]
  if data[offset+4:offset+8]==b'IDAT':compressed+=data[offset+8:offset+8+n]
  offset+=n+12
 values=zlib.decompress(compressed);stride=width*channels;previous=bytearray(stride);pixels=bytearray();offset=0
 for _ in range(height):
  kind=values[offset];offset+=1;row=bytearray(values[offset:offset+stride]);offset+=stride
  for i in range(stride):
   left=row[i-channels] if i>=channels else 0;up=previous[i];corner=previous[i-channels] if i>=channels else 0
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

w,h,rgba=decode(source_bytes);mw,mh,raw=decode(mask_bytes);assert (mw,mh)==(512,512)
payload=dict(schemaVersion=4,input=str(source),mask=str(mask),output=str(root/'default.png'),sourceSha256=hashlib.sha256(source_bytes).hexdigest(),maskFileSha256=hashlib.sha256(mask_bytes).hexdigest(),maskSha256=hashlib.sha256(raw).hexdigest())
def write(value):request.write_text(json.dumps(value));request.chmod(0o600)
def command():return [str(a.executable.resolve()),'--mask',str(request)]
def invoke(value):write(value);return subprocess.run(command(),capture_output=True,env={},timeout=30)
rows=[dict(tool='brush',size=0.5,points=[[0.,y/4],[1.,y/4]],at=0.) for y in range(5)]
results={}
for name,strokes,feather in [('keep',rows,0.),('erase',[{**r,'tool':'eraser'} for r in rows],0.),('soft',[dict(tool='brush',size=0.2,points=[[0.5,0.5]])],0.01)]:
 output=root/(name+'.png');correction=dict(custom=True,strokes=strokes,feather=feather)
 value={**payload,'output':str(output),'correction':correction};result=invoke(value);assert result.returncode==0,result.stderr.decode(errors='replace')
 meta=json.loads(result.stdout);ow,oh,pixels=decode(output.read_bytes());assert (ow,oh)==(w,h)
 assert all(pixels[n*4:n*4+3]==rgba[n*4:n*4+3] and pixels[n*4+3]<=rgba[n*4+3] for n in range(w*h))
 if name=='keep':assert pixels==rgba
 if name=='erase':assert all(pixels[i]==0 for i in range(3,len(pixels),4))
 if name=='soft':assert any(0<pixels[i]<rgba[i] for i in range(3,len(pixels),4))
 assert meta['maskFileHash']==payload['maskFileSha256'] and meta['maskHash']==payload['maskSha256'] and meta['sourceHash']==payload['sourceSha256']
 assert meta['outputHash']==hashlib.sha256(output.read_bytes()).hexdigest() and output.stat().st_mode&0o777==0o600
 assert [json.loads(line)['completed'] for line in result.stderr.splitlines() if line.startswith(b'{')]==[0,1,2]
 assert invoke(value).returncode!=0
 results[name]=meta
# Bounded but long enough to observe a genuine in-flight edit.
slow=dict(custom=True,strokes=[dict(tool='brush',size=0.5,points=[[0.5,0.5]]*500)],feather=0.)
for mode in ('cancel','source-change','mask-change'):
 output=root/(mode+'.png');value={**payload,'output':str(output),'correction':slow};write(value)
 process=subprocess.Popen(command(),stdout=subprocess.PIPE,stderr=subprocess.PIPE,env={});selector=selectors.DefaultSelector();selector.register(process.stderr,selectors.EVENT_READ)
 try:
  end=time.monotonic()+10;observed=False
  while time.monotonic()<end:
   if not selector.select(0.2):continue
   line=process.stderr.readline()
   if not line:break
   if line.startswith(b'{') and json.loads(line).get('completed')==0:
    observed=True
    if mode=='cancel':process.send_signal(signal.SIGTERM)
    else:
     target=source if mode=='source-change' else mask
     with target.open('ab') as stream:stream.write(b'changed')
    break
  assert observed
  stdout,stderr=process.communicate(timeout=30);assert process.returncode!=0 and not output.exists(),(mode,stdout,stderr)
  if mode!='cancel':assert b'correction source/mask changed' in stderr,(mode,stderr)
 finally:
  selector.close()
  if process.poll() is None:process.kill();process.communicate()
  source.write_bytes(source_bytes);mask.write_bytes(mask_bytes)
for key in ('sourceSha256','maskFileSha256','maskSha256'):
 value={**payload,key:'0'*64,'output':str(root/(key+'.png')),'correction':dict(custom=False,strokes=[],feather=0.)};assert invoke(value).returncode!=0 and not Path(value['output']).exists()
assert not list(root.glob('.enhance-*'))
(root/'report.json').write_text(json.dumps(results,indent=2));print('PASS cached native keep/erase/feather without model/runtime, source/mask mutations, SIGTERM, hashes/no-replace/private PNG:',root)
