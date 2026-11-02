#!/usr/bin/env python3
"""Model-free temporal region protocol against independent scalar f32 composition."""
import argparse, copy, hashlib, json, math, pathlib, struct, subprocess, tempfile
p=argparse.ArgumentParser();p.add_argument('--binary',type=pathlib.Path,required=True);p.add_argument('--oracle',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
root=pathlib.Path(tempfile.mkdtemp(prefix='person-regions-',dir=a.output)).resolve();binary=a.binary.resolve()
sha=lambda b:hashlib.sha256(b).hexdigest()
f=lambda x:struct.unpack('f',struct.pack('f',x))[0]
def sample(data,w,h,u,v):
 x=f(f(u*w)-.5);y=f(f(v*h)-.5);x0=math.floor(x);y0=math.floor(y);fx=f(x-x0);fy=f(y-y0)
 def at(x,y):return data[max(0,min(h-1,y))*w+max(0,min(w-1,x))]
 upper=f(f(at(x0,y0)*f(1-fx))+f(at(x0+1,y0)*fx));lower=f(f(at(x0,y0+1)*f(1-fx))+f(at(x0+1,y0+1)*fx))
 return f(f(f(upper*f(1-fy))+f(lower*fy))/255)
def run(req,ok=True):
 path=root/'request.json';path.write_text(json.dumps(req));r=subprocess.run([str(binary),'--person-correct',str(path)],capture_output=True,text=True,timeout=30)
 assert (r.returncode==0)==ok,(r.stdout,r.stderr)
 if ok:return json.loads(r.stdout)
 assert not pathlib.Path(req['output']).exists() and not pathlib.Path(req['maskOutput']).exists(),r.stderr
cases=[]
for geometry,w,h,mw,mh in [('37x23',37,23,512,320),('23x37',23,37,320,512),('80x80',80,80,512,512)]:
 folder=root/geometry;folder.mkdir();oracle=a.oracle/geometry
 source=(oracle/'source.rgba').read_bytes();regions=(oracle/'reference.masks').read_bytes();auto=bytes([200])*(mw*mh*4)
 inp=folder/'source.rgba';inp.write_bytes(source);base=folder/'auto.gray';base.write_bytes(auto);reg=folder/'regions.gray';reg.write_bytes(regions)
 points=[[.5,.5]];encoded_hash=sha(b'public original movie identity');provenance={'sourceId':'original','maskSourceId':'tracked','sourceHash':encoded_hash,'maskHash':sha(regions),'maskFileHash':sha(b'encoded region identity'),'encoderHash':'a'*64,'decoderHash':'b'*64,'runtimeHash':'c'*64,'promptHash':sha(b'[[0.5,0.5]]'),'points':points}
 def request(name):
  edits=[]
  for ordinal,tool in [(1,'smartBrush'),(2,'smartEraser')]:
   plane=copy.deepcopy(provenance);plane['maskHash']=sha(regions[ordinal*65536:(ordinal+1)*65536])
   edits.append({'frame':ordinal,'correction':{'custom':True,'strokes':[{'tool':tool,'size':.1,'points':points,'region':plane}],'feather':0}})
  return {'schemaVersion':8,'input':str(inp),'mask':str(base),'output':str(folder/(name+'.rgba')),'maskOutput':str(folder/(name+'.gray')),'sourceSha256':sha(source),'maskSha256':sha(auto),'width':w,'height':h,'frames':4,'corrections':edits,'regions':[{'input':str(reg),'attachment':copy.deepcopy(provenance)}],'regionSourceSha256':encoded_hash}
 req=request('valid');meta=run(req)
 expected=bytearray(auto)
 for ordinal,color in [(1,255),(2,0)]:
  plane=regions[ordinal*65536:(ordinal+1)*65536]
  for y in range(mh):
   v=f(f(y+.5)/mh)
   for x in range(mw):
    if sample(plane,256,256,f(f(x+.5)/mw),v)>=.5:expected[ordinal*mw*mh+y*mw+x]=color
 actual=pathlib.Path(req['maskOutput']).read_bytes();assert actual==expected,(geometry,'mask mismatch')
 rgba=bytearray(source)
 for ordinal in range(4):
  plane=expected[ordinal*mw*mh:(ordinal+1)*mw*mh]
  for y in range(h):
   for x in range(w):
    index=ordinal*w*h*4+(y*w+x)*4+3
    rgba[index]=min(255,int(f(f(source[index]*sample(plane,mw,mh,f(f(x+.5)/w),f(f(y+.5)/h)))+.5)))
 assert pathlib.Path(req['output']).read_bytes()==rgba,(geometry,'RGBA mismatch')
 assert meta['regionHashes']=={'tracked':sha(regions)} and meta['regionSourceHash']==encoded_hash
 assert meta['outputHash']==sha(rgba) and meta['maskHash']==sha(expected)
 assert (pathlib.Path(req['output']).stat().st_mode&0o777)==0o600
 (folder/'expected.rgba').write_bytes(rgba);(folder/'expected.gray').write_bytes(expected);(folder/'valid.json').write_text(json.dumps(req));(folder/'metadata.json').write_text(json.dumps(meta))
 cases.append(geometry+':exact')
 req=request('ordered');first=copy.deepcopy(req['corrections'][1]['correction']['strokes'][0]);first['tool']='smartBrush';req['corrections'][1]['correction']['strokes'].insert(0,first);run(req)
 assert pathlib.Path(req['maskOutput']).read_bytes()==expected and pathlib.Path(req['output']).read_bytes()==rgba
 cases.append(geometry+':ordered')
 req=request('inactive-invalid-plane');req['corrections'][0]['correction']['custom']=False;req['corrections'][0]['correction']['strokes'][0]['region']['maskHash']='0'*64;run(req,False);cases.append(geometry+':inactive-invalid-plane')
 req=request('empty');empty=bytes(len(regions));reg.write_bytes(empty);req['regions'][0]['attachment']['maskHash']=sha(empty)
 for edit in req['corrections']:edit['correction']['strokes'][0]['region']['maskHash']=sha(bytes(65536))
 run(req);assert pathlib.Path(req['maskOutput']).read_bytes()==auto
 baseline=bytearray(source)
 for index in range(3,len(source),4):baseline[index]=min(255,int(f(f(source[index]*f(200/255))+.5)))
 assert pathlib.Path(req['output']).read_bytes()==baseline
 reg.write_bytes(regions);cases.append(geometry+':empty-no-fallback')

 for mode in ['unbound','unused','duplicate','source','provenance','points','plane','stream','length','regular','timed','unknown']:
  req=request('bad-'+mode)
  if mode=='unbound':req['regions']=[];req.pop('regionSourceSha256')
  elif mode=='unused':req['corrections']=[]
  elif mode=='duplicate':req['regions']*=2
  elif mode=='source':req['regionSourceSha256']='0'*64
  elif mode=='provenance':req['corrections'][0]['correction']['strokes'][0]['region']['runtimeHash']='d'*64
  elif mode=='points':req['corrections'][0]['correction']['strokes'][0]['points']=[[.1,.1]]
  elif mode=='plane':req['corrections'][0]['correction']['strokes'][0]['region']['maskHash']='0'*64
  elif mode=='stream':req['regions'][0]['attachment']['maskHash']='0'*64
  elif mode=='length':reg.write_bytes(regions+b'x')
  elif mode=='regular':req['corrections'][0]['correction']['strokes'][0]['tool']='brush'
  elif mode=='timed':req['corrections'][0]['correction']['strokes'][0]['at']=.5
  else:req['regions'][0]['extra']=True
  run(req,False);reg.write_bytes(regions);cases.append(geometry+':'+mode)
 for mode in ['changed-region','occupied-rgba','occupied-mask']:
  req=request(mode);path=folder/'race.json';path.write_text(json.dumps(req));proc=subprocess.Popen([str(binary),'--person-correct',str(path)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
  try:
   assert json.loads(proc.stderr.readline())['completed']==0
   if mode=='changed-region':reg.write_bytes(bytes([regions[0]^1])+regions[1:])
   else:pathlib.Path(req['output'] if mode=='occupied-rgba' else req['maskOutput']).write_bytes(b'foreign')
   out,err=proc.communicate(timeout=30);assert proc.returncode!=0,(mode,out,err)
   if mode=='occupied-rgba':assert pathlib.Path(req['output']).read_bytes()==b'foreign' and not pathlib.Path(req['maskOutput']).exists()
   elif mode=='occupied-mask':assert pathlib.Path(req['maskOutput']).read_bytes()==b'foreign' and not pathlib.Path(req['output']).exists()
   else:assert not pathlib.Path(req['output']).exists() and not pathlib.Path(req['maskOutput']).exists()
  finally:
   if proc.poll() is None:proc.kill();proc.wait()
   reg.write_bytes(regions)
  cases.append(geometry+':'+mode)
 assert not list(folder.glob('.person-correct-*'))
report={'passed':True,'cases':cases,'count':len(cases),'artifacts':str(root),'scope':'raw protocol/scalar f32 composition; no public job/editor or photographic quality claim'}
(root/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
