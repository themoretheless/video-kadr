#!/usr/bin/env python3
"""Original Editor move witnesses; generated data stays in ignored backend/target."""
from pathlib import Path
import hashlib, json, subprocess
root = Path(__file__).resolve().parent.parent
reference = root / 'backend/target/concat-reference'
commit = '78e4be101c2d3c41a7dc73c112c6b1f11b9ce170'
assert subprocess.check_output(['git', '-C', str(reference), 'rev-parse', 'HEAD'], text=True).strip() == commit
subprocess.run(['git', '-C', str(reference), 'diff', '--quiet', 'HEAD', '--', 'src/crates/concat-project/src', 'src/crates/concat-core/src'], check=True)
out = root / 'backend/target/concat-relocation-oracle'
out.mkdir(exist_ok=True)
(out / 'Cargo.toml').write_text('''[package]
name="video-kadr-concat-relocation"
version="0.1.0"
edition="2024"
[workspace]
[[bin]]
name="concat-relocation"
path="../../../scripts/reference/concat_relocation.rs"
[dependencies]
concat-project={path="../concat-reference/src/crates/concat-project"}
serde_json="1"
''')
plans = []
for curved in [False, True]:
 for start in [0., 2.00000049, 2.00000051, 2.0000005, 1/3, 359999.9]:
  for target in [0, 1, 1000000, 3000000, 359999000000, 360000000000]:
   clip = {'id':'c','trackId':'v','mediaId':'m','kind':'video','start':start,'duration':3.,'sourceStart':1.5,'speed':1.75,'fadeIn':.3,'fadeOut':.4,'keys':[{'property':'opacity','at':.375,'value':.8}]}
   if curved: clip['speedCurve'] = [{'at':0.,'speed':.5},{'at':.5,'speed':3.},{'at':1.,'speed':1.}]
   doc = {'version':1,'activeTimelineId':'t','media':[{'id':'m','kind':'video','path':'/unopened/retime.mov','duration':100.,'hasAudio':True,'width':16,'height':16}],'timelines':[{'id':'t','video':{'width':16,'height':16,'rateNum':30000,'rateDen':1001},'tracks':[{'id':'v','name':'Video'}],'clips':[clip]}]}
   plans.append({'name':f'move-{curved}-{start}-{target}','wireTarget':target,'document':doc})
(out / 'plans.json').write_text(json.dumps(plans, allow_nan=False))
subprocess.run(['cargo','+1.97.1','run','--manifest-path',str(out/'Cargo.toml'),'--target-dir',str(root/'backend/target/concat-retime-command-build'),'--',str(out/'plans.json'),str(out/'commands.json')],check=True)
rows=json.loads((out/'commands.json').read_text())
for row, plan in zip(rows, plans): row['wireTarget']=plan['wireTarget']
(out/'commands.json').write_text(json.dumps(rows, allow_nan=False))
sources=list((reference/'src/crates/concat-project/src').rglob('*.rs'))+list((reference/'src/crates/concat-core/src').rglob('*.rs'))
(out/'provenance.json').write_text(json.dumps({'commit':commit,'sources':{str(p.relative_to(reference)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(sources)},'runner':hashlib.sha256((root/'scripts/reference/concat_relocation.rs').read_bytes()).hexdigest(),'plans':hashlib.sha256((out/'plans.json').read_bytes()).hexdigest(),'commands':hashlib.sha256((out/'commands.json').read_bytes()).hexdigest()},indent=2))
