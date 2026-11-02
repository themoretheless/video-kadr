#!/usr/bin/env python3
"""Inspect the unmodified pinned mixer Plan and audio_fx DSP configuration."""
import argparse,json,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
SHA='5231852443363f001c3f6b396dd9b1e6461ae2be'
p=argparse.ArgumentParser();p.add_argument('--reference',type=Path,required=True);p.add_argument('--check',action='store_true');a=p.parse_args();ref=a.reference.resolve()
assert subprocess.check_output(['git','-C',str(ref),'rev-parse','HEAD'],text=True).strip()==SHA
assert not subprocess.check_output(['git','-C',str(ref),'status','--porcelain','--untracked-files=no'],text=True).strip()
with tempfile.TemporaryDirectory(prefix='video-kadr-audio-inserts-oracle-') as d:
 d=Path(d)
 wrapper='''
pub fn inspect_insert_pcm(seq:&filmcraft_project::Sequence,post:bool,start:i64,frames:usize,rate:u32,lat:usize)->serde_json::Value {
 let plan=Plan::build(seq,&std::collections::HashMap::new());let mut state=build_state(&plan,rate);
 let strip=&plan.strips[0];let st=&mut state.strips[0];
 let (chain,inserts)=if post{(&mut st.post,&strip.post)}else{(&mut st.pre,&strip.pre)};
 let latency:usize=chain.iter().map(|d|d.latency()).sum();
 let mut bus:Vec<Vec<f32>>=(0..2).map(|side|(0..frames).map(|i|{let n=start+i as i64;if side==0{(n.rem_euclid(31)-15)as f32/32.}else{(n.rem_euclid(23)-7)as f32/64.}}).collect()).collect();
 let bits=|bus:&Vec<Vec<f32>>|bus.iter().map(|c|c.iter().map(|v|v.to_bits()).collect::<Vec<_>>()).collect::<Vec<_>>();
 let original=bus.clone();let input=bits(&bus);run_inserts(chain,inserts,&mut bus,start,lat,rate);
 let mut other=build_state(&plan,rate);let st=&mut other.strips[0];let other_chain=if post{&mut st.post}else{&mut st.pre};
 let mut split=vec![Vec::new(),Vec::new()];
 for (a,b) in [(0,67),(67,frames)] {let mut piece:Vec<Vec<f32>>=original.iter().map(|c|c[a..b].to_vec()).collect();run_inserts(other_chain,inserts,&mut piece,start+a as i64,lat,rate);for (out,channel) in split.iter_mut().zip(piece){out.extend(channel);}}
 serde_json::json!({"input":input,"bits":bits(&bus),"splitBits":bits(&split),"latency":latency})
}
'''
 (d/'mixer.rs').write_text((ref/'crates/render/src/mixer.rs').read_text()+wrapper)
 source=(ROOT/'scripts/filmcraft-audio-inserts-pcm-oracle.rs').read_text().replace('__AUDIO_PATH__',str(ref/'crates/render/src/audio.rs')).replace('__AUDIO_FX_PATH__',str(ref/'crates/render/src/audio_fx.rs'))
 (d/'main.rs').write_text(source)
 manifest='[package]\nname="video-kadr-audio-inserts-oracle"\nversion="0.1.0"\nedition="2024"\n[[bin]]\nname="audio-inserts-oracle"\npath="main.rs"\n[dependencies]\nserde_json="1"\nrayon="1"\n'
 manifest+=''.join(f'filmcraft-{n}={{path={json.dumps(str(ref/"crates"/n))}}}\n' for n in ['render','project','audio-dsp','frame','media','time'])
 (d/'Cargo.toml').write_text(manifest)
 result=subprocess.check_output(['cargo','+1.98.1','run','--quiet','--manifest-path',str(d/'Cargo.toml')],env=dict(os.environ,CARGO_TARGET_DIR='/tmp/video-kadr-filmcraft-video-oracle-target'))
 parsed=json.loads(result);assert parsed['commit']==SHA and len(parsed['cases'])==3960
 target=ROOT/'backend/tests/fixtures/filmcraft-audio-inserts-pcm.json'
 if a.check:assert parsed==json.loads(target.read_text())
 else:target.write_bytes(result)
print('Filmcraft original mixer insert plan oracle: PASS')
