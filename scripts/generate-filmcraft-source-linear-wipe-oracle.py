#!/usr/bin/env python3
"""Run pinned original full source-sequence frames with intrinsic Motion/Opacity."""
import argparse,json,os,subprocess,tempfile
from pathlib import Path
root=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--reference',type=Path,default=root/'backend/target/filmcraft-pinned-52318524');p.add_argument('--check',action='store_true');a=p.parse_args();ref=a.reference.resolve()
assert subprocess.check_output(['git','-C',str(ref),'rev-parse','HEAD'],text=True).strip()=='5231852443363f001c3f6b396dd9b1e6461ae2be'
assert not subprocess.check_output(['git','-C',str(ref),'status','--porcelain','--untracked-files=no'],text=True).strip()
with tempfile.TemporaryDirectory() as tmp:
 d=Path(tmp)
 (d/'main.rs').write_text((root/'scripts/filmcraft-source-linear-wipe-oracle.rs').read_text())
 manifest='[package]\nname="video-kadr-wav-audio-oracle"\nversion="0.1.0"\nedition="2024"\n[[bin]]\nname="oracle"\npath="main.rs"\n[dependencies]\nserde_json="1"\n'
 manifest+=f'filmcraft-media={{path={json.dumps(str(ref/"crates/media"))}}}\n'
 manifest+=''.join(f'filmcraft-{n}={{path={json.dumps(str(ref/"crates"/n))}}}\n' for n in ['project','edit','time','render','frame','geom'])
 (d/'Cargo.toml').write_text(manifest)
 result=subprocess.check_output(['cargo','+1.98.1','run','--offline','--quiet','--manifest-path',str(d/'Cargo.toml'),'--',str(root/'backend/tests/fixtures/filmcraft-source-effects.json'),str(root/'backend/tests/fixtures/filmcraft-source-masks.json')],env=dict(os.environ,CARGO_TARGET_DIR=str(root/'backend/target/filmcraft-original-oracle')))
 parsed=json.loads(result);assert len(parsed['cases'])==216
 target=root/'backend/tests/fixtures/filmcraft-source-linear-wipe.json'
 if a.check: assert parsed==json.loads(target.read_text())
 else: target.write_bytes(result)
print('PASS: original source linear wipe composition oracle')
