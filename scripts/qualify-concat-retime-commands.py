#!/usr/bin/env python3
"""Execute pinned Concat document commands; all generated data/builds are ignored."""
from pathlib import Path
import hashlib, json, subprocess
root=Path(__file__).resolve().parent.parent
reference=root/'backend/target/concat-reference/src'
assert subprocess.check_output(['git','-C',str(reference),'rev-parse','HEAD'],text=True).strip()=='78e4be101c2d3c41a7dc73c112c6b1f11b9ce170'
subprocess.run(['git','-C',str(reference),'diff','--quiet','HEAD','--',':(top)src/crates/concat-project/src',':(top)src/crates/concat-core/src'],check=True)
out=root/'backend/target/concat-retime-command-oracle';out.mkdir(exist_ok=True)
manifest='''[package]
name="video-kadr-concat-retime-commands"
version="0.1.0"
edition="2024"
[workspace]
[[bin]]
name="concat-retime-commands"
path="../../../scripts/reference/concat_retime_commands.rs"
[dependencies]
concat-project={path="../concat-reference/src/crates/concat-project"}
serde_json="1"
'''
(out/'Cargo.toml').write_text(manifest)
points=[{'at':0.,'speed':.5},{'at':.5,'speed':3.},{'at':1.,'speed':1.}]
clip={'id':'c','trackId':'v','mediaId':'m','kind':'video','start':2.,'duration':3.,'sourceStart':1.5,'speed':1.75,'speedCurve':points,'fadeIn':.3,'fadeOut':.4,'keys':[]}
neighbor={**clip,'id':'later','start':8.,'duration':1.,'sourceStart':0.,'speed':1.,'speedCurve':None}
doc={'version':1,'activeTimelineId':'t','media':[{'id':'m','kind':'video','path':'/unopened/retime.mov','duration':100.,'hasAudio':True,'width':16,'height':16}], 'timelines':[{'id':'t','name':'Clock commands','video':{'width':16,'height':16,'rateNum':30000,'rateDen':1001},'tracks':[{'id':'v','name':'Video','visible':True,'muted':False}],'clips':[clip,neighbor]}]}
commands=[]
for time in [2.,2.+1/60,3.125,5.-1/60,5.]:commands.append((f'split-{time}',{'op':'splitClips','clipIds':['c'],'time':time}))
for edge in ['start','end']:
 for delta in [-20.,-.375,.625,20.]:
  for ripple in [False,True]:commands.append((f'trim-{edge}-{delta}-{ripple}',{'op':'trimClip','clipId':'c','edge':edge,'delta':delta,'ripple':ripple}))
for curve in [None,[],points,[{'at':1.,'speed':2.},{'at':.5,'speed':4.},{'at':.5,'speed':1.},{'at':0.,'speed':.25}],[{'at':-1.,'speed':0.},{'at':2.,'speed':100.}]]:commands.append((f'set-curve-{len(commands)}',{'op':'setClipSpeedCurve','clipId':'c','curve':curve}))
# Video freeze uses an already-probed still; the oracle never opens media.
for time in [2.,3.125,5.]:commands.append((f'freeze-{time}',{'op':'freezeFrame','clipId':'c','time':time,'duration':1.,'still':{'path':'/unopened/frame.jpg','name':'Still','kind':'image','duration':None,'width':16,'height':16,'hasAudio':False}}))
plans=[{'name':name,'document':doc,'command':command} for name,command in commands]
zero_doc=json.loads(json.dumps(doc));zero_doc['timelines'][0]['clips'][0]['speed']=1.5
zero_points=[{'at':0.,'speed':1.},{'at':-0.,'speed':3.},{'at':1.,'speed':2.}]
zero_doc['timelines'][0]['clips'][0]['speedCurve']=zero_points
candidate=json.loads(json.dumps(zero_points));candidate[0]['at']=-0.
plans.append({'name':'set-curve-equal-signed-zero-points','document':zero_doc,'command':{'op':'setClipSpeedCurve','clipId':'c','curve':candidate}})
same=json.loads(json.dumps(doc));same['timelines'][0]['clips'][0]['speed']=8.03125;same['timelines'][0]['clips'][0]['speedCurve']=None
plans.append({'name':'set-curve-normalized-back-to-same-clock','document':same,'command':{'op':'setClipSpeedCurve','clipId':'c','curve':[{'at':-1.,'speed':0.},{'at':2.,'speed':100.}]}})
keyed=[]
for plan in plans:
 copy=json.loads(json.dumps(plan));copy['name']='keyed-'+copy['name'];target=copy['document']['timelines'][0]['clips'][0]
 target['fadeIn']=.30000049;target['fadeOut']=.40000049
 run=[{'at':at,'value':value,'ease':ease} for at,value,ease in [(0.,.2,[0.,0.,1.,1.]),(.373,.8,[.42,-.2,.58,1.2]),(.375,1.1,[0.,0.,.58,1.]),(.3769,1.3,[.42,0.,1.,1.]),(.8,.4,[.42,0.,.58,1.]),(1.,.9,[0.,0.,1.,1.])]]
 target['keys']=[dict(key,property=prop) for prop in ['opacity','volume','offsetX'] for key in run]
 target['filters']=[{'id':'concat.bass','params':{'gain':5.},'enabled':True,'keys':{'gain':run}}]
 target['videoEffects']=[{'id':'concat.contrast','params':{'amount':0.},'enabled':True,'keys':{'amount':run}}]
 keyed.append(copy)
plans.extend(keyed)
(out/'plans.json').write_text(json.dumps(plans))
subprocess.run(['cargo','+1.97.1','run','--manifest-path',str(out/'Cargo.toml'),'--target-dir',str(root/'backend/target/concat-retime-command-build'),'--',str(out/'plans.json'),str(out/'commands.json')],check=True)
sources=[*sorted((reference/'crates/concat-project/src').rglob('*.rs')),*sorted((reference/'crates/concat-core/src').rglob('*.rs'))]
(out/'provenance.json').write_text(json.dumps({'commit':'78e4be101c2d3c41a7dc73c112c6b1f11b9ce170','sources':{str(p.relative_to(reference)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sources},'oracle':hashlib.sha256((root/'scripts/reference/concat_retime_commands.rs').read_bytes()).hexdigest(),'plans':hashlib.sha256((out/'plans.json').read_bytes()).hexdigest(),'output':hashlib.sha256((out/'commands.json').read_bytes()).hexdigest()},indent=2))
print(out/'commands.json')
