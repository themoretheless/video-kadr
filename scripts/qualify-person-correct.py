#!/usr/bin/env python3
"""Actual model-free ordinal replay; independent alpha/RGB and retained oracle checks."""
import argparse, hashlib, json, pathlib, subprocess, tempfile
p=argparse.ArgumentParser();p.add_argument('--binary',required=True,type=pathlib.Path);p.add_argument('--oracle',required=True,type=pathlib.Path);p.add_argument('--output',required=True,type=pathlib.Path);a=p.parse_args()
a.output.mkdir(parents=True,exist_ok=True)
source=(a.oracle/'input.rgba').read_bytes();masks=(a.oracle/'reference.masks').read_bytes();reference=(a.oracle/'reference.rgba').read_bytes();frame=37*23*4;mask_frame=512*320
sha=lambda b:hashlib.sha256(b).hexdigest()
points=[[x/8,y/8] for y in range(9) for x in (range(9) if y%2==0 else reversed(range(9)))]
correction=lambda tool:{'custom':True,'strokes':[{'tool':tool,'size':0.5,'points':points}],'feather':0}
with tempfile.TemporaryDirectory(prefix='person-correct-',dir=a.output) as tmp:
 root=pathlib.Path(tmp).resolve();inp=root/'source.rgba';auto=root/'auto.masks';inp.write_bytes(source);auto.write_bytes(masks)
 def request(name,edits):
  return {'schemaVersion':8,'input':str(inp),'mask':str(auto),'output':str(root/(name+'.rgba')),'maskOutput':str(root/(name+'.masks')),'sourceSha256':sha(source),'maskSha256':sha(masks),'width':37,'height':23,'frames':4,'corrections':edits}
 def run(req,ok):
  path=root/'request.json';path.write_text(json.dumps(req));result=subprocess.run([str(a.binary.resolve()),'--person-correct',str(path)],capture_output=True,text=True,timeout=30)
  assert (result.returncode==0)==ok,(result.stdout,result.stderr)
  if ok:return json.loads(result.stdout)
  assert not pathlib.Path(req['output']).exists() and not pathlib.Path(req['maskOutput']).exists()
 req=request('identity',[]);meta=run(req,True);assert meta['correctionHash']==sha(b'[]');assert pathlib.Path(req['output']).read_bytes()==reference;assert pathlib.Path(req['maskOutput']).read_bytes()==masks
 req=request('corrected',[{'frame':1,'correction':correction('eraser')},{'frame':2,'correction':correction('brush')}]);meta=run(req,True);rgba=pathlib.Path(req['output']).read_bytes();corrected_masks=pathlib.Path(req['maskOutput']).read_bytes()
 assert rgba[:frame]==reference[:frame] and rgba[3*frame:]==reference[3*frame:]
 for i in range(frame,2*frame,4):assert rgba[i:i+3]==source[i:i+3] and rgba[i+3]==0
 assert rgba[2*frame:3*frame]==source[2*frame:3*frame]
 assert corrected_masks[mask_frame:2*mask_frame]==bytes(mask_frame)
 assert corrected_masks[2*mask_frame:3*mask_frame]==bytes([255])*mask_frame
 assert len(meta['correctionHash'])==64 and meta['correctionHash']!=sha(b'[]');assert meta['outputHash']==sha(rgba) and meta['maskHash']==sha(corrected_masks)
 assert (pathlib.Path(req['output']).stat().st_mode&0o777)==0o600
 old=pathlib.Path(req['output']).read_bytes();r=subprocess.run([str(a.binary.resolve()),'--person-correct',str(root/'request.json')],capture_output=True,timeout=30);assert r.returncode!=0 and pathlib.Path(req['output']).read_bytes()==old
 for name,edits in [('duplicate',[{'frame':1,'correction':correction('brush')}]*2),('outside',[{'frame':4,'correction':correction('brush')}]),('timed',[{'frame':0,'correction':{'custom':True,'strokes':[{'tool':'brush','size':0.1,'points':[[0.5,0.5]],'at':0.2}],'feather':0}}])]:run(request(name,edits),False)
 req=request('bad-hash',[]);req['maskSha256']='0'*64;run(req,False)
 req=request('gained-frames',[]);auto.write_bytes(masks+b'x');run(req,False);auto.write_bytes(masks)
 for mode in ['source-mutated','occupied-rgba','occupied-mask']:
  req=request('race-'+mode,[{'frame':1,'correction':correction('eraser')},{'frame':2,'correction':correction('brush')}]);path=root/'race.json';path.write_text(json.dumps(req))
  proc=subprocess.Popen([str(a.binary.resolve()),'--person-correct',str(path)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
  try:
   first=proc.stderr.readline();assert json.loads(first)['completed']==0,first
   if mode=='source-mutated':inp.write_bytes(bytes([source[0]^1])+source[1:])
   else:pathlib.Path(req['output'] if mode=='occupied-rgba' else req['maskOutput']).write_bytes(b'occupied')
   out,err=proc.communicate(timeout=30);assert proc.returncode!=0,(mode,out,err)
   if mode=='occupied-rgba':assert pathlib.Path(req['output']).read_bytes()==b'occupied' and not pathlib.Path(req['maskOutput']).exists()
   elif mode=='occupied-mask':assert pathlib.Path(req['maskOutput']).read_bytes()==b'occupied' and not pathlib.Path(req['output']).exists()
   else:assert not pathlib.Path(req['output']).exists() and not pathlib.Path(req['maskOutput']).exists()
  finally:
   if proc.poll() is None:proc.kill();proc.wait()
   inp.write_bytes(source)
 assert not list(root.glob('.person-correct-*'))
 report={'passed':True,'frames':4,'identityOriginalOracle':True,'eraseAllExact':True,'paintAllOriginalAlpha':True,'unmodifiedOrdinalsExact':True,'noReplace':True,'refusals':['duplicate','outside','timed','bad-hash','gained-frames','source-mutated','occupied-rgba','occupied-mask'],'outputHash':meta['outputHash'],'maskHash':meta['maskHash']}
 (a.output/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
