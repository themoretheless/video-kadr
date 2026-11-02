use filmcraft_project::{Project,ItemKind,MediaClip,MediaRef,Label,SequenceSettings,TrackItem,Param};
use filmcraft_project::mixer::AutomationMode;
use filmcraft_media::{Generator,MediaSource,SharedSource,generators::GeneratorSource};
use filmcraft_time::{Tick,FrameRate};
use filmcraft_render::mixer::mix_graph;
use serde_json::json;
use std::sync::Arc;
const T:i64=254016000000;
fn bits(buffer:filmcraft_frame::AudioBuffer)->Vec<Vec<u32>>{buffer.channels.iter().map(|c|c.iter().map(|v|v.to_bits()).collect()).collect()}
fn main(){let mut cases=vec![];
for sr in [44100u32,48000,96000]{for mode in AutomationMode::ALL{for kind in 0..12{for start in [0i64,1,400,500,1000]{
let mut p=Project::new("Fader PCM");let generator=Generator::Tone{hz:440.,db:-6.};let src=GeneratorSource::new(generator.clone(),4,4,FrameRate::new(24,1),Tick(10*T));
let media=p.add_item("Tone",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::Generator(generator),info:src.info().clone(),interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);
let src=Arc::new(src)as SharedSource;let sources=|id|if id==media{Some(src.clone())}else{None};
let sid=p.new_sequence("Sequence",SequenceSettings{sample_rate:sr,..Default::default()},0,1,None);
p.sequence_mut(sid).unwrap().audio_tracks[0].items.push(serde_json::from_value::<TrackItem>(json!({"id":100,"item":media,"name":"Tone","label":"Iris","start":0,"duration":10*T,"source_in":0,"speed":1.,"enabled":true,"effects":[]})).unwrap());
let mut seq=p.sequence(sid).unwrap().clone();let input=bits(mix_graph(&p,&seq,start,17,&sources,None));
seq.master_mixer.mode=mode;
if kind<5 {seq.master_volume_db=[-120.,-96.,-95.9,0.,15.][kind];}else{
seq.master_volume_db=-13.25;let interp=["Linear","Hold","Bezier","AutoBezier","ContinuousBezier","EaseIn","EaseOut"][kind-5];
let param=json!({"value":{"Float":-8.},"keyframes":[{"time":0,"value":{"Float":-96.},"interp":interp},{"time":T/100,"value":{"Float":6.},"interp":"Linear"}]});seq.master_mixer.lanes.insert("volume".into(),serde_json::from_value::<Param>(param).unwrap());}
let output=bits(mix_graph(&p,&seq,start,17,&sources,None));
cases.push(json!({"sequence":{"master_volume_db":seq.master_volume_db,"master_mixer":seq.master_mixer},"rate":sr,"start":start,"input":input,"bits":output}));
}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
