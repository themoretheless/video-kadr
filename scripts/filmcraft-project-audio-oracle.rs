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
fn clip(id:u64,source:filmcraft_project::ItemId,start:i64,duration:i64,source_in:i64,speed:f64,reverse:bool)->TrackItem{serde_json::from_value(json!({"id":id,"item":source,"name":"Nested audio","label":"Iris","start":start,"duration":duration,"source_in":source_in,"speed":speed,"reverse":reverse,"enabled":true,"effects":[]})).unwrap()}
fn emit(p:&Project,root:filmcraft_project::ItemId,media:filmcraft_project::ItemId,provider:&dyn SourceProvider)->serde_json::Value {
 mixer::clear_test_cache();audio_fx::clear_test_cache();
 let requests:Vec<_>=[(0,65),(65,192),(-17,31),(24003,257)].into_iter().map(|(start,frames)|json!({"start":start,"frames":frames,"bits":bits(mixer::mix_graph(p,p.sequence(root).unwrap(),start,frames,provider,None))})).collect();
 json!({"document":{"format":"filmcraft.project","schema_version":12,"project":p},"root":root,"source":media,"requests":requests})
}
fn main(){let mut cases=vec![];
for rate in [44100u32,48000,96000]{for own in [44100u32,48000,96000]{for mode in 0..4 {for angle in [None,Some(0),Some(1)] {for speed in [0.5,1.,1.25] {for reverse in [false,true] {
let mut p=Project::new("Nested PCM");let generator=Generator::Tone{hz:440.,db:-6.};let base=GeneratorSource::new(generator.clone(),4,4,FrameRate::new(24,1),Tick(10*T));let media=p.add_item("Pattern",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::Generator(generator),info:base.info().clone(),interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);
let src=Arc::new(AsymmetricSource{base})as SharedSource;let provider=|id|if id==media{Some(src.clone())}else{None};
let child=p.new_sequence("Child",SequenceSettings{sample_rate:own,..Default::default()},2,3,None);
{let seq=p.sequence_mut(child).unwrap();for side in 0..3 {let mut item=clip(100+side as u64,media,-T,10*T,2*T,0.8,false);item.gain_db=-3.25;item.source_channels=match side{0=>vec![0,1],1=>vec![1,0],_=>vec![1]};seq.audio_tracks[side].items.push(item);seq.audio_tracks[side].volume_db=[-3.,1.25,-6.][side];}
let delay=filmcraft_project::effect_defs().iter().find(|d|d.id=="delay").unwrap().instance();seq.audio_tracks[0].effects.push(delay);seq.master_volume_db=-2.5;
if mode>0 {seq.multicam=Some(filmcraft_project::MulticamSource{cameras:(0..3).map(|i|filmcraft_project::Camera{name:format!("Camera {i}"),video_track:if i<2{Some(seq.video_tracks[i].id)}else{None},audio_tracks:vec![seq.audio_tracks[i].id],enabled:i!=0,source:None}).collect(),audio:match mode{1=>filmcraft_project::MulticamAudio::Camera1,2=>filmcraft_project::MulticamAudio::AllCameras,_=>filmcraft_project::MulticamAudio::SwitchAudio},sync:"in".into()});}}
let root=p.new_sequence("Root",SequenceSettings{sample_rate:rate,..Default::default()},0,1,None);let mut item=clip(1000,child,7,5*T,Tick::from_units(-17,rate as i64).0,speed,reverse);if let Some(angle)=angle{item.multicam=Some(filmcraft_project::MulticamSel{enabled:true,angle});}item.gain_db=-1.25;
{let seq=p.sequence_mut(root).unwrap();seq.audio_tracks[0].items.push(item);seq.audio_tracks[0].pan=-17.25;seq.master_volume_db=-1.75;}
cases.push(emit(&p,root,media,&provider));
}}}}}}
for depth in [7,8,9,99] {
let mut p=Project::new("Depth guard");let generator=Generator::Tone{hz:440.,db:-6.};let base=GeneratorSource::new(generator.clone(),4,4,FrameRate::new(24,1),Tick(10*T));let media=p.add_item("Pattern",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::Generator(generator),info:base.info().clone(),interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);let src=Arc::new(AsymmetricSource{base})as SharedSource;let provider=|id|if id==media{Some(src.clone())}else{None};
let mut root=p.new_sequence("Leaf",SequenceSettings::default(),0,1,None);let source=if depth==99{root}else{media};p.sequence_mut(root).unwrap().audio_tracks[0].items.push(clip(2000,source,0,5*T,0,1.,false));
if depth!=99{for level in 0..depth{let parent=p.new_sequence("Parent",SequenceSettings::default(),0,1,None);p.sequence_mut(parent).unwrap().audio_tracks[0].items.push(clip(2001+level as u64,root,0,5*T,0,1.,false));root=parent;}}
cases.push(emit(&p,root,media,&provider));
}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
