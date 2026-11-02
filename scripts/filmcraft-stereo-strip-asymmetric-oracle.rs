use filmcraft_project::{Project,ItemKind,MediaClip,MediaRef,Label,SequenceSettings,TrackItem,Param};
use filmcraft_project::mixer::AutomationMode;
use filmcraft_media::{Generator,MediaSource,SharedSource,generators::GeneratorSource};
use filmcraft_time::{Tick,FrameRate};
use filmcraft_render::mixer::mix_graph;
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
  for i in 0..frames {let n=start+i as i64;if n<0 {continue;}out.channels[0][i]=((n%31)-15)as f32/32.;out.channels[1][i]=((n%23)-7)as f32/64.;}
  Ok(out)
 }
}
fn main(){let mut cases=vec![];
for sr in [44100u32,48000,96000]{for mode in AutomationMode::ALL{for kind in 0..12{for start in [0i64,1,400,500,1000]{
let mut p=Project::new("Fader PCM");let generator=Generator::Tone{hz:440.,db:-6.};let src=GeneratorSource::new(generator.clone(),4,4,FrameRate::new(24,1),Tick(10*T));
let media=p.add_item("Tone",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::Generator(generator),info:src.info().clone(),interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);
let src=Arc::new(AsymmetricSource{base:src})as SharedSource;let sources=|id|if id==media{Some(src.clone())}else{None};
let sid=p.new_sequence("Sequence",SequenceSettings{sample_rate:sr,..Default::default()},0,1,None);
p.sequence_mut(sid).unwrap().audio_tracks[0].items.push(serde_json::from_value::<TrackItem>(json!({"id":100,"item":media,"name":"Tone","label":"Iris","start":0,"duration":10*T,"source_in":0,"speed":1.,"enabled":true,"effects":[]})).unwrap());
let mut seq=p.sequence(sid).unwrap().clone();let input=bits(filmcraft_render::audio::track_input(&p,&seq.audio_tracks[0],start,17,sr,&sources));
seq.audio_tracks[0].mixer.mode=mode;
if kind<5 {seq.audio_tracks[0].volume_db=[-120.,-96.,-95.9,0.,15.][kind];}else{
seq.audio_tracks[0].volume_db=-13.25;let interp=["Linear","Hold","Bezier","AutoBezier","ContinuousBezier","EaseIn","EaseOut"][kind-5];
let param=json!({"value":{"Float":-8.},"keyframes":[{"time":0,"value":{"Float":-96.},"interp":interp},{"time":T/100,"value":{"Float":6.},"interp":"Linear"}]});seq.audio_tracks[0].mixer.lanes.insert("volume".into(),serde_json::from_value::<Param>(param).unwrap());}
seq.audio_tracks[0].pan=[-100.,-50.,0.,50.,100.][kind%5];seq.audio_tracks[0].muted=kind==0;
seq.audio_tracks[0].channels=if kind%2==0{filmcraft_project::AudioChannels::Mono}else{filmcraft_project::AudioChannels::Stereo};
seq.audio_tracks[0].mixer.input_map=[filmcraft_project::InputMap::Stereo,filmcraft_project::InputMap::Left,filmcraft_project::InputMap::Right,filmcraft_project::InputMap::Swap,filmcraft_project::InputMap::Mono][kind%5];
if kind>=5 {
let interp=["Linear","Hold","Bezier","AutoBezier","ContinuousBezier","EaseIn","EaseOut"][kind-5];
seq.audio_tracks[0].mixer.lanes.insert("pan".into(),serde_json::from_value(json!({"value":{"Float":25.},"keyframes":[{"time":0,"value":{"Float":-100.},"interp":interp},{"time":T/100,"value":{"Float":100.},"interp":"Linear"}]})).unwrap());
seq.audio_tracks[0].mixer.lanes.insert("mute".into(),serde_json::from_value(json!({"value":{"Bool":true},"keyframes":[{"time":T/200,"value":{"Bool":false},"interp":"Linear"},{"time":T/100,"value":{"Bool":true},"interp":"Linear"}]})).unwrap());
}
let output=bits(mix_graph(&p,&seq,start,17,&sources,None));
cases.push(json!({"track":seq.audio_tracks[0],"rate":sr,"start":start,"input":input,"bits":output}));
}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
