// Original full scene: default video effects on each side, signed preroll and postroll clocks.
use filmcraft_project::{Project,ItemKind,MediaClip,MediaRef,Label,SequenceSettings,TrackItem,Transition};
use filmcraft_media::{Generator,MediaSource,SharedSource,generators::GeneratorSource};
use filmcraft_time::{Tick,FrameRate};
use filmcraft_render::{render_sequence,RenderOptions};
use serde_json::json;
use std::sync::Arc;
const T:i64=254016000000;
fn main(){let mut cases=vec![];
 let definitions:Vec<_>=filmcraft_project::effect_defs().iter().filter(|d|d.kind==filmcraft_project::EffectKind::Video&&!d.intrinsic).collect();
 let count=definitions.len();
 for definition in definitions {
 let effect=definition.id;
 for mode in 0..2{for reverse in [false,true]{for at in [9*T,10*T,11*T-1]{
  let mut p=Project::new("transition sequence");let rate=FrameRate::new(24,1);
  let mut sources=vec![];let mut media=vec![];
  for color in [[1.,0.,0.,1.],[0.,0.,1.,1.]]{
   let generator=Generator::ColorMatte{color};let src=GeneratorSource::new(generator.clone(),4,4,rate,Tick(100*T));let info=src.info().clone();
   let id=p.add_item("matte",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::Generator(generator),info,interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);
   media.push(id);sources.push((id,Arc::new(src)as SharedSource));
  }
  let sid=p.new_sequence("s",SequenceSettings{width:4,height:4,..Default::default()},1,0,None);
  let seq=p.sequence_mut(sid).unwrap();let track=&mut seq.video_tracks[0];
  for (index,id) in [100,101].iter().enumerate(){let item:TrackItem=serde_json::from_value(json!({"id":id,"item":media[index],"name":"matte","label":"Iris","start":index as i64*10*T,"duration":10*T,"source_in":20*T,"speed":1,"enabled":true,"effects":if index==mode {vec![definition.instance()]}else{vec![]}})).unwrap();track.items.push(item);}
  let tr:Transition=serde_json::from_value(json!({"id":500,"effect":{"effect":"cross_dissolve","enabled":true,"params":{}},"start":9*T,"duration":2*T,"from":Some(100),"to":Some(101),"align":"CenterAtCut","reverse":reverse})).unwrap();track.transitions.push(tr);
  if mode==5{let mut second=track.transitions[0].clone();second.id=serde_json::from_value(json!(501)).unwrap();second.from=None;second.to=Some(serde_json::from_value(json!(101)).unwrap());track.transitions.push(second);}
  let source=|id|sources.iter().find(|(key,_)|*key==id).map(|(_,source)|source.clone());
  let out=render_sequence(&p,sid,Tick(at),RenderOptions{working_output:true,..Default::default()},&source);
  cases.push(json!({"effect":effect,"mode":mode,"reverse":reverse,"at":at.to_string(),"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }}}
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","definitions":count,"cases":cases}));
}
