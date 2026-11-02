#!/usr/bin/env python3
"""Original FreezeFrame lane placement oracle; generated artifacts stay ignored."""
from pathlib import Path
import hashlib, json, subprocess
root=Path(__file__).resolve().parent.parent
reference=root/'backend/target/concat-reference/src'
commit='78e4be101c2d3c41a7dc73c112c6b1f11b9ce170'
assert subprocess.check_output(['git','-C',str(reference),'rev-parse','HEAD'],text=True).strip()==commit
subprocess.run(['git','-C',str(reference),'diff','--quiet','HEAD','--',':(top)src/crates/concat-project/src',':(top)src/crates/concat-core/src'],check=True)
out=root/'backend/target/concat-freeze-lane-oracle';out.mkdir(exist_ok=True)
(out/'Cargo.toml').write_text('''[package]
name="video-kadr-concat-freeze-lane"
version="0.1.0"
edition="2024"
[workspace]
[[bin]]
name="concat-freeze-lane"
path="../../../scripts/reference/concat_retime_commands.rs"
[dependencies]
concat-project={path="../concat-reference/src/crates/concat-project"}
serde_json="1"
''')
plans=[]
for start in [2.,359998.]:
 for time_offset in [0.,.01,1.125,2.99,3.]:
  for hold in [None,-1.,.001,1.00000049,5.]:
   target={'id':'target','trackId':'v','mediaId':'m','kind':'video','start':start,'duration':3.,'sourceStart':1.5,'speed':1.75,'speedCurve':[{'at':0.,'speed':.5},{'at':1.,'speed':3.}]}
   at=start+time_offset
   neighbors=[]
   for name,track,kind,offset in [('overlap','v','video',-.25),('at','v','image',0.),('later','v','audio',.75),('other','other','video',1.),('before','v','video',-1.)]:
    neighbors.append({**target,'id':name,'trackId':track,'kind':kind,'start':max(0.,at+offset),'duration':4.,'speedCurve':None})
   doc={'version':1,'activeTimelineId':'t','media':[{'id':'m','kind':'video','path':'/unopened/source.mov','duration':100.,'hasAudio':True,'width':16,'height':16}], 'timelines':[{'id':'t','name':'Freeze lane','video':{'width':16,'height':16,'rateNum':30000,'rateDen':1001},'tracks':[{'id':i,'name':i,'visible':True,'muted':False} for i in ['v','other']],'clips':[target,*neighbors]}]}
   plans.append({'name':f'{start}-{time_offset}-{hold}','document':doc,'command':{'op':'freezeFrame','clipId':'target','time':at,'duration':hold,'still':{'path':'/unopened/frame.jpg','name':'Still','kind':'image','width':16,'height':16,'hasAudio':False}}})
(out/'plans.json').write_text(json.dumps(plans))
subprocess.run(['cargo','+1.97.1','run','--manifest-path',str(out/'Cargo.toml'),'--target-dir',str(root/'backend/target/concat-retime-command-build'),'--',str(out/'plans.json'),str(out/'commands.json')],check=True)
sources=[*sorted((reference/'crates/concat-project/src').rglob('*.rs')),*sorted((reference/'crates/concat-core/src').rglob('*.rs'))]
files=[*sources,root/'scripts/reference/concat_retime_commands.rs',Path(__file__).resolve(),out/'plans.json',out/'commands.json',out/'Cargo.lock']
(out/'provenance.json').write_text(json.dumps({'commit':commit,'sha256':{str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in files}},indent=2))
print(out/'commands.json')
