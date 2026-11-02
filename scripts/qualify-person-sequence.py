#!/usr/bin/env python3
"""Actual ordered RVM stream versus retained independent upstream-model/apply oracle."""
import argparse,hashlib,json,os,selectors,signal,subprocess,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser()
for name in ('executable','model','runtime','oracle','artifacts'):p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();root=Path(tempfile.mkdtemp(prefix='person-sequence-',dir=a.artifacts.resolve()));root.chmod(0o700)
ref=json.loads((a.oracle/'reference.json').read_text());original=(a.oracle/'input.rgba').read_bytes();expected=(a.oracle/'reference.rgba').read_bytes();masks=(a.oracle/'reference.masks').read_bytes()
assert hashlib.sha256(expected).hexdigest()==ref['outputHash'];assert hashlib.sha256(masks).hexdigest()==ref['maskHash']
source=root/'input.rgba';source.write_bytes(original);source.chmod(0o600);request=root/'request.json'
payload=dict(schemaVersion=7,model=str(a.model.resolve()),runtimeLibrary=str(a.runtime.resolve()),input=str(source),output=str(root/'output.rgba'),maskOutput=str(root/'output.masks'),sourceSha256=hashlib.sha256(original).hexdigest(),width=ref['width'],height=ref['height'],frames=ref['frames'])
def write(value):request.write_text(json.dumps(value));request.chmod(0o600)
def command():return [str(a.executable.resolve()),'--person-sequence',str(request)]
def invoke(value):write(value);return subprocess.run(command(),capture_output=True,env={},timeout=60)
result=invoke(payload);assert result.returncode==0,result.stderr.decode(errors='replace');metadata=json.loads(result.stdout);output=Path(payload['output']);mask=Path(payload['maskOutput']);assert output.read_bytes()==expected;assert mask.read_bytes()==masks
assert metadata['schemaVersion']==7 and metadata['width']==ref['width'] and metadata['height']==ref['height'] and metadata['frames']==ref['frames'];assert metadata['maskWidth']==ref['maskWidth'] and metadata['maskHeight']==ref['maskHeight'];assert metadata['bytes']==len(expected) and metadata['maskBytes']==len(masks)
assert metadata['outputHash']==ref['outputHash'] and metadata['maskHash']==ref['maskHash'];assert metadata['sourceHash']==payload['sourceSha256'];assert metadata['modelHash']==hashlib.sha256(a.model.read_bytes()).hexdigest();assert metadata['runtimeHash']==hashlib.sha256(a.runtime.read_bytes()).hexdigest()
stride=ref['maskWidth']*ref['maskHeight'];assert metadata['blankFrames']==sum(all(v<10 for v in masks[i:i+stride]) for i in range(0,len(masks),stride))
for i in range(0,len(original),4):assert expected[i:i+3]==original[i:i+3] and expected[i+3]<=original[i+3]
assert any(expected[i+3]<original[i+3] for i in range(0,len(original),4))
assert output.stat().st_mode&0o777==0o600 and mask.stat().st_mode&0o777==0o600
progress=[json.loads(line) for line in result.stderr.splitlines() if line.startswith(b'{')];assert [e['completed'] for e in progress]==list(range(ref['frames']+1));assert all(e['total']==ref['frames'] for e in progress)
assert invoke(payload).returncode!=0 and output.read_bytes()==expected and mask.read_bytes()==masks
replay={**payload,'output':str(root/'replay.rgba'),'maskOutput':str(root/'replay.masks')};assert invoke(replay).returncode==0;assert Path(replay['output']).read_bytes()==expected;assert Path(replay['maskOutput']).read_bytes()==masks
without_mask={k:v for k,v in payload.items() if k!='maskOutput'};without_mask['output']=str(root/'without-mask.rgba');result=invoke(without_mask);assert result.returncode==0,result.stderr.decode(errors='replace');assert Path(without_mask['output']).read_bytes()==expected;assert json.loads(result.stdout)['maskHash']==ref['maskHash']
for mode in ('cancel','changed','occupied','mask-occupied'):
 target=root/(mode+'.rgba');target_mask=root/(mode+'.masks');write({**payload,'output':str(target),'maskOutput':str(target_mask)});process=subprocess.Popen(command(),stdout=subprocess.PIPE,stderr=subprocess.PIPE,env={},start_new_session=True)
 try:
  with selectors.DefaultSelector() as selector:
   selector.register(process.stderr,selectors.EVENT_READ);deadline=time.monotonic()+30
   while True:
    assert time.monotonic()<deadline
    if not selector.select(1):assert process.poll() is None;continue
    line=process.stderr.readline()
    if line.startswith(b'{') and json.loads(line).get('event')=='progress':
     assert json.loads(line)['completed']==0
     if mode=='cancel':os.killpg(process.pid,signal.SIGTERM)
     elif mode=='changed':source.write_bytes(bytes([original[0]^255])+original[1:])
     elif mode=='occupied':target.write_bytes(b'occupied')
     else:target_mask.write_bytes(b'occupied mask')
     process.communicate(timeout=60);assert process.returncode!=0;break
 finally:
  if process.poll() is None:os.killpg(process.pid,signal.SIGKILL);process.communicate(timeout=10)
 source.write_bytes(original)
 if mode=='occupied':assert target.read_bytes()==b'occupied'
 else:assert not target.exists()
 if mode=='mask-occupied':assert target_mask.read_bytes()==b'occupied mask'
 else:assert not target_mask.exists()
 # SIGTERM cannot run Rust destructors. The owner cleans only this reaped child's staging.
 if mode=='cancel':
  for staged in root.glob(f'.person-sequence-{process.pid}-*'):staged.unlink()
for update in ({'schemaVersion':2},{'frames':0},{'frames':3},{'frames':360001},{'width':0},{'sourceSha256':'0'*64},{'url':'https://invalid'}):
 refused={**payload,**update,'output':str(root/'refused.rgba'),'maskOutput':str(root/'refused.masks')};assert invoke(refused).returncode!=0;assert not Path(refused['output']).exists() and not Path(refused['maskOutput']).exists()
assert not list(root.glob('.person-sequence-*'))
report=dict(root=str(root),frames=ref['frames'],exactOriginalMasks=True,exactOriginalRgba=True,replayResetsState=True,cancelledNoPublication=True,changedSourceRefused=True,occupiedImageAndMaskPreserved=True,metadata=metadata)
(root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report))
