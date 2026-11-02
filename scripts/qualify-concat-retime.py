#!/usr/bin/env python3
"""Run unchanged pinned Concat speed curves and audio_pieces without media.

Known gain tracks are constructed directly; gain slicing, curve timing,
source progress and fades execute unchanged source functions. Rendering and
media decoding are outside this oracle. Outputs stay under ignored target.
"""
from pathlib import Path
import hashlib
import json
import subprocess
root = Path(__file__).resolve().parent.parent
ref = root / 'backend/target/concat-reference/src/crates'
checkout=ref.parent.parent
used=[ref/name for name in ['concat-core/src/time.rs','concat-core/src/retime.rs','concat-core/src/animate.rs','concat-core/src/timeline.rs','concat-export/src/lib.rs','concat-export/src/resolve.rs','concat-media/src/audio.rs']]
commit=subprocess.check_output(['git','-C',str(checkout),'rev-parse','HEAD'],text=True).strip()
assert commit=='78e4be101c2d3c41a7dc73c112c6b1f11b9ce170'
subprocess.run(['git','-C',str(checkout),'diff','--quiet','HEAD','--',*[str(p.relative_to(checkout)) for p in used]],check=True)
source = (ref / 'concat-export/src/lib.rs').read_text()
audio = (ref / 'concat-media/src/audio.rs').read_text()
def item(text, signature):
    start = text.index(signature)
    opening = text.index('{', start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[start:end]
def module(name, path):
    return f'#[path = "{path}"] mod {name};\n'
program = module('time', ref / 'concat-core/src/time.rs') + module('retime', ref / 'concat-core/src/retime.rs')
program += module('animate', ref / 'concat-core/src/animate.rs')
program += r'''
mod timeline{#[derive(Clone,Copy,Default)]pub struct Transform{pub scale:f64,pub offset_x:f64,pub offset_y:f64,pub rotation:f64,pub stretch_x:f64,pub stretch_y:f64}}
use animate::{Track,Track as AnimTrack,Key as AnimKey,Ease as AnimEase};
use time::{FrameRate,Rational};use retime::SpeedCurve;use std::path::PathBuf;
struct ExportClip{path:String,audio_stream:Option<u32>,start:f64,duration:f64,source_start:f64,speed:f64,preserve_pitch:bool,volume:f64,fade_in:f64,fade_out:f64,filter_chain:String,speed_curve:Vec<(f64,f64)>,gain_track:AnimTrack}
struct AudioClip{path:PathBuf,stream:Option<usize>,start:f64,duration:f64,source_start:f64,speed:f64,preserve_pitch:bool,volume:f64,volume_curve:AnimTrack,fade_in:f64,fade_out:f64,filter_chain:String}
fn volume_track(clip: &ExportClip)->AnimTrack{clip.gain_track.clone()}
mod audio{const SPEED_RANGE:std::ops::RangeInclusive<f64>=0.0625..=16.;
'''+item(audio,'pub fn clamp_speed(')+'}\n'+item(source,'pub fn audio_pieces(')+item(source,'fn track_slice(')
program += '\nconst EASE_STEPS:usize=12;\n' + item(audio, 'fn volume_expr(')
timeline = (ref / 'concat-core/src/timeline.rs').read_text()
resolve = (ref / 'concat-export/src/resolve.rs').read_text()
program += item(resolve, 'pub(crate) fn quantise(')
program += '\nstruct EngineClip{start:Rational,duration:Rational,source_start:Rational,speed:Rational,hold:bool,retime:Option<SpeedCurve>}\nimpl EngineClip{fn contains(&self,t:Rational)->bool{t>=self.start&&t<self.start+self.duration}\n' + item(timeline,'pub fn source_time_at(') + '}\n'
program += r'''
fn raw_opacity(fraction:f64)->u32{
 let animation=animate::Animation{opacity:Track::new(vec![AnimKey{at:0.,value:0.2,ease:AnimEase::LINEAR},AnimKey{at:1.,value:0.8,ease:AnimEase{x1:0.42,y1:0.,x2:0.58,y2:1.}}]),..Default::default()};
 animation.opacity_at(1.,fraction).to_bits()
}
fn raw_clock_records(){
 let points=vec![(0.,1.),(1.,3.)];
 for (case,(start,duration,source_start)) in [(0.0199996,2.01900049,0.0199996),(0.65065049,2.01900049,1.01900049),(0.02100049,2.0199996,1.01900049)].into_iter().enumerate(){
  for (num,den) in [(25,1),(30000,1001),(24000,1001)]{
   let rate=FrameRate::checked(num,den).unwrap();let engine=EngineClip{start:quantise(start,rate),duration:quantise(duration,rate),source_start:quantise(source_start,rate),speed:Rational::ONE,hold:false,retime:SpeedCurve::new(&points)};
   let count=(engine.duration.as_f64()*rate.fps().as_f64()).round()as i64;
   for frame in 0..count{let exact=engine.start+rate.time_of_frame(frame);println!("8 {case} {num} {den} {frame} {} {} {} {} {}",ticks(exact),ticks(engine.source_time_at(exact).unwrap()),ticks(engine.start),ticks(engine.duration),raw_opacity(((exact-engine.start)/engine.duration).as_f64()));}
  }
  let clip=ExportClip{path:String::new(),audio_stream:None,start,duration,source_start,speed:1.75,preserve_pitch:true,volume:1.,fade_in:0.35,fade_out:0.27,filter_chain:String::new(),speed_curve:points.clone(),gain_track:AnimTrack::default()};
  for (index,piece) in audio_pieces(&clip).iter().enumerate(){println!("9 {case} {index} {} {} {} {} {} {}",piece.start,piece.duration,piece.source_start,piece.speed,piece.fade_in,piece.fade_out);}
  for curve in [0,2,5]{let raw=ExportClip{path:String::new(),audio_stream:None,start,duration,source_start,speed:1.75,preserve_pitch:true,volume:1.,fade_in:0.35,fade_out:0.27,filter_chain:String::new(),speed_curve:cases()[curve].clone(),gain_track:AnimTrack::default()};for (index,piece) in audio_pieces(&raw).iter().enumerate(){println!("10 {curve} {case} {index} {} {} {} {} {} {}",piece.start,piece.duration,piece.source_start,piece.speed,piece.fade_in,piece.fade_out);}}
  for (speed_index,speed) in [0.0625,1.75,1.23456789,16.].into_iter().enumerate(){
   for (num,den) in [(25,1),(30000,1001),(24000,1001)]{
    let rate=FrameRate::checked(num,den).unwrap();let engine=EngineClip{start:quantise(start,rate),duration:quantise(duration,rate),source_start:quantise(source_start,rate),speed:Rational::approximate(speed).unwrap(),hold:false,retime:None};
    let count=(engine.duration.as_f64()*rate.fps().as_f64()).round()as i64;
    for frame in 0..count{let exact=engine.start+rate.time_of_frame(frame);println!("11 {case} {speed_index} {num} {den} {frame} {} {} {} {} {}",ticks(exact),ticks(engine.source_time_at(exact).unwrap()),ticks(engine.start),ticks(engine.duration),raw_opacity(((exact-engine.start)/engine.duration).as_f64()));}
   }
   let clip=ExportClip{path:String::new(),audio_stream:None,start,duration,source_start,speed,preserve_pitch:true,volume:1.,fade_in:0.35,fade_out:0.27,filter_chain:String::new(),speed_curve:vec![],gain_track:AnimTrack::default()};
   let pieces=audio_pieces(&clip);assert_eq!(pieces.len(),1);let piece=&pieces[0];println!("12 {speed_index} {case} 0 {} {} {} {} {} {}",piece.start,piece.duration,piece.source_start,piece.speed,piece.fade_in,piece.fade_out);
  }
 }
}
'''
program += r'''
fn ticks(value:Rational)->i128{value.numerator()as i128*254016000000i128/value.denominator()as i128}
fn cases()->Vec<Vec<(f64,f64)>>{vec![vec![(0.,1.),(1.,3.)],vec![(0.25,1.),(0.75,3.)],vec![(0.5,4.),(0.,0.5),(1.,0.5)],vec![(2.,0.),(-1.,100.)],vec![(0.4,2.)],vec![(0.5,1.),(0.5,4.),(1.,2.)],vec![(f64::NAN,1.),(0.3,1.),(1.,f64::NAN)]]}
fn main(){raw_clock_records();gain_records();for(index,points)in cases().into_iter().enumerate(){let curve=SpeedCurve::new(&points).unwrap();
 for (k,(x,speed))in curve.points().iter().enumerate(){println!("0 {index} {k} {x} {speed}");}
 for(num,den)in[(25,1),(30000,1001),(24000,1001)]{let rate=FrameRate::checked(num,den).unwrap();let count=(2.019*rate.fps().as_f64()).round()as i64;let duration=rate.time_of_frame(count);
 for frame in 0..count{let local=rate.time_of_frame(frame);let x=(local/duration).as_f64();let offset=Rational::approximate(curve.consumed(x)*duration.as_f64()).unwrap();println!("1 {index} {num} {den} {frame} {} {} {}",ticks(local),ticks(duration),ticks(offset));let engine=EngineClip{start:quantise(0.65065,rate),duration,source_start:quantise(0.65065,rate),speed:Rational::ONE,hold:false,retime:Some(curve.clone())};let exact=engine.start+local;let source=engine.source_time_at(exact).unwrap();println!("3 {index} {num} {den} {frame} {} {}",ticks(exact),ticks(source));let us=(ticks(exact)/254016)as i64;let coarse=Rational::new(us,1000000);if let Some(source)=engine.source_time_at(coarse){println!("4 {index} {num} {den} {frame} {} {}",us,ticks(source)/254016);}
}}
 for duration in [0.025,0.8,2.019,100.]{let clip=ExportClip{path:String::new(),audio_stream:None,start:0.021,duration,source_start:1.019,speed:1.,preserve_pitch:true,volume:1.,fade_in:0.35,fade_out:0.27,filter_chain:String::new(),speed_curve:points.clone(),gain_track:AnimTrack::default()};for(k,piece)in audio_pieces(&clip).iter().enumerate(){println!("2 {index} {duration} {k} {} {} {} {} {} {}",piece.start,piece.duration,piece.source_start,piece.speed,piece.fade_in,piece.fade_out);}}
}}
'''
program += r'''
fn gains()->Vec<AnimTrack>{let key=|at,value,ease|AnimKey{at,value,ease};let linear=AnimEase::LINEAR;let ease=AnimEase{x1:0.42,y1:0.,x2:0.58,y2:1.};vec![AnimTrack::default(),AnimTrack::new(vec![key(0.,0.2,linear),key(1.,0.8,linear)]),AnimTrack::new(vec![key(0.,0.2,linear),key(1.,0.8,ease)]),AnimTrack::new(vec![key(1.,0.9,linear),key(0.4,1.2,linear),key(0.4,0.3,AnimEase{x1:0.,y1:0.,x2:0.58,y2:1.}),key(0.,0.1,linear)]),AnimTrack::new(vec![key(0.25,0.2,linear),key(0.75,1.8,AnimEase{x1:0.42,y1:-0.2,x2:0.58,y2:1.2})])]}
fn gain_records(){for(index,track)in gains().iter().enumerate(){for(slice,(from,to))in[(0.,0.1),(0.2,0.3),(0.25,0.75),(0.,1.),(0.5,0.5),(0.9,1.)].into_iter().enumerate(){let part=track_slice(track,from,to);for duration in [0.,0.09614285714285714,2.019]{for rest in [1.,0.25]{let filter=volume_expr(&part,duration).map(|expr|format!("volume=volume='{expr}':eval=frame")).unwrap_or_else(||if (rest-1.0_f64).abs()>f64::EPSILON{format!("volume={rest:.4}")}else{String::from("-")});eprintln!("GAIN_EXPR\t{index}\t{slice}\t{duration}\t{rest}\t{filter}");}}for(k,key)in part.keys().iter().enumerate(){println!("6 {index} {slice} {k} {} {} {} {} {} {}",key.at,key.value,key.ease.x1,key.ease.y1,key.ease.x2,key.ease.y2);}for x in [0.,0.1,0.5,0.9,1.]{println!("5 {index} {slice} {x} {}",part.value_at(x,1.));}}
 for(curve,points)in cases().into_iter().enumerate(){let clip=ExportClip{path:String::new(),audio_stream:None,start:0.021,duration:2.019,source_start:1.019,speed:1.,preserve_pitch:true,volume:1.,fade_in:0.35,fade_out:0.27,filter_chain:String::new(),speed_curve:points,gain_track:track.clone()};for(k,piece)in audio_pieces(&clip).iter().enumerate(){for key in piece.volume_curve.keys(){eprintln!("GAIN_PIECE_KEY\t{curve}\t{index}\t{k}\t{}\t{}\t{}\t{}\t{}\t{}",key.at,key.value,key.ease.x1,key.ease.y1,key.ease.x2,key.ease.y2);}for x in [0.,0.5,1.]{println!("7 {curve} {index} {k} {x} {}",piece.volume_curve.value_at(x,1.));}}}
}}
'''
out = root / 'backend/target/concat-retime-oracle'
out.mkdir(exist_ok=True)
path = out / 'main.rs'
path.write_text(program)
subprocess.run(['rustc', '+1.89.0', '--edition=2021', str(path), '-o', str(out / 'oracle')], check=True)
result = subprocess.run([str(out / 'oracle')], check=True, capture_output=True, text=True)
(out / 'retime.txt').write_text(result.stdout)
(out / 'gain-expressions.tsv').write_text('\n'.join(line for line in result.stderr.splitlines() if line.startswith('GAIN_EXPR\t'))+'\n')
(out / 'gain-pieces.tsv').write_text('\n'.join(line for line in result.stderr.splitlines() if line.startswith('GAIN_PIECE_KEY\t'))+'\n')
(out/'raw-clock-provenance.json').write_text(json.dumps({'commit':commit,'sources':{str(p.relative_to(checkout)):hashlib.sha256(p.read_bytes()).hexdigest() for p in used},'generator':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'generatedRust':hashlib.sha256(path.read_bytes()).hexdigest(),'outputs':{name:hashlib.sha256((out/name).read_bytes()).hexdigest() for name in ['retime.txt','gain-expressions.tsv','gain-pieces.tsv']}},indent=2))
print(result.stdout, end='')
