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
pub fn inspect_inserts(seq:&filmcraft_project::Sequence,master:bool,t:i64)->serde_json::Value {
 let plan=Plan::build(seq,&std::collections::HashMap::new());
 let strip=if master{plan.strips.last().unwrap()}else{&plan.strips[0]};
 let mut out=vec![];
 for (post,inserts) in [(false,&strip.pre),(true,&strip.post)] {for insert in inserts {
 let dsp=crate::audio_fx::configured(&insert.effect,filmcraft_time::Tick(t),48000).unwrap();
 let targets:std::collections::BTreeMap<_,_>=dsp.params().iter().map(|p|(p.id,dsp.param(p.id).unwrap())).collect();
 out.push(serde_json::json!({"slot":insert.slot,"post":post,"dsp":dsp.id(),"targets":targets}));
 }}
 serde_json::json!(out)
}
'''
 (d/'mixer.rs').write_text((ref/'crates/render/src/mixer.rs').read_text()+wrapper)
 source=(ROOT/'scripts/filmcraft-audio-inserts-oracle.rs').read_text().replace('__AUDIO_PATH__',str(ref/'crates/render/src/audio.rs')).replace('__AUDIO_FX_PATH__',str(ref/'crates/render/src/audio_fx.rs'))
 (d/'main.rs').write_text(source)
 manifest='[package]\nname="video-kadr-audio-inserts-oracle"\nversion="0.1.0"\nedition="2024"\n[[bin]]\nname="audio-inserts-oracle"\npath="main.rs"\n[dependencies]\nserde_json="1"\nrayon="1"\n'
 manifest+=''.join(f'filmcraft-{n}={{path={json.dumps(str(ref/"crates"/n))}}}\n' for n in ['render','project','audio-dsp','frame','media','time'])
 (d/'Cargo.toml').write_text(manifest)
 result=subprocess.check_output(['cargo','+1.98.1','run','--quiet','--manifest-path',str(d/'Cargo.toml')],env=dict(os.environ,CARGO_TARGET_DIR='/tmp/video-kadr-filmcraft-video-oracle-target'))
 parsed=json.loads(result);assert parsed['commit']==SHA and len(parsed['cases'])==6600
 target=ROOT/'backend/tests/fixtures/filmcraft-audio-inserts.json'
 if a.check:assert parsed==json.loads(target.read_text())
 else:target.write_bytes(result)
print('Filmcraft original mixer insert plan oracle: PASS')
