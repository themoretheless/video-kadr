#!/usr/bin/env python3
"""Execute extracted, unmodified pinned upstream correction functions with upstream Affine."""
import argparse,json,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
SHA='5231852443363f001c3f6b396dd9b1e6461ae2be'
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--reference',type=Path,required=True);p.add_argument('--check',action='store_true');a=p.parse_args();ref=a.reference.resolve()
assert subprocess.check_output(['git','-C',str(ref),'rev-parse','HEAD'],text=True).strip()==SHA
assert not subprocess.check_output(['git','-C',str(ref),'status','--porcelain','--untracked-files=no'],text=True).strip()
src=(ref/'crates/render/src/vfx/temporal.rs').read_text()
math=src[src.index('/// Similarity parameters'):src.index('/// Warp Stabilizer: see the module docs.')]
driver=r'''
use filmcraft_geom::{Affine,Vec2};use serde_json::json;
struct StabilizerPath {w:usize,h:usize,steps:Vec<Affine>}
fn main(){let mut cases=vec![];
for profile in 0..3{let steps:Vec<Affine>=(0..7).map(|k|match profile{
0=>Affine::translate((k as f64-3.)*1.25,k as f64*0.3),
1=>Affine::translate(2.,-1.).then_apply(&Affine::rotate_deg(175.+k as f64*3.)),
_=>Affine::translate(-3.,2.).then_apply(&Affine::rotate_deg(k as f64*2.)).then_apply(&Affine::scale(1.+k as f64*0.01,1.+k as f64*0.01))}).collect();
for smooth in [0.,0.01,1.,3.5]{for fixed in [false,true]{for preserve in [false,true]{let path=StabilizerPath{w:160,h:90,steps:steps.clone()};let corr=corrections(&path,smooth,fixed,preserve);
cases.push(json!({"steps":steps,"stepBits":steps.iter().map(|m|[m.a,m.b,m.c,m.d,m.e,m.f].map(f64::to_bits)).collect::<Vec<_>>(),"smooth":smooth,"fixed":fixed,"preserve":preserve,"bits":corr.iter().map(|m|[m.a,m.b,m.c,m.d,m.e,m.f].map(f64::to_bits)).collect::<Vec<_>>(),"coverBits":corr.iter().map(|m|cover_scale(m,160.,90.,4.).to_bits()).collect::<Vec<_>>()}));}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
'''
with tempfile.TemporaryDirectory(prefix='stabilizer-oracle-') as temp:
 t=Path(temp);(t/'main.rs').write_text(driver+'\n'+math)
 (t/'Cargo.toml').write_text('[package]\nname="stabilizer-oracle"\nversion="0.1.0"\nedition="2024"\n[[bin]]\nname="stabilizer-oracle"\npath="main.rs"\n[dependencies]\nserde_json="1"\nfilmcraft-geom={path='+json.dumps(str(ref/'crates/geom'))+'}\n')
 result=subprocess.check_output(['cargo','+1.98.1','run','--quiet','--manifest-path',str(t/'Cargo.toml')],env=dict(os.environ,CARGO_TARGET_DIR='/tmp/video-kadr-filmcraft-video-oracle-target'))
 data=json.loads(result);assert len(data['cases'])==48
 target=ROOT/'backend/tests/fixtures/filmcraft-stabilizer.json'
 if a.check:assert data==json.loads(target.read_text())
 else:target.write_bytes(result)
print('Filmcraft extracted correction oracle: 48 cases PASS')
