#!/usr/bin/env python3
"""Actual isolated learned cutout; only an owned synthetic transparent PNG."""
import argparse,hashlib,json,os,selectors,signal,struct,subprocess,tempfile,time,zlib
from pathlib import Path
p=argparse.ArgumentParser()
for name in ('executable','model','runtime','artifacts'):p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();root=Path(tempfile.mkdtemp(prefix='mask-edit-',dir=a.artifacts.resolve()));root.chmod(0o700)
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

def case(name,correction):
 output=root/(name+'.png');mask_output=root/(name+'-mask.png')
 value={**payload,'output':str(output),'maskOutput':str(mask_output),'correction':correction}
 completed=invoke(value);assert completed.returncode==0,completed.stderr.decode(errors='replace')
 metadata=json.loads(completed.stdout);mask_png=mask_output.read_bytes();mw,mh,mask=decode(mask_png)
 assert (mw,mh)==(512,512) and len(mask)==512*512
 assert hashlib.sha256(mask).hexdigest()==metadata['maskHash']
 assert hashlib.sha256(mask_png).hexdigest()==metadata['maskFileHash']
 assert hashlib.sha256(output.read_bytes()).hexdigest()==metadata['outputHash']
 ow,oh,pixels=decode(output.read_bytes());assert (ow,oh)==(w,h)
 assert all(pixels[n*4:n*4+3]==rgba[n*4:n*4+3] and pixels[n*4+3]<=rgba[n*4+3] for n in range(w*h))
 assert output.stat().st_mode&0o777==mask_output.stat().st_mode&0o777==0o600
 assert invoke(value).returncode!=0
 return metadata,pixels,mask
rows=[{'tool':'brush','size':0.5,'points':[[0.,y/4],[1.,y/4]],'at':0.} for y in range(5)]
kept,pixels,mask=case('kept',dict(custom=True,strokes=rows,feather=0.))
assert pixels==rgba and kept['resolvedMaskHash']==hashlib.sha256(bytes([255])*(512*512)).hexdigest()
erased,pixels,other=case('erased',dict(custom=True,strokes=[{**row,'tool':'eraser'} for row in rows],feather=0.))
assert mask==other and all(pixels[i]==0 for i in range(3,len(pixels),4))
assert erased['resolvedMaskHash']==hashlib.sha256(bytes(512*512)).hexdigest()
soft,pixels,other=case('soft',dict(custom=True,strokes=[dict(tool='brush',size=0.2,points=[[0.5,0.5]])],feather=0.01))
assert other==mask and any(0<pixels[i]<rgba[i] for i in range(3,len(pixels),4))
# Failed second publication removes only its newly owned mask.
failed={**payload,'output':str(root/'missing-parent'/'failure.png'),'maskOutput':str(root/'rollback.png'),'correction':dict(custom=False,strokes=[],feather=0.)}
assert invoke(failed).returncode!=0 and not (root/'rollback.png').exists()
invalid={**payload,'output':str(root/'invalid.png'),'maskOutput':str(root/'invalid-mask.png'),'correction':dict(custom=True,strokes=[],feather=0.11)}
assert invoke(invalid).returncode!=0 and not (root/'invalid.png').exists() and not (root/'invalid-mask.png').exists()
assert not list(root.glob('.enhance-*'))
report=dict(kept=kept,erased=erased,soft=soft)
(root/'report.json').write_text(json.dumps(report,indent=2));print('PASS native correction, raw grayscale mask, independent PNG/RGB/alpha/hash checks, full keep/erase, feather and rollback:',root)
