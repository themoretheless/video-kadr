pub use filmcraft_render::{SourceProvider,MAX_NEST_DEPTH,transitions,remix};
#[path="__AUDIO_PATH__"] pub mod audio;
#[path="__AUDIO_FX_PATH__"] pub mod audio_fx;
mod mixer;
use filmcraft_project::{Project,ItemKind,MediaClip,MediaRef,Label,SequenceSettings,TrackItem,EffectKind};
use filmcraft_media::{Generator,MediaSource,SharedSource,generators::GeneratorSource};
use filmcraft_time::{Tick,FrameRate};
use serde_json::json;
use std::sync::Arc;
const T:i64=254016000000;
fn bits(buffer:filmcraft_frame::AudioBuffer)->Vec<Vec<u32>>{buffer.channels.iter().map(|c|c.iter().map(|v|v.to_bits()).collect()).collect()}
struct AsymmetricSource { base:GeneratorSource }
impl MediaSource for AsymmetricSource {
 fn info(&self)->&filmcraft_media::MediaInfo {self.base.info()}
 fn video_frame(&self,request:filmcraft_media::FrameRequest)->filmcraft_media::Result<Arc<filmcraft_frame::VideoFrame>> {self.base.video_frame(request)}
 fn audio(&self,start:i64,frames:usize,rate:u32)->filmcraft_media::Result<filmcraft_frame::AudioBuffer>{
  let mut out=filmcraft_frame::AudioBuffer::silence(rate,2,frames);
  for i in 0..frames {let n=start+i as i64;if n<0 {continue;}out.channels[0][i]=(((n+0)%31)-15)as f32/32.;out.channels[1][i]=(((n+0)%23)-7)as f32/64.;}
  Ok(out)
 }
}
fn main(){let mut cases=vec![];
for definition in filmcraft_project::effect_defs().iter().filter(|d|d.kind==EffectKind::Audio&&!d.intrinsic){for scenario in 0..4 {for post in [false,true] {
let mut p=Project::new("Clip DSP");let generator=Generator::Tone{hz:440.,db:-6.};let base=GeneratorSource::new(generator.clone(),4,4,FrameRate::new(24,1),Tick(5*T));
let media=p.add_item("Pattern",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::Generator(generator),info:base.info().clone(),interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);
let src=Arc::new(AsymmetricSource{base})as SharedSource;let provider=|id|if id==media{Some(src.clone())}else{None};
let sid=p.new_sequence("Sequence",SequenceSettings::default(),0,1,None);let mut effect=definition.instance();effect.post_fader=post;
if scenario==1 {if let Some(param)=definition.params.iter().find(|param|param.animatable&&param.default.as_f64().is_some()) {let a=param.default.as_f64().unwrap();let b=a*0.8+0.1;effect.params.insert(param.id.into(),serde_json::from_value( json!({"value":{"Float":a},"keyframes":[{"time":0,"value":{"Float":a},"interp":"Bezier"},{"time":2*T,"value":{"Choice":b.max(0.)as u32},"interp":"Linear"}]})).unwrap());}}
if scenario==2 {effect.params.insert("bypass".into(),filmcraft_project::Param::new(filmcraft_project::ParamValue::Bool(true)));}
let mut effects=vec![];if scenario==3 {for _ in 0..5 {let mut disabled=effect.clone();disabled.enabled=false;effects.push(disabled);}}
effects.push(effect);if scenario==3 {let mut gain=filmcraft_project::effect_defs().iter().find(|d|d.id=="amplify").unwrap().instance();gain.params.insert("gain".into(),filmcraft_project::Param::new(filmcraft_project::ParamValue::Float(-1.25)));effects.push(gain);}
let mut clip:TrackItem=serde_json::from_value(json!({"id":100,"item":media,"name":"Pattern","label":"Iris","start":7,"duration":5*T,"source_in":Tick::from_units(37,48000),"speed":if scenario==1{1.25}else{1.},"reverse":scenario==1,"enabled":true,"gain_db":-3.25,"frame_hold":if post{Some(Tick(T))}else{None},"effects":[]})).unwrap();clip.effects=effects;
p.sequence_mut(sid).unwrap().audio_tracks[0].items.push(clip);
let track=&p.sequence(sid).unwrap().audio_tracks[0];audio_fx::clear_test_cache();
let requests:Vec<_>=[(48001,65),(48066,192),(-17,31),(24003,257)].into_iter().map(|(start,frames)|json!({"start":start,"frames":frames,"bits":bits(audio::track_input(&p,track,start,frames,48000,&provider))})).collect();
cases.push(json!({"track":track,"source":media,"rate":48000,"requests":requests,"effect":definition.id,"scenario":scenario,"post":post}));
}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
