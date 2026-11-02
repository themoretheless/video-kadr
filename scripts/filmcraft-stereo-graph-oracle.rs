use filmcraft_project::{Project,ItemKind,MediaClip,MediaRef,Label,SequenceSettings,TrackItem,Param};
use filmcraft_project::mixer::AutomationMode;
use filmcraft_media::{Generator,MediaSource,SharedSource,generators::GeneratorSource};
use filmcraft_time::{Tick,FrameRate};
use filmcraft_render::mixer::mix_graph;
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
for sr in [44100u32,48000,96000]{for route in 0..8 {for solo in 0..6 {for start in [0i64,401,1001] {
let mut p=Project::new("Graph");let mut sources=vec![];let mut media=vec![];
for side in 0..2 {let generator=Generator::Tone{hz:440.,db:-6.};let base=GeneratorSource::new(generator.clone(),4,4,FrameRate::new(24,1),Tick(10*T));let id=p.add_item("Pattern",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::Generator(generator),info:base.info().clone(),interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);media.push(id);sources.push((id,Arc::new(AsymmetricSource{base,side})as SharedSource));}
let provider=|id|sources.iter().find(|(key,_)|*key==id).map(|(_,source)|source.clone());
let sid=p.new_sequence("Sequence",SequenceSettings{sample_rate:sr,..Default::default()},0,2,None);
let seq=p.sequence_mut(sid).unwrap();
for i in 0..2 {seq.audio_tracks[i].id=filmcraft_project::TrackId(10+i as u64);seq.audio_tracks[i].items.push(serde_json::from_value::<TrackItem>(json!({"id":200+i,"item":media[i],"name":"Pattern","label":"Iris","start":0,"duration":10*T,"source_in":0,"speed":1.,"enabled":true,"effects":[]})).unwrap());seq.audio_tracks[i].volume_db=-3.*i as f64;seq.audio_tracks[i].pan=if i==0{-31.}else{23.};}
let mut a=seq.audio_tracks[0].clone();a.id=filmcraft_project::TrackId(100);a.items.clear();a.volume_db=-2.5;a.pan=-11.;
let mut b=a.clone();b.id=filmcraft_project::TrackId(101);b.volume_db=1.25;b.pan=17.;seq.submix_tracks=vec![a,b];
let target=|id|Some(filmcraft_project::TrackId(id));
match route {
0=>{},1=>{seq.audio_tracks[0].mixer.output=target(100);seq.audio_tracks[1].mixer.output=target(100);},
2=>{seq.audio_tracks[0].mixer.output=target(100);seq.audio_tracks[1].mixer.output=target(101);seq.submix_tracks[0].mixer.output=target(101);},
3=>{seq.audio_tracks[0].mixer.output=target(101);seq.audio_tracks[1].mixer.output=target(100);seq.submix_tracks[0].mixer.output=target(101);},
4=>{seq.audio_tracks[0].mixer.output=target(999);seq.audio_tracks[1].mixer.output=target(100);seq.submix_tracks[0].mixer.output=target(100);},
5=>{seq.audio_tracks[0].mixer.output=target(100);seq.audio_tracks[1].mixer.output=target(101);seq.submix_tracks[1].mixer.output=target(100);},
6=>{seq.audio_tracks[0].mixer.output=target(100);seq.audio_tracks[1].mixer.output=target(100);seq.submix_tracks[0].muted=true;},
_=>{seq.audio_tracks[0].enabled=false;seq.audio_tracks[1].mixer.output=target(101);}}
match solo{1=>seq.audio_tracks[0].solo=true,2=>seq.audio_tracks[1].solo=true,3=>seq.submix_tracks[0].solo=true,4=>seq.submix_tracks[1].solo=true,5=>{seq.audio_tracks[0].solo=true;seq.audio_tracks[1].mixer.solo_safe=true;},_=>{}}
seq.master_volume_db=-1.75;
let seq=p.sequence(sid).unwrap();let inputs:Vec<_>=seq.audio_tracks.iter().map(|track|json!({"id":track.id,"bits":bits(filmcraft_render::audio::track_input(&p,track,start,31,sr,&provider))})).collect();
let output=bits(mix_graph(&p,seq,start,31,&provider,None));
cases.push(json!({"document":{"format":"filmcraft.project","schema_version":12,"project":p},"id":sid,"rate":sr,"start":start,"inputs":inputs,"bits":output}));
}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
