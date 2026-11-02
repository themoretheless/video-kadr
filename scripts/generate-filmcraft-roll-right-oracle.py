#!/usr/bin/env python3
"""Run the pinned original linked ripple-trim group, without substituting native conversion code."""
import argparse,json,os,subprocess,tempfile
from pathlib import Path
root=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--reference',type=Path,default=Path('/tmp/video-kadr-filmcraft-reference'));p.add_argument('--check',action='store_true');a=p.parse_args();ref=a.reference.resolve()
assert subprocess.check_output(['git','-C',str(ref),'rev-parse','HEAD'],text=True).strip()=='5231852443363f001c3f6b396dd9b1e6461ae2be'
assert not subprocess.check_output(['git','-C',str(ref),'status','--porcelain','--untracked-files=no'],text=True).strip()
with tempfile.TemporaryDirectory() as tmp:
 d=Path(tmp)
 context=(ref/'crates/engine/src/lib.rs').read_text();context=context[context.index('pub fn media_start('):context.index('/// Subclips of subclips followed')]
 (d/'main.rs').write_text((root/'scripts/filmcraft-roll-right-oracle.rs').read_text().replace('__CONTEXT__',context))
 manifest='[package]\nname="video-kadr-wav-audio-oracle"\nversion="0.1.0"\nedition="2024"\n[[bin]]\nname="oracle"\npath="main.rs"\n[dependencies]\nserde_json="1"\n'
 manifest+=f'filmcraft-media={{path={json.dumps(str(ref/"crates/media"))}}}\n'
 manifest+=''.join(f'filmcraft-{n}={{path={json.dumps(str(ref/"crates"/n))}}}\n' for n in ['project','edit','time'])
 (d/'Cargo.toml').write_text(manifest)
 result=subprocess.check_output(['cargo','+1.98.1','run','--quiet','--manifest-path',str(d/'Cargo.toml'),'--',str(root/'backend/tests/fixtures/filmcraft-project-index.json')],env=dict(os.environ,CARGO_TARGET_DIR='/tmp/video-kadr-filmcraft-video-oracle-target'))
 parsed=json.loads(result);assert len(parsed['cases'])==1120
 target=root/'backend/tests/fixtures/filmcraft-roll-right.json'
 if a.check: assert parsed==json.loads(target.read_text())
 else: target.write_bytes(result)
print('PASS: original roll oracle')
