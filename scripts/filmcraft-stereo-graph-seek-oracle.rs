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
struct AsymmetricSource { base:GeneratorSource,side:i64 }
impl MediaSource for AsymmetricSource {
 fn info(&self)->&filmcraft_media::MediaInfo {self.base.info()}
 fn video_frame(&self,request:filmcraft_media::FrameRequest)->filmcraft_media::Result<Arc<filmcraft_frame::VideoFrame>> {self.base.video_frame(request)}
 fn audio(&self,start:i64,frames:usize,rate:u32)->filmcraft_media::Result<filmcraft_frame::AudioBuffer>{
  let mut out=filmcraft_frame::AudioBuffer::silence(rate,2,frames);
  for i in 0..frames {let n=start+i as i64;if n<0 {continue;}out.channels[0][i]=(((n+self.side*7)%31)-15)as f32/32.;out.channels[1][i]=(((n+self.side*11)%23)-7)as f32/64.;}
  Ok(out)
 }
}
fn main(){let mut cases=vec![];
for definition in filmcraft_project::effect_defs().iter().filter(|d|d.kind==EffectKind::Audio&&!d.intrinsic){for host in 0..3 {for post in [false,true] {
let mut p=Project::new("Insert graph");let mut sources=vec![];let mut media=vec![];
for side in 0..2 {let generator=Generator::Tone{hz:440.,db:-6.};let base=GeneratorSource::new(generator.clone(),4,4,FrameRate::new(24,1),Tick(10*T));let id=p.add_item("Pattern",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::Generator(generator),info:base.info().clone(),interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);media.push(id);sources.push((id,Arc::new(AsymmetricSource{base,side})as SharedSource));}
let provider=|id|sources.iter().find(|(key,_)|*key==id).map(|(_,source)|source.clone());
let sid=p.new_sequence("Sequence",SequenceSettings::default(),0,2,None);let seq=p.sequence_mut(sid).unwrap();
for i in 0..2 {seq.audio_tracks[i].id=filmcraft_project::TrackId(10+i as u64);seq.audio_tracks[i].items.push(serde_json::from_value::<TrackItem>(json!({"id":200+i,"item":media[i],"name":"Pattern","label":"Iris","start":0,"duration":10*T,"source_in":0,"speed":1.,"enabled":true,"effects":[]})).unwrap());seq.audio_tracks[i].volume_db=-3.*i as f64;seq.audio_tracks[i].pan=if i==0{-31.}else{23.};}
let mut a=seq.audio_tracks[0].clone();a.id=filmcraft_project::TrackId(100);a.items.clear();a.volume_db=-2.5;a.pan=-11.;let mut b=a.clone();b.id=filmcraft_project::TrackId(101);b.volume_db=1.25;b.pan=17.;seq.submix_tracks=vec![a,b];
seq.audio_tracks[0].mixer.output=Some(filmcraft_project::TrackId(100));seq.audio_tracks[1].mixer.output=Some(filmcraft_project::TrackId(101));seq.submix_tracks[0].mixer.output=Some(filmcraft_project::TrackId(101));
seq.audio_tracks[0].mixer.sends=vec![filmcraft_project::mixer::TrackSend{target:filmcraft_project::TrackId(101),level_db:-9.,pan:13.25,pre_fader:true,muted:false},filmcraft_project::mixer::TrackSend{target:filmcraft_project::TrackId(101),level_db:-12.,pan:-17.125,pre_fader:false,muted:false}];
seq.audio_tracks[1].mixer.sends=vec![filmcraft_project::mixer::TrackSend{target:filmcraft_project::TrackId(100),level_db:-15.,pan:29.5,pre_fader:true,muted:false}];
seq.master_volume_db=-1.75;
let mut effect=definition.instance();effect.post_fader=post;
match host{0=>seq.audio_tracks[0].effects=vec![effect],1=>seq.submix_tracks[0].effects=vec![effect],_=>seq.master_effects=vec![effect]}
let seq=p.sequence(sid).unwrap();
mixer::clear_test_cache();
let requests:Vec<_>=[(48001,65),(48066,192),(-17,31),(24003,257)].into_iter().map(|(start,frames)| {
 let reused=mixer::has_cached_graph(seq,start);
 let output=bits(mixer::mix_graph(&p,seq,start,frames,&provider,None));
 json!({"start":start,"frames":frames,"bits":output,"reused":reused})
}).collect();
cases.push(json!({"document":{"format":"filmcraft.project","schema_version":12,"project":p},"id":sid,"requests":requests,"effect":definition.id,"host":host,"post":post}));
}}}
assert!(cases.iter().all(|case| case["requests"].as_array().unwrap().iter().enumerate().all(|(i,r)|r["reused"].as_bool()==Some(i==1))));
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
