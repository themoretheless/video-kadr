pub use filmcraft_render::{SourceProvider,MAX_NEST_DEPTH,transitions,remix};
#[path="__AUDIO_PATH__"] pub mod audio;
#[path="__AUDIO_FX_PATH__"] pub mod audio_fx;
mod mixer;
use filmcraft_project::{Project,ItemKind,MediaClip,MediaRef,Label,SequenceSettings,TrackItem};
use filmcraft_media::{MediaSource,SharedSource};
use filmcraft_time::{Tick};
use serde_json::json;
use std::sync::Arc;
const T:i64=254016000000;
fn bits(buffer:filmcraft_frame::AudioBuffer)->Vec<Vec<u32>>{buffer.channels.iter().map(|c|c.iter().map(|v|v.to_bits()).collect()).collect()}
fn wav_bytes()->Vec<u8> {
 let mut b=b"RIFF\0\0\0\0WAVEfmt ".to_vec();b.extend(16u32.to_le_bytes());b.extend(1u16.to_le_bytes());b.extend(2u16.to_le_bytes());b.extend(48000u32.to_le_bytes());b.extend(192000u32.to_le_bytes());b.extend(4u16.to_le_bytes());b.extend(16u16.to_le_bytes());b.extend(b"data");b.extend(400000u32.to_le_bytes());
 for n in 0..100000i32 {b.extend(((((n*13)%31-15)*1973)as i16).to_le_bytes());b.extend(((((n*7)%23-11)*2137)as i16).to_le_bytes());}
 let len=b.len() as u32-8;b[4..8].copy_from_slice(&len.to_le_bytes());b
}
fn clip(id:u64,source:filmcraft_project::ItemId,start:i64,duration:i64,source_in:i64,speed:f64,reverse:bool)->TrackItem{serde_json::from_value(json!({"id":id,"item":source,"name":"Nested audio","label":"Iris","start":start,"duration":duration,"source_in":source_in,"speed":speed,"reverse":reverse,"enabled":true,"effects":[]})).unwrap()}
fn emit(p:&Project,root:filmcraft_project::ItemId,media:filmcraft_project::ItemId,provider:&dyn SourceProvider)->serde_json::Value {
 mixer::clear_test_cache();audio_fx::clear_test_cache();
 let requests:Vec<_>=[(0,65),(65,192),(-17,31),(24003,257)].into_iter().map(|(start,frames)|json!({"start":start,"frames":frames,"bits":bits(mixer::mix_graph(p,p.sequence(root).unwrap(),start,frames,provider,None))})).collect();
 json!({"document":{"format":"filmcraft.project","schema_version":12,"project":p},"root":root,"source":media,"requests":requests})
}
fn main(){let mut cases=vec![];let wav=wav_bytes();
for rate in [44100u32,48000]{for own in [44100u32,48000]{for mode in 0..1 {for angle in [None::<u32>] {for speed in [0.5,1.,1.25] {for reverse in [false,true] {
let mut p=Project::new("WAV nested PCM");let source=filmcraft_media::wav::WavSource::parse("pattern.wav",wav.clone().into()).unwrap();let media=p.add_item("Pattern",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::File{path:"pattern.wav".into()},info:source.info().clone(),interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);
let src=Arc::new(source)as SharedSource;let provider=|id|if id==media{Some(src.clone())}else{None};
let child=p.new_sequence("Child",SequenceSettings{sample_rate:own,..Default::default()},2,3,None);
{let seq=p.sequence_mut(child).unwrap();for side in 0..3 {let mut item=clip(100+side as u64,media,-T/100,2*T,T/10,0.8,false);item.gain_db=-3.25;item.effects.push(filmcraft_project::effect_defs().iter().find(|d|d.id=="amplify").unwrap().instance());item.source_channels=match side{0=>vec![0,1],1=>vec![1,0],_=>vec![1]};seq.audio_tracks[side].items.push(item);seq.audio_tracks[side].volume_db=[-3.,1.25,-6.][side];}
let delay=filmcraft_project::effect_defs().iter().find(|d|d.id=="delay").unwrap().instance();seq.audio_tracks[0].effects.push(delay);seq.master_volume_db=-2.5;
if mode>0 {seq.multicam=Some(filmcraft_project::MulticamSource{cameras:(0..3).map(|i|filmcraft_project::Camera{name:format!("Camera {i}"),video_track:if i<2{Some(seq.video_tracks[i].id)}else{None},audio_tracks:vec![seq.audio_tracks[i].id],enabled:i!=0,source:None}).collect(),audio:match mode{1=>filmcraft_project::MulticamAudio::Camera1,2=>filmcraft_project::MulticamAudio::AllCameras,_=>filmcraft_project::MulticamAudio::SwitchAudio},sync:"in".into()});}}
let root=p.new_sequence("Root",SequenceSettings{sample_rate:rate,..Default::default()},0,1,None);let mut item=clip(1000,child,7,T,Tick::from_units(-17,rate as i64).0,speed,reverse);if let Some(angle)=angle{item.multicam=Some(filmcraft_project::MulticamSel{enabled:true,angle});}item.gain_db=-1.25;
{let seq=p.sequence_mut(root).unwrap();seq.audio_tracks[0].items.push(item);seq.audio_tracks[0].pan=-17.25;seq.master_volume_db=-1.75;}
cases.push(emit(&p,root,media,&provider));
}}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","wav":wav,"cases":cases}));}
