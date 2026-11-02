#!/usr/bin/env python3
"""Reapply a real retained SAM region with cached probabilities and no model/ORT."""
import argparse,hashlib,json,selectors,signal,struct,subprocess,tempfile,time,zlib
from pathlib import Path
p=argparse.ArgumentParser()
for name in ('executable','source','region','artifacts'):p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();root=Path(tempfile.mkdtemp(prefix='cached-regions-',dir=a.artifacts.resolve()));root.chmod(0o700)
source=root/'source.png';source.write_bytes(a.source.read_bytes());source.chmod(0o600);source_bytes=source.read_bytes();region=root/'region.png';region.write_bytes(a.region.read_bytes());region.chmod(0o600);region_bytes=region.read_bytes();request=root/'request.json'
def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
mask=root/'automatic.png';raw=bytes([128])*512*512;mask_bytes=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',512,512,8,0,0,0,0))+chunk(b'IDAT',zlib.compress(b''.join(b'\0'+raw[y*512:(y+1)*512] for y in range(512))))+chunk(b'IEND',b'');mask.write_bytes(mask_bytes);mask.chmod(0o600)
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

rw,rh,region_raw=decode(region_bytes);assert(rw,rh)==(256,256) and len(region_raw)==65536
source_width,source_height,source_rgba=decode(source_bytes)
region_input=dict(path=str(region),fileSha256=hashlib.sha256(region_bytes).hexdigest(),maskSha256=hashlib.sha256(region_raw).hexdigest())
settings=dict(custom=True,strokes=[dict(tool='smartBrush',size=0.005,points=[[i/39,0.3] for i in range(40)],at=None)],feather=0.)
payload=dict(schemaVersion=4,input=str(source),mask=str(mask),output=str(root/'default.png'),sourceSha256=hashlib.sha256(source_bytes).hexdigest(),maskFileSha256=hashlib.sha256(mask_bytes).hexdigest(),maskSha256=hashlib.sha256(raw).hexdigest(),correction=settings)
def write(value):request.write_text(json.dumps(value));request.chmod(0o600)
def command():return [str(a.executable.resolve()),'--mask',str(request)]
def invoke(value):write(value);return subprocess.run(command(),capture_output=True,env={},timeout=30)
def sample(x,y):
 x0=int(x//1);y0=int(y//1);fx=x-x0;fy=y-y0
 def at(x,y):return region_raw[max(0,min(255,y))*256+max(0,min(255,x))]
 top=at(x0,y0)*(1-fx)+at(x0+1,y0)*fx;bottom=at(x0,y0+1)*(1-fx)+at(x0+1,y0+1)*fx
 return (top*(1-fy)+bottom*fy)/255
expected=bytes(255 if sample((x+.5)/2-.5,(y+.5)/2-.5)>=.5 else 128 for y in range(512) for x in range(512))
assert expected!=raw
fallback=bytes(255 if abs(y+.5-.3*512)<=.005*(source_width/source_height)*512/2 else 128 for y in range(512) for x in range(512))
metadata={}
for name,regions in [('learned',[region_input]),('learned-erase',[region_input]),('fallback',None),('null',[None])]:
 output=root/(name+'.png');value={**payload,'output':str(output)}
 if regions is not None:value['regions']=regions
 if name=='learned-erase':value['correction']={**settings,'strokes':[{**settings['strokes'][0],'tool':'smartEraser'}]}
 result=invoke(value);assert result.returncode==0,result.stderr.decode(errors='replace');meta=json.loads(result.stdout);metadata[name]=meta
 w,h,rgba=decode(output.read_bytes());assert(w,h)==(source_width,source_height)
 assert all(rgba[n*4:n*4+3]==source_rgba[n*4:n*4+3] and rgba[n*4+3]<=source_rgba[n*4+3] for n in range(w*h))
 assert meta['resolvedMaskHash']==hashlib.sha256((bytes(0 if p==255 else 128 for p in expected) if name=='learned-erase' else expected) if name.startswith('learned') else fallback).hexdigest()
 if regions is None:assert 'regionHashes' not in meta
 else:assert meta['regionHashes']==[region_input['maskSha256'] if name.startswith('learned') else None]
 assert meta['outputHash']==hashlib.sha256(output.read_bytes()).hexdigest() and output.stat().st_mode&0o777==0o600
 assert invoke(value).returncode!=0
assert metadata['learned']['outputHash']!=metadata['fallback']['outputHash']
assert metadata['fallback']['outputHash']==metadata['null']['outputHash']
for name,patch in [('count',{'regions':[region_input,None]}),('stale-file',{'regions':[{**region_input,'fileSha256':'0'*64}]}),('stale-raw',{'regions':[{**region_input,'maskSha256':'0'*64}]}),('wrong-tool',{'regions':[region_input],'correction':{**settings,'strokes':[{**settings['strokes'][0],'tool':'brush'}]}})]:
 output=root/(name+'.png');result=invoke({**payload,'output':str(output),**patch});assert result.returncode!=0 and not output.exists(),name
output=root/'too-many.png';result=invoke({**payload,'output':str(output),'regions':[region_input]*1025,'correction':{**settings,'strokes':settings['strokes']*1025}});assert result.returncode!=0 and not output.exists()
for mode in ('cancel','region-change'):
 output=root/(mode+'.png');write({**payload,'output':str(output),'regions':[region_input]});process=subprocess.Popen(command(),stdout=subprocess.PIPE,stderr=subprocess.PIPE,env={});selector=selectors.DefaultSelector();selector.register(process.stderr,selectors.EVENT_READ)
 try:
  end=time.monotonic()+10;observed=False
  while time.monotonic()<end:
   if not selector.select(.1):continue
   line=process.stderr.readline()
   if not line:break
   if line.startswith(b'{') and json.loads(line).get('completed')==0:
    observed=True
    if mode=='cancel':process.send_signal(signal.SIGTERM)
    else:region.write_bytes(region_bytes+b'changed')
    break
  assert observed;stdout,stderr=process.communicate(timeout=30);assert process.returncode!=0 and not output.exists(),(mode,stdout,stderr)
  if mode=='region-change':assert b'correction region changed' in stderr,stderr
 finally:
  selector.close()
  if process.poll() is None:process.kill();process.wait()
  region.write_bytes(region_bytes)
assert not list(root.glob('.enhance-*'))
(root/'report.json').write_text(json.dumps(metadata,indent=2));print('PASS model-free learned region replay, exact independently reconstructed mask, fallback compatibility, source RGB/alpha, identity/mutation/cancel/no-replace:',root/'report.json')
