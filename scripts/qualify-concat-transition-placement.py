#!/usr/bin/env python3
"""Execute unchanged pinned Concat overlap/head/tail/extend functions in an isolated oracle.

No media or pixel fixture. The unchanged source hold code runs too. A speed-curve
stub refuses nonempty curves, which remain outside this clock qualification.
"""
from pathlib import Path
import subprocess
root = Path(__file__).resolve().parent.parent
source = root / 'backend/target/concat-reference/src/crates/concat-export/src/lib.rs'
text = source.read_text()
functions = text[text.index('fn overlap('):text.index('/// The timing functions')]
out = root / 'backend/target/concat-transition-placement-oracle'
out.mkdir(exist_ok=True)
prefix = r'''
#[derive(Clone,PartialEq)] enum ClipKind {Video,Image}
#[derive(Clone)] struct Key {at:f64,property:String,value:f64}
#[derive(Clone)] struct ExportClip {kind:ClipKind,start:f64,duration:f64,source_start:f64,speed:f64,media_duration:Option<f64>,speed_curve:Vec<()>,track:usize,fade_in:f64,fade_out:f64,animation:Vec<Key>,hold:bool,muted:bool,has_audio:Option<bool>,volume:f64,transition:Option<()>,video_fade_in:f64,transition_shapes:Vec<()>,scale:f64,offset_x:f64,offset_y:f64,rotation:f64,opacity:f64}
struct Cut {outgoing:usize,incoming:usize,duration:f64}
struct Overlap {incoming:usize,length:f64,outgoing_runs:bool}
struct SpeedCurve;
impl SpeedCurve {fn new(v:&[()])->Option<Self>{assert!(v.is_empty());None}fn consumed(&self,_:f64)->f64{panic!("speed curve outside oracle")}fn speed_at(&self,_:f64)->f64{panic!("speed curve outside oracle")}}
'''
main = r'''
fn main(){for duration in [10.,3.] {for head in [0.,0.2,1.] {let clip=|start,source_start|ExportClip{kind:ClipKind::Video,start,duration:2.,source_start,speed:1.,media_duration:Some(duration),speed_curve:vec![],track:0,fade_in:0.,fade_out:0.,animation:vec![],hold:false,muted:false,has_audio:Some(true),volume:1.,transition:None,video_fade_in:0.,transition_shapes:vec![],scale:1.,offset_x:0.,offset_y:0.,rotation:0.,opacity:1.};let mut clips=vec![clip(0.,1.),clip(2.,head)];let span=overlap(&mut clips,&Cut{outgoing:0,incoming:1,duration:1.},0.04).unwrap();let held=clips.iter().find(|clip|clip.hold).map_or(-1.,|clip|{assert!(clip.muted&&clip.has_audio==Some(false)&&clip.volume==0.);clip.source_start});println!("{} {} {} {} {} {}",head,clips[span.incoming].start,span.length,clips[span.incoming].source_start,if span.outgoing_runs{1}else{0},held);}}}
'''

main=main.replace('fn main(){','fn singles(){',1)+r'''
fn chains(){for reversed in [false,true]{for held in [false,true]{
 let clip=|start,duration,source_start,media_duration|ExportClip{kind:ClipKind::Video,start,duration,source_start,speed:1.,media_duration:Some(media_duration),speed_curve:vec![],track:0,fade_in:0.,fade_out:0.,animation:vec![],hold:false,muted:false,has_audio:Some(true),volume:1.,transition:None,video_fade_in:0.,transition_shapes:vec![],scale:1.,offset_x:0.,offset_y:0.,rotation:0.,opacity:1.};
 let mut clips=vec![clip(0.,2.,1.,10.),clip(2.,2.,1.,if held{3.}else{10.}),clip(4.,6.,2.,20.)];
 let cuts=if reversed{vec![(1,2,3.),(0,1,1.)]}else{vec![(0,1,1.),(1,2,3.)]};
 for(step,(outgoing,incoming,duration))in cuts.into_iter().enumerate(){let span=overlap(&mut clips,&Cut{outgoing,incoming,duration},0.04).unwrap();let b=&clips[span.incoming];println!("-1 {} {} {} {} {} {} {} {}",reversed as u8,held as u8,step,incoming,b.start,span.length,b.source_start,span.outgoing_runs as u8);}
 for(index,clip)in clips[..3].iter().enumerate(){println!("-2 {} {} {} {} {} {} {} {}",reversed as u8,held as u8,index,clip.start,clip.duration,clip.source_start,clip.fade_in,clip.fade_out);}
}}}
fn keyed(){for held in [false,true]{for head in [0.,0.2,1.]{
 let clip=|start,source_start,media_duration|ExportClip{kind:ClipKind::Video,start,duration:2.,source_start,speed:1.,media_duration:Some(media_duration),speed_curve:vec![],track:0,fade_in:0.,fade_out:0.,animation:[("scale",0.6,1.1),("offsetX",-0.2,0.25),("offsetY",0.15,-0.1),("rotation",-25.,65.),("opacity",0.3,0.9),("volume",0.2,0.8)].into_iter().flat_map(|(name,first,last)|[Key{at:0.,property:name.into(),value:first},Key{at:1.,property:name.into(),value:last}]).collect(),hold:false,muted:false,has_audio:Some(true),volume:1.,transition:None,video_fade_in:0.,transition_shapes:vec![],scale:1.,offset_x:0.,offset_y:0.,rotation:0.,opacity:1.};
 let mut clips=vec![clip(0.,1.,if held{3.}else{10.}),clip(2.,head,10.)];overlap(&mut clips,&Cut{outgoing:0,incoming:1,duration:1.},0.04).unwrap();
 for(index,clip)in clips[..2].iter().enumerate(){for key in &clip.animation{let property=["scale","offsetX","offsetY","rotation","opacity","volume"].iter().position(|name|*name==key.property).unwrap();println!("-3 {} {} {} {} {} {} {} {} {} {} {}",held as u8,head,index,property,key.at,key.value,clip.start,clip.duration,clip.source_start,clip.fade_in,clip.fade_out);}}
 if held{let clip=clips.iter().find(|c|c.hold).unwrap();for(index,value)in [clip.scale,clip.offset_x,clip.offset_y,clip.rotation,clip.opacity].into_iter().enumerate(){println!("-4 1 {} {} {}",head,index,value);}}
}}}
fn main(){singles();chains();keyed();}
'''

# Execute the original engine's last-added-active selector unchanged as well.
timeline_source = (root / 'backend/target/concat-reference/src/crates/concat-core/src/timeline.rs').read_text()
selector = timeline_source[timeline_source.index('    pub fn clip_on_track_at('):timeline_source.index('    /// Where the last clip ends.')]
prefix += r'''
 type TrackId=usize;type ClipId=usize;type Rational=i64;
 struct EngineClip{start:i64,duration:i64,source_start:i64,hold:bool}
 impl EngineClip{fn contains(&self,time:i64)->bool{time>=self.start&&time<self.start+self.duration}}
 struct Track{clips:Vec<usize>}
 struct Timeline{tracks:Vec<Track>,clips:Vec<EngineClip>}
''' + 'impl Timeline {' + selector + '}\n'
main = main.replace('fn main(){singles();chains();keyed();}',r'''
fn visuals(){for held in [false,true]{for head in [0.,1.]{for requested in [0.9,1.,1.1]{
 let clip=|start,source_start,media_duration|ExportClip{kind:ClipKind::Video,start,duration:2.,source_start,speed:1.,media_duration:Some(media_duration),speed_curve:vec![],track:0,fade_in:0.,fade_out:0.,animation:vec![],hold:false,muted:false,has_audio:Some(true),volume:1.,transition:None,video_fade_in:0.,transition_shapes:vec![],scale:1.,offset_x:0.,offset_y:0.,rotation:0.,opacity:1.};
 let mut clips=vec![clip(0.,1.,if held{3.}else{10.}),clip(2.,head,10.)];
 overlap(&mut clips,&Cut{outgoing:0,incoming:1,duration:requested},0.04).unwrap();
 let quantise=|v:f64|(v*25.).round().max(0.)as i64;
 let mut timeline=Timeline{tracks:vec![Track{clips:vec![]},Track{clips:vec![]}],clips:vec![]};
 for(index,clip)in clips.iter().enumerate(){timeline.clips.push(EngineClip{start:quantise(clip.start),duration:quantise(clip.duration),source_start:quantise(clip.source_start),hold:clip.hold});timeline.tracks[clip.track].clips.push(index);}
 for time in 35..82{for lane in 0..2{if let Some(index)=timeline.clip_on_track_at(lane,time){let c=&timeline.clips[index];let source=c.source_start+if c.hold{0}else{time-c.start};println!("-5 {} {} {} {} {} {} {} {}",held as u8,head,requested,time,lane,if index==2{0}else{index},c.hold as u8,source);}}}
}}}}
fn main(){singles();chains();keyed();visuals();}
''')

main=main.replace('fn main(){singles();chains();keyed();visuals();}',r'''
fn visual_chains(){for reversed in [false,true]{for held in [false,true]{
 let clip=|start,duration,source_start,media_duration|ExportClip{kind:ClipKind::Video,start,duration,source_start,speed:1.,media_duration:Some(media_duration),speed_curve:vec![],track:0,fade_in:0.,fade_out:0.,animation:vec![],hold:false,muted:false,has_audio:Some(true),volume:1.,transition:None,video_fade_in:0.,transition_shapes:vec![],scale:1.,offset_x:0.,offset_y:0.,rotation:0.,opacity:1.};
 let mut clips=vec![clip(0.,2.,1.,10.),clip(2.,2.,1.,if held{3.}else{10.}),clip(4.,6.,2.,20.)];
 if reversed{clips.swap(1,2);}
 let cuts=if reversed{vec![(2,1,3.),(0,2,1.)]}else{vec![(0,1,1.),(1,2,3.)]};
 for(outgoing,incoming,duration)in cuts{overlap(&mut clips,&Cut{outgoing,incoming,duration},0.04).unwrap();}
 let mut timeline=Timeline{tracks:(0..4).map(|_|Track{clips:vec![]}).collect(),clips:vec![]};
 let q=|v:f64|(v*25.).round().max(0.)as i64;
 for(index,clip)in clips.iter().enumerate(){timeline.clips.push(EngineClip{start:q(clip.start),duration:q(clip.duration),source_start:q(clip.source_start),hold:clip.hold});timeline.tracks[clip.track].clips.push(index);}
 for time in 35..170{for lane in 0..4{if let Some(index)=timeline.clip_on_track_at(lane,time){let c=&timeline.clips[index];let canonical=if index==3{1}else if reversed&&index>0{3-index}else{index};let source=c.source_start+if c.hold{0}else{time-c.start};println!("-6 {} {} {} {} {} {} {}",reversed as u8,held as u8,time,lane,canonical,c.hold as u8,source);}}}
}}}
fn main(){singles();chains();keyed();visuals();visual_chains();}
''')

resolve_source=(root / 'backend/target/concat-reference/src/crates/concat-export/src/resolve.rs').read_text()
covers=resolve_source[resolve_source.index('    pub(crate) fn covers(&self, time: Rational)'):resolve_source.index('    /// How far through the cut')]
stages=text[text.index('fn transition_stages('):text.index('/// The frame when a packaged transition is live:')]
prefix+=r'''
struct TransitionSpan{start:i64,end:i64,to_track:usize,ordinal:usize}
#[derive(Clone)]struct PlannedLayer{clip:usize,track:usize,opacity:f32}
struct FramePlan{time:i64,width:u32,height:u32,layers:Vec<PlannedLayer>,treatments:Vec<()>,output:()}
'''+ 'impl TransitionSpan {'+covers+'}\n'+stages
main=main.replace('fn main(){singles();chains();keyed();visuals();visual_chains();}',r'''
fn overlapping(){for reversed in [false,true]{for held in [false,true]{
 let clip=|start,duration,source_start,media_duration|ExportClip{kind:ClipKind::Video,start,duration,source_start,speed:1.,media_duration:Some(media_duration),speed_curve:vec![],track:0,fade_in:0.,fade_out:0.,animation:vec![],hold:false,muted:false,has_audio:Some(true),volume:1.,transition:None,video_fade_in:0.,transition_shapes:vec![],scale:1.,offset_x:0.,offset_y:0.,rotation:0.,opacity:1.};
 let mut clips=vec![clip(0.,2.,1.,10.),clip(2.,0.4,1.,if held{1.44}else{10.}),clip(2.4,2.,1.,10.)];
 clips[1].animation=vec![Key{at:0.,property:"opacity".into(),value:0.2},Key{at:1.,property:"opacity".into(),value:0.8}];
 if reversed{clips.swap(1,2);}
 let cuts=if reversed{vec![(2,1),(0,2)]}else{vec![(0,1),(1,2)]};
 let q=|v:f64|(v*25.).round().max(0.)as i64;let mut spans=vec![];
 for(outgoing,incoming)in cuts{let span=overlap(&mut clips,&Cut{outgoing,incoming,duration:2.},0.04).unwrap();let b=&mut clips[incoming];b.video_fade_in=span.length;spans.push(TransitionSpan{start:q(b.start),end:q(b.start)+q(span.length),to_track:b.track,ordinal:if reversed{3-incoming}else{incoming}});}
 let canonical=|index:usize|if index>=3{1}else if reversed&&index>0{3-index}else{index};
 // A keyed companion shares these windows; stage rows below are the static scene.
 for clip in &mut clips{for key in &clip.animation{println!("-10 {} {} {} {}",reversed as u8,held as u8,key.at,key.value);}clip.animation.clear();}
 for(index,clip)in clips[..3].iter().enumerate(){println!("-8 {} {} {} {} {} {} {} {}",reversed as u8,held as u8,canonical(index),clip.start,clip.duration,clip.source_start,clip.fade_in,clip.fade_out);}
 let mut timeline=Timeline{tracks:(0..4).map(|_|Track{clips:vec![]}).collect(),clips:vec![]};
 for(index,clip)in clips.iter().enumerate(){timeline.clips.push(EngineClip{start:q(clip.start),duration:q(clip.duration),source_start:q(clip.source_start),hold:clip.hold});timeline.tracks[clip.track].clips.push(index);}
 for time in 35..82{
  let layers=(0..4).filter_map(|lane|{let index=timeline.clip_on_track_at(lane,time)?;let c=&timeline.clips[index];let fade=q(clips[index].video_fade_in);let opacity=if fade>0{((time-c.start)as f32/fade as f32).clamp(0.,1.)}else{1.};Some(PlannedLayer{clip:canonical(index),track:lane,opacity})}).collect::<Vec<_>>();
  let plan=FramePlan{time,width:16,height:16,layers,treatments:vec![],output:()};
  if let Some(span)=spans.iter().find(|span|span.covers(time)){
   println!("-7 {} {} {} {} {} {} {}",reversed as u8,held as u8,time,span.ordinal,span.start,span.end,span.to_track);
   let(from,to)=transition_stages(&plan,span);let above=plan.layers.iter().filter(|layer|layer.track>span.to_track).cloned().collect();
   for(stage,layers)in [from.layers,to.layers,above].into_iter().enumerate(){for layer in layers{println!("-9 {} {} {} {} {} {} {}",reversed as u8,held as u8,time,stage,layer.clip,layer.track,layer.opacity);}}
  }else{
   println!("-7 {} {} {} -1 -1 -1 -1",reversed as u8,held as u8,time);
   for layer in plan.layers{println!("-9 {} {} {} 3 {} {} {}",reversed as u8,held as u8,time,layer.clip,layer.track,layer.opacity);}
  }
 }
}}}
fn main(){singles();chains();keyed();visuals();visual_chains();overlapping();}
''')

def rust_item(source, needle):
    start=source.index(needle)
    opening=source.index('{',start)
    depth=1
    finish=opening+1
    while depth:
        depth += (source[finish]=='{')-(source[finish]=='}')
        finish+=1
    return source[start:finish]
treatment_covers=rust_item(resolve_source[resolve_source.index('impl Treatment {'):],'pub(crate) fn covers(')
treatment_strength=rust_item(resolve_source,'pub(crate) fn strength_at(')
treatment_quantise=rust_item(resolve_source,'pub(crate) fn quantise(')
treatment_shaded=rust_item(resolve_source,'pub(crate) fn shaded(')
prefix+=r'''
mod treatment_clock {
#[derive(Clone,Copy,PartialEq,PartialOrd)]struct Rational(f64);impl Rational{fn as_f64(self)->f64{self.0}}
#[derive(Clone,Copy)]struct FrameRate;impl FrameRate{fn fps(self)->Rational{Rational(25.)}fn time_of_frame(self,frame:i64)->Rational{Rational(frame as f64/25.)}}
#[derive(Clone)]struct AppliedFilter{id:String,enabled:bool}
struct Catalogue;struct Package;
impl Catalogue{fn builtin()->Self{Self}fn get(&self,id:&str)->Option<Package>{if id=="concat.exposure"{Some(Package)}else{None}}}
impl Package{fn shader(&self)->Option<()>{Some(())}}
struct Treatment{start:Rational,end:Rational,strength:f32,ramp_in:f64,ramp_out:f64}
'''+treatment_quantise+treatment_shaded+'impl Treatment{'+treatment_covers+treatment_strength+'}'+r'''
pub fn run(){for mode in 0..10{
 let strength=if mode==2{0.}else if mode==3{0.8}else{1.};
 let(start,duration)=if mode==9{(1.53,0.01)}else{(1.53,0.9)};
 let effects=if mode==0||mode==5{vec![]}else{vec![AppliedFilter{id:if mode==8{"unknown.plugin".into()}else{"concat.exposure".into()},enabled:mode!=4&&mode!=8}]};
 let first=quantise(start,FrameRate);let length=quantise(duration,FrameRate);
 let treatment=Treatment{start:first,end:Rational(first.0+length.0),strength,ramp_in:if mode==3{1.}else{0.},ramp_out:if mode==3{0.4}else{0.}};
 let retained=mode!=6&&mode!=7&&length.0>0.&&!shaded(&effects).is_empty();
 for frame in 35..71{let time=Rational(frame as f64/25.);let live=retained&&treatment.covers(time);println!("-11 {} {} {} {} {}",mode,frame,live as u8,if live{treatment.strength_at(time)}else{0.},if live{(time.0-first.0)/length.0}else{0.});}
}}
}
'''
main=main.replace('visual_chains();overlapping();}', 'visual_chains();overlapping();treatment_clock::run();}')

# Keep the complete original resolver for legacy dispatch: the catalogue stub
# is never consulted by cross-fade, which precedes packaged ids in its match.
legacy_resolver=rust_item(text,'fn resolve_transitions_at(')
legacy_shapes=rust_item(text,'enum TransitionShape')
legacy_rides=rust_item(text,'fn rides(')+rust_item(text,'fn ride(')
legacy_quantise=rust_item(resolve_source,'pub(crate) fn quantise(')+rust_item(resolve_source,'pub(crate) fn quantise_at(')
prefix+=r'''
mod legacy_cross_fade{
use std::collections::BTreeMap;
#[derive(Clone,PartialEq)]enum ClipKind{Video,Image}impl ClipKind{fn is_visual(&self)->bool{true}}
#[derive(Clone)]struct ExportKey{at:f64,property:String,value:f64,ease:[f64;4]}
#[derive(Clone)]struct Request{kind:String,duration:f64}
#[derive(Clone)]struct ExportClip{kind:ClipKind,start:f64,duration:f64,source_start:f64,speed:f64,media_duration:Option<f64>,speed_curve:Vec<()>,track:usize,fade_in:f64,fade_out:f64,animation:Vec<ExportKey>,hold:bool,muted:bool,has_audio:Option<bool>,volume:f64,transition:Option<Request>,video_fade_in:f64,transition_shapes:Vec<TransitionShape>,scale:f64,offset_x:f64,offset_y:f64,rotation:f64,opacity:f64,hidden:bool}
struct Cut{outgoing:usize,incoming:usize,duration:f64,kind:String}
struct Overlap{incoming:usize,length:f64,outgoing_runs:bool}
struct SpeedCurve;impl SpeedCurve{fn new(v:&[()])->Option<Self>{assert!(v.is_empty());None}fn consumed(&self,_:f64)->f64{panic!("speed curves outside oracle")}fn speed_at(&self,_:f64)->f64{panic!("speed curves outside oracle")}}
#[derive(Clone,Copy)]struct FrameRate;struct RateNumber;impl FrameRate{fn fps(self)->RateNumber{RateNumber}fn time_of_frame(self,v:i64)->i64{v}}impl RateNumber{fn as_f64(self)->f64{25.}}
type Rational=i64;type TrackId=usize;type ClipId=usize;
struct TransitionSpan{start:i64,end:i64,to_track:usize,id:String,params:BTreeMap<String,f64>}
struct Catalogue;struct Package;impl Catalogue{fn builtin()->Self{Self}fn get(&self,_:&str)->Option<Package>{None}}impl Package{fn transition(&self)->Option<()>{None}}
const EASE_IN_OUT:[f64;4]=[0.42,0.,0.58,1.];const EASE_OUT:[f64;4]=[0.,0.,0.58,1.];const EASE_IN:[f64;4]=[0.42,0.,1.,1.];fn linear_ease()->[f64;4]{[0.,0.,1.,1.]}
struct EngineClip{start:i64,duration:i64,source_start:i64,hold:bool}impl EngineClip{fn contains(&self,time:i64)->bool{time>=self.start&&time<self.start+self.duration}}
struct Track{clips:Vec<usize>}struct Timeline{tracks:Vec<Track>,clips:Vec<EngineClip>}
'''+ 'impl Timeline {'+selector+'}\n'+ '#[derive(Clone)]'+legacy_shapes+functions+legacy_rides+legacy_quantise+legacy_resolver+r'''
pub fn run(){for held in [false,true]{for head in [0.,0.2,1.]{for requested in [0.9,1.,1.1]{
 let clip=|start,source_start,media_duration,opacity|ExportClip{kind:ClipKind::Video,start,duration:2.,source_start,speed:1.,media_duration:Some(media_duration),speed_curve:vec![],track:0,fade_in:0.,fade_out:0.,animation:vec![],hold:false,muted:false,has_audio:Some(true),volume:1.,transition:None,video_fade_in:0.,transition_shapes:vec![],scale:1.,offset_x:0.,offset_y:0.,rotation:0.,opacity,hidden:false};
 let mut clips=vec![clip(0.,1.,if held{3.}else{10.},0.5),clip(2.,head,10.,0.25)];clips[1].transition=Some(Request{kind:"cross-fade".into(),duration:requested});
 assert!(resolve_transitions_at(&mut clips,FrameRate,0.).is_empty(),"legacy cross-fade must never emit a packaged shader span");
 let mut timeline=Timeline{tracks:(0..2).map(|_|Track{clips:vec![]}).collect(),clips:vec![]};
 for(index,clip)in clips.iter().enumerate(){println!("-12 {} {} {} {} {} {} {} {} {} {} {} {}",held as u8,head,requested,if index==2{0}else{index},clip.start,clip.duration,clip.source_start,clip.fade_in,clip.fade_out,clip.video_fade_in,clip.hold as u8,clip.track);timeline.clips.push(EngineClip{start:quantise(clip.start,FrameRate),duration:quantise(clip.duration,FrameRate),source_start:quantise(clip.source_start,FrameRate),hold:clip.hold});timeline.tracks[clip.track].clips.push(index);}
 for time in 35..88{for lane in 0..2{if let Some(index)=timeline.clip_on_track_at(lane,time){let c=&timeline.clips[index];let fade=quantise(clips[index].video_fade_in,FrameRate);let opacity=clips[index].opacity*if fade>0{((time-c.start)as f64/fade as f64).clamp(0.,1.)}else{1.};let source=c.source_start+if c.hold{0}else{time-c.start};println!("-13 {} {} {} {} {} {} {} {} {}",held as u8,head,requested,time,lane,if index==2{0}else{index},c.hold as u8,source,opacity);}}}
}}}}
}
'''
main=main.replace('treatment_clock::run();}', 'treatment_clock::run();legacy_cross_fade::run();}')

original_animate=(root/'backend/target/concat-reference/src/crates/concat-core/src/animate.rs').read_text()
original_animate='\n'.join(line for line in original_animate.splitlines() if not line.startswith('//!'))
prefix+=r'''
pub mod timeline{#[derive(Clone,Copy,Debug,Default)]pub struct Transform{pub scale:f64,pub offset_x:f64,pub offset_y:f64,pub rotation:f64,pub stretch_x:f64,pub stretch_y:f64}}
mod original_animation{
'''+original_animate+'}\n'+'pub mod animate{pub use crate::original_animation::*;}\n'
# Compile unchanged original animation conversion and the engine evaluator.
prefix=prefix.replace('mod legacy_cross_fade{','mod legacy_cross_fade{\nuse crate as concat_core;use crate::original_animation::Animation;'+rust_item(resolve_source,'pub(crate) fn animation_of('),1)
motion_oracle=r'''
fn pose_rows(clips:&[ExportClip],tag:i32,scenario:&str,stretch:[f64;2],reversed:bool){
 let mut timeline=Timeline{tracks:(0..4).map(|_|Track{clips:vec![]}).collect(),clips:vec![]};
 for(index,clip)in clips.iter().enumerate(){timeline.clips.push(EngineClip{start:quantise(clip.start,FrameRate),duration:quantise(clip.duration,FrameRate),source_start:quantise(clip.source_start,FrameRate),hold:clip.hold});}
 let mut order=(0..clips.len()).collect::<Vec<_>>();if reversed{order.swap(1,2);}for index in order{timeline.tracks[clips[index].track].clips.push(index);}
 for time in [35,38,40,45,50,53,55,60,63,70,85,100,108,120,130]{for lane in 0..4{if let Some(index)=timeline.clip_on_track_at(lane,time){let c=&clips[index];let engine=&timeline.clips[index];let progress=((time-engine.start)as f64/engine.duration as f64).clamp(0.,1.);let base=crate::timeline::Transform{scale:c.scale,offset_x:c.offset_x,offset_y:c.offset_y,rotation:c.rotation,stretch_x:stretch[0],stretch_y:stretch[1]};let animation=animation_of(&c.animation);let pose=animation.as_ref().map_or(base,|a|a.transform_at(base,progress));let opacity=animation.as_ref().map_or(c.opacity as f32,|a|a.opacity_at(c.opacity as f32,progress));let fade=quantise(c.video_fade_in,FrameRate);let ramp=if fade>0{((time-engine.start)as f64/fade as f64).clamp(0.,1.)}else{1.};let source=engine.source_start+if engine.hold{0}else{time-engine.start};println!("{} {} {} {} {} {} {} {} {} {} {} {} {}",tag,scenario,time,lane,if c.hold{if tag==-14{0}else{1}}else{index},c.hold as u8,pose.scale*pose.stretch_x,pose.scale*pose.stretch_y,pose.offset_x*16.,pose.offset_y*16.,opacity,ramp,source);}}}
}
pub fn motions(){for(kind_code,kind,property)in [(0,"push","offsetX"),(1,"zoom","scale")]{for held in [false,true]{for head in [0.,1.]{for keyed in 0..3{for rest in 0..3{
 let clip=|start,source_start,media_duration|ExportClip{kind:ClipKind::Video,start,duration:2.,source_start,speed:1.,media_duration:Some(media_duration),speed_curve:vec![],track:0,fade_in:0.,fade_out:0.,animation:vec![],hold:false,muted:false,has_audio:Some(true),volume:1.,transition:None,video_fade_in:0.,transition_shapes:vec![],scale:if rest==2{8.}else if rest==1{1.7}else{1.},offset_x:if rest==2{-0.4}else if rest==1{0.2}else{0.},offset_y:0.,rotation:0.,opacity:0.65,hidden:false};
 let mut clips=vec![clip(0.,1.,if held{3.}else{10.}),clip(2.,head,10.)];
 if keyed>0{let c=&mut clips[keyed-1];if property=="scale"{c.scale=1.;}else{c.offset_x=0.;}c.animation=vec![ExportKey{at:0.,property:property.into(),value:if property=="scale"{0.8}else{-0.1},ease:linear_ease()},ExportKey{at:1.,property:property.into(),value:if property=="scale"{1.2}else{0.2},ease:linear_ease()}];}
 clips[1].transition=Some(Request{kind:kind.into(),duration:1.});assert!(resolve_transitions_at(&mut clips,FrameRate,0.).is_empty());
 pose_rows(&clips,-14,&format!("{} {} {} {} {}",kind_code,held as u8,head,keyed,rest),if rest>0{[1.2,0.8]}else{[1.,1.]},false);
}}}}}
 for first in 0..2{for second in 0..2{for reversed in [false,true]{for held in [false,true]{
 let clip=|start,media_duration|ExportClip{kind:ClipKind::Video,start,duration:2.,source_start:1.,speed:1.,media_duration:Some(media_duration),speed_curve:vec![],track:0,fade_in:0.,fade_out:0.,animation:vec![],hold:false,muted:false,has_audio:Some(true),volume:1.,transition:None,video_fade_in:0.,transition_shapes:vec![],scale:1.7,offset_x:0.2,offset_y:0.,rotation:0.,opacity:0.65,hidden:false};
 let mut clips=vec![clip(0.,10.),clip(2.,if held{3.}else{10.}),clip(4.,10.)];
 clips[1].transition=Some(Request{kind:if first==0{"push"}else{"zoom"}.into(),duration:1.});clips[2].transition=Some(Request{kind:if second==0{"push"}else{"zoom"}.into(),duration:1.});
 // Preserve canonical clip indices while resolving cuts in original source order.
 if reversed{clips.swap(1,2);}assert!(resolve_transitions_at(&mut clips,FrameRate,0.).is_empty());if reversed{clips.swap(1,2);}
 pose_rows(&clips,-15,&format!("{} {} {} {}",first,second,reversed as u8,held as u8),[1.2,0.8],reversed);
 }}}}
}
'''
prefix=prefix.replace('pub fn run(){for held in [false,true]{for head in [0.,0.2,1.]{for requested in [0.9,1.,1.1]{',motion_oracle+'pub fn run(){for held in [false,true]{for head in [0.,0.2,1.]{for requested in [0.9,1.,1.1]{',1)
main=main.replace('legacy_cross_fade::run();}', 'legacy_cross_fade::run();legacy_cross_fade::motions();}')

plan_source=(root/'backend/target/concat-reference/src/crates/concat-render/src/plan.rs').read_text()
source_shading='#[derive(Clone,Copy,Debug,PartialEq)]'+rust_item(plan_source,'pub enum Transition')+rust_item(plan_source,'pub struct Shading')+rust_item(plan_source,'impl Shading')+rust_item(text,'pub(crate) fn transitions_at(')
prefix=prefix.replace('mod legacy_cross_fade{','mod legacy_cross_fade{'+source_shading,1)
wipe_oracle=r'''
pub fn wipes(){for(direction,kind)in [(0,"wipe-left"),(1,"wipe-right")]{for held in [false,true]{for head in [0.,1.]{for requested in [0.9,1.1]{for keyed in [false,true]{
 let clip=|start,source_start,media_duration,opacity|ExportClip{kind:ClipKind::Video,start,duration:2.,source_start,speed:1.,media_duration:Some(media_duration),speed_curve:vec![],track:0,fade_in:0.,fade_out:0.,animation:vec![],hold:false,muted:false,has_audio:Some(true),volume:1.,transition:None,video_fade_in:0.,transition_shapes:vec![],scale:1.,offset_x:0.,offset_y:0.,rotation:0.,opacity,hidden:false};
 let mut clips=vec![clip(0.,1.,if held{3.}else{10.},0.5),clip(2.,head,10.,if keyed{1.}else{0.25})];
 if keyed{clips[1].animation=vec![ExportKey{at:0.,property:"opacity".into(),value:0.2,ease:linear_ease()},ExportKey{at:1.,property:"opacity".into(),value:0.8,ease:linear_ease()}];}
 clips[1].transition=Some(Request{kind:kind.into(),duration:requested});assert!(resolve_transitions_at(&mut clips,FrameRate,0.).is_empty());assert_eq!(clips[1].video_fade_in,0.,"legacy wipes must not add a dissolve ramp");
 let mut timeline=Timeline{tracks:(0..2).map(|_|Track{clips:vec![]}).collect(),clips:vec![]};
 for(index,c)in clips.iter().enumerate(){timeline.clips.push(EngineClip{start:quantise(c.start,FrameRate),duration:quantise(c.duration,FrameRate),source_start:quantise(c.source_start,FrameRate),hold:c.hold});timeline.tracks[c.track].clips.push(index);}
 let b=&clips[1];let engine=&timeline.clips[1];let animation=animation_of(&b.animation);
 for frame in 35..88{
  let visible=timeline.clip_on_track_at(1,frame).is_some();let effects=if visible{transitions_at(&b.transition_shapes,frame-engine.start)}else{vec![]};let shade=Shading::of(&effects);
  let mut bits=0u32;for probe in 0..18{let u=if probe<16{(probe as f32+0.5)/16.}else{(probe-16)as f32};if shade.keeps(u){bits|=1<<probe;}}
  let progress=((frame-engine.start)as f64/engine.duration as f64).clamp(0.,1.);let opacity=if visible{animation.as_ref().map_or(b.opacity as f32,|a|a.opacity_at(b.opacity as f32,progress))}else{0.};
  println!("-16 {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",direction,held as u8,head,requested,keyed as u8,frame,engine.start,engine.duration,shade.left_edge,shade.right_edge,opacity,bits,(!effects.is_empty())as u8,timeline.clip_on_track_at(0,frame).is_some()as u8,visible as u8);
 }
}}}}}}
'''
prefix=prefix.replace('pub fn motions(){',wipe_oracle+'pub fn motions(){',1)
main=main.replace('legacy_cross_fade::motions();}','legacy_cross_fade::motions();legacy_cross_fade::wipes();}')

colour_oracle=r'''
pub fn colours(){for white in [false,true]{for requested in [0.01,0.9,1.1,10.]{for short in [false,true]{for head in [0.,1.]{
 let length=if short{0.4}else{2.};
 let clip=|start,duration,source_start,media_duration,opacity|ExportClip{kind:ClipKind::Video,start,duration,source_start,speed:1.,media_duration:Some(media_duration),speed_curve:vec![],track:0,fade_in:0.,fade_out:0.,animation:vec![],hold:false,muted:false,has_audio:Some(true),volume:1.,transition:None,video_fade_in:0.,transition_shapes:vec![],scale:1.,offset_x:0.,offset_y:0.,rotation:0.,opacity,hidden:false};
 let mut clips=vec![clip(0.,length,1.,1.+length,0.5),clip(length,2.,head,10.,0.25)];clips[1].transition=Some(Request{kind:if white{"fade-white"}else{"fade-black"}.into(),duration:requested});
 assert!(resolve_transitions_at(&mut clips,FrameRate,0.).is_empty());assert_eq!(clips.len(),2);assert_eq!(clips[0].duration,length);assert_eq!(clips[1].start,length);assert_eq!(clips[1].track,0);assert!(clips.iter().all(|c|c.fade_in==0.&&c.fade_out==0.&&c.video_fade_in==0.));
 for frame in 0..111{let index=if frame<quantise(length,FrameRate){Some(0)}else if frame<quantise(length,FrameRate)+50{Some(1)}else{None};
  if let Some(index)=index{let c=&clips[index];let start=quantise(c.start,FrameRate);let shade=Shading::of(&transitions_at(&c.transition_shapes,frame-start));println!("-17 {} {} {} {} {} {} {} {} {} {} {} {}",white as u8,requested,short as u8,head,frame,index,start,quantise(c.duration,FrameRate),shade.scale,shade.offset[0],c.opacity,quantise(c.source_start,FrameRate)+frame-start);}
  else{println!("-17 {} {} {} {} {} -1 0 0 1 0 0 -1",white as u8,requested,short as u8,head,frame);}
 }
}}}}
for first in [false,true]{for second in [false,true]{for reversed in [false,true]{
 let clip=|start,duration,opacity|ExportClip{kind:ClipKind::Video,start,duration,source_start:1.,speed:1.,media_duration:Some(10.),speed_curve:vec![],track:0,fade_in:0.,fade_out:0.,animation:vec![],hold:false,muted:false,has_audio:Some(true),volume:1.,transition:None,video_fade_in:0.,transition_shapes:vec![],scale:1.,offset_x:0.,offset_y:0.,rotation:0.,opacity,hidden:false};
 let mut clips=vec![clip(0.,2.,0.5),clip(2.,0.4,0.25),clip(2.4,2.,0.75)];clips[1].transition=Some(Request{kind:if first{"fade-white"}else{"fade-black"}.into(),duration:2.});clips[2].transition=Some(Request{kind:if second{"fade-white"}else{"fade-black"}.into(),duration:2.});if reversed{clips.swap(1,2);}assert!(resolve_transitions_at(&mut clips,FrameRate,0.).is_empty());if reversed{clips.swap(1,2);}
 for frame in 0..111{let index=if frame<50{Some(0)}else if frame<60{Some(1)}else if frame<110{Some(2)}else{None};if let Some(index)=index{let c=&clips[index];let shade=Shading::of(&transitions_at(&c.transition_shapes,frame-quantise(c.start,FrameRate)));println!("-18 {} {} {} {} {} {} {} {}",first as u8,second as u8,reversed as u8,frame,index,shade.scale,shade.offset[0],c.opacity);}else{println!("-18 {} {} {} {} -1 1 0 0",first as u8,second as u8,reversed as u8,frame);}}
}}}
}
'''
prefix=prefix.replace('pub fn motions(){',colour_oracle+'pub fn motions(){',1)
# Recognize only the existing packaged dissolve for mixed legacy/package lane checks.
prefix=prefix.replace('impl Catalogue{fn builtin()->Self{Self}fn get(&self,_:&str)->Option<Package>{None}}impl Package{fn transition(&self)->Option<()>{None}}','impl Catalogue{fn builtin()->Self{Self}fn get(&self,id:&str)->Option<Package>{if id=="concat.dissolve"{Some(Package)}else{None}}}impl Package{fn transition(&self)->Option<()>{Some(())}}',1)
mixed_colour=r'''
pub fn colour_lanes(){for colour_first in [false,true]{
 let clip=|start,duration|ExportClip{kind:ClipKind::Video,start,duration,source_start:1.,speed:1.,media_duration:Some(10.),speed_curve:vec![],track:0,fade_in:0.,fade_out:0.,animation:vec![],hold:false,muted:false,has_audio:Some(true),volume:1.,transition:None,video_fade_in:0.,transition_shapes:vec![],scale:1.,offset_x:0.,offset_y:0.,rotation:0.,opacity:1.,hidden:false};
 let mut clips=vec![clip(0.,2.),clip(2.,0.4),clip(2.4,2.)];clips[1].transition=Some(Request{kind:if colour_first{"fade-white"}else{"concat.dissolve"}.into(),duration:1.});clips[2].transition=Some(Request{kind:if colour_first{"concat.dissolve"}else{"fade-black"}.into(),duration:1.});let spans=resolve_transitions_at(&mut clips,FrameRate,0.);assert_eq!(spans.len(),1);let span=&spans[0];println!("-19 {} {} {} {}",colour_first as u8,span.to_track,span.start,span.end);
}}
'''
prefix=prefix.replace('pub fn motions(){',mixed_colour+'pub fn motions(){',1)
main=main.replace('legacy_cross_fade::wipes();}','legacy_cross_fade::wipes();legacy_cross_fade::colours();legacy_cross_fade::colour_lanes();}')

offgrid_colour=r'''
pub fn offgrid_colours(){for white in [false,true]{for start in [0.,0.019,0.02,0.021]{for length in [0.001,0.019,0.02,0.021,2.019,2.02,2.021]{for source_start in [0.019,0.02,0.021]{
 let clip=|start,duration|ExportClip{kind:ClipKind::Video,start,duration,source_start,speed:1.,media_duration:Some(10.),speed_curve:vec![],track:0,fade_in:0.,fade_out:0.,animation:vec![],hold:false,muted:false,has_audio:Some(true),volume:1.,transition:None,video_fade_in:0.,transition_shapes:vec![],scale:1.,offset_x:0.,offset_y:0.,rotation:0.,opacity:1.,hidden:false};
 let mut clips=vec![clip(start,length),clip(start+length,2.)];clips[1].transition=Some(Request{kind:if white{"fade-white"}else{"fade-black"}.into(),duration:1.});assert!(resolve_transitions_at(&mut clips,FrameRate,0.).is_empty());
 let mut timeline=Timeline{tracks:vec![Track{clips:vec![0,1]}],clips:vec![]};
 for c in &clips{timeline.clips.push(EngineClip{start:quantise(c.start,FrameRate),duration:quantise(c.duration,FrameRate),source_start:quantise(c.source_start,FrameRate),hold:false});}
 for frame in 0..105{if let Some(index)=timeline.clip_on_track_at(0,frame){let c=&clips[index];let start_frame=quantise(c.start,FrameRate);let shade=Shading::of(&transitions_at(&c.transition_shapes,frame-start_frame));println!("-20 {} {} {} {} {} {} {} {} {}",white as u8,start,length,source_start,frame,index,quantise(c.source_start,FrameRate)+frame-start_frame,shade.scale,shade.offset[0]);}else{println!("-20 {} {} {} {} {} -1 -1 1 0",white as u8,start,length,source_start,frame);}}
}}}}}
'''
prefix=prefix.replace('pub fn motions(){',offgrid_colour+'pub fn motions(){',1)
main=main.replace('legacy_cross_fade::colour_lanes();}', 'legacy_cross_fade::colour_lanes();legacy_cross_fade::offgrid_colours();}')

rs = out / 'main.rs'
rs.write_text(prefix + functions + main)
subprocess.run(['rustc','+1.89.0','--edition=2021',str(rs),'-o',str(out/'oracle')],check=True)
result = subprocess.run([str(out/'oracle')],check=True,capture_output=True,text=True)
(out/'placement.txt').write_text(result.stdout)
print(result.stdout,end='')
