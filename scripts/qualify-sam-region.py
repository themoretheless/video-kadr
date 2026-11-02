#!/usr/bin/env python3
"""Actual pinned SlimSAM region protocol, synthetic media only, no environment paths."""
import argparse,hashlib,json,selectors,signal,struct,subprocess,tempfile,time,zlib
from pathlib import Path
p=argparse.ArgumentParser()
for name in ('executable','source','encoder','decoder','runtime','artifacts'):p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();root=Path(tempfile.mkdtemp(prefix='sam-region-',dir=a.artifacts.resolve()));root.chmod(0o700)
source=root/'source.png';source.write_bytes(a.source.read_bytes());source.chmod(0o600);original=source.read_bytes();request=root/'request.json'
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

payload=dict(schemaVersion=5,encoder=str(a.encoder.resolve()),decoder=str(a.decoder.resolve()),runtimeLibrary=str(a.runtime.resolve()),input=str(source),output=str(root/'region.png'),sourceSha256=hashlib.sha256(original).hexdigest(),points=[[0.5,0.5]])
def write(value):request.write_text(json.dumps(value));request.chmod(0o600)
def command():return [str(a.executable.resolve()),'--region',str(request)]
def invoke(value):write(value);return subprocess.run(command(),capture_output=True,env={},timeout=60)
results={}
for name,points in [('center',[[0.5,0.5]]),('path',[[i/39,0.3] for i in range(40)])]:
 output=root/(name+'.png');value={**payload,'output':str(output),'points':points};result=invoke(value);assert result.returncode==0,result.stderr.decode(errors='replace');meta=json.loads(result.stdout);w,h,raw=decode(output.read_bytes());assert(w,h)==(256,256) and len(raw)==65536
 assert meta['maskHash']==hashlib.sha256(raw).hexdigest() and meta['outputHash']==hashlib.sha256(output.read_bytes()).hexdigest();assert meta['sourceHash']==payload['sourceSha256'];assert output.stat().st_mode&0o777==0o600
 assert [json.loads(line)['completed'] for line in result.stderr.splitlines() if line.startswith(b'{')]==[0,1,2,3]
 assert invoke(value).returncode!=0
 results[name]=dict(metadata=meta,minimum=min(raw),maximum=max(raw),uniqueValues=len(set(raw)))
assert all(row['uniqueValues']>1 for row in results.values())
assert results['center']['metadata']['maskHash']!=results['path']['metadata']['maskHash']
for name,patch in [('stale',{'sourceSha256':'0'*64}),('empty',{'points':[]}),('outside',{'points':[[2,0]]}),('model',{'encoder':str(source)})]:
 output=root/(name+'.png');result=invoke({**payload,'output':str(output),**patch});assert result.returncode!=0 and not output.exists(),name
for mode in ('cancel','source-change'):
 output=root/(mode+'.png');write({**payload,'output':str(output)});process=subprocess.Popen(command(),stdout=subprocess.PIPE,stderr=subprocess.PIPE,env={});selector=selectors.DefaultSelector();selector.register(process.stderr,selectors.EVENT_READ)
 try:
  end=time.monotonic()+20;observed=False
  while time.monotonic()<end:
   if not selector.select(0.2):continue
   line=process.stderr.readline()
   if not line:break
   if line.startswith(b'{') and json.loads(line).get('completed')==0:
    observed=True
    if mode=='cancel':process.send_signal(signal.SIGTERM)
    else:source.write_bytes(original+b'changed')
    break
  assert observed;stdout,stderr=process.communicate(timeout=60);assert process.returncode!=0 and not output.exists(),(mode,stdout,stderr)
  if mode=='source-change':assert b'SAM source/model/runtime changed' in stderr,stderr
 finally:
  selector.close()
  if process.poll() is None:process.kill();process.wait()
  source.write_bytes(original)
assert not list(root.glob('.enhance-*'))
(root/'report.json').write_text(json.dumps(results,indent=2));print('PASS actual SlimSAM region, PNG/raw hashes, private no-replace output, strict admission, source mutation and reaped SIGTERM:',root/'report.json')
