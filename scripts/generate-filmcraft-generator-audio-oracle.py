#!/usr/bin/env python3
"""Regenerate synthetic-audio reference bits from the pinned original implementation."""
import argparse, pathlib, subprocess, tempfile
parser=argparse.ArgumentParser()
parser.add_argument('--reference',default='/tmp/video-kadr-filmcraft-reference')
parser.add_argument('--check',action='store_true')
args=parser.parse_args()
reference=pathlib.Path(args.reference)
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=reference,text=True).strip() == '5231852443363f001c3f6b396dd9b1e6461ae2be'
assert not subprocess.check_output(['git','diff','HEAD','--','crates/media/src/generators.rs'],cwd=reference)
s=(reference/'crates/media/src/generators.rs').read_text()
enums=s[s.index('#[derive(Clone, Copy'):s.index('impl Generator {')].replace(', Serialize, Deserialize','')
functions=s[s.index('fn db('):s.index('fn lerp3(')]
main='fn main(){ let gs=[Generator::BarsAndTone,Generator::CountingLeader,Generator::Tone{hz:437.5,db:-13.25},Generator::Demo(DemoScene::Aurora),Generator::Demo(DemoScene::OceanSunset),Generator::Demo(DemoScene::CityNight),Generator::Demo(DemoScene::Dunes),Generator::Demo(DemoScene::Plasma),Generator::Demo(DemoScene::Forest)]; for (g,x) in gs.iter().enumerate(){for rate in [44100,48000,96000]{for start in [-17,0,1837,44099,48001,96001]{ for i in 0..97 {let n=start+i;let (l,r)=if n<0 || n>=rate*2 {(0.,0.)} else {audio_sample(x,n as f32/rate as f32,n)};println!("{} {} {} {} {}",g,rate,n,l.to_bits(),r.to_bits());}}}}}\n'
with tempfile.TemporaryDirectory() as tmp:
    source=pathlib.Path(tmp)/'oracle.rs'; binary=pathlib.Path(tmp)/'oracle'
    source.write_text('use std::f32::consts::TAU;\n'+enums+functions+main)
    subprocess.run(['rustc',str(source),'-o',str(binary)],check=True)
    result=subprocess.check_output([str(binary)])
output=pathlib.Path(__file__).resolve().parents[1]/'backend/tests/fixtures/filmcraft-generator-audio.txt'
if args.check:
    assert output.read_bytes()==result, 'generator oracle fixture differs'
    print('PASS: original generator fixture is deterministic')
else:
    output.write_bytes(result)
