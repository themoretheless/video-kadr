use filmcraft_captions::{Cue,Document,Format,WriteOptions};
use filmcraft_time::{Tick,TICKS_PER_SECOND};
use serde_json::json;
fn main(){
 let mut cases=Vec::new();
 for format in Format::ALL {for custom_name in [false,true] {
  let mut session=filmcraft_engine::Session::default();session.execute("file.openDemoProject",json!({})).unwrap();
  let rate=session.active_sequence().unwrap().settings.frame_rate;
  let document=Document{cues:vec![Cue{start:Tick(TICKS_PER_SECOND/2),end:Tick(TICKS_PER_SECOND*2-1),text:"Hello".into(),speaker:Some("Ann".into()),id:Some("cue-1".into()),settings:"align:start".into()}],blocks:vec!["NOTE retained".into()],warnings:vec![]};
  let bytes=filmcraft_captions::write(&document,format,WriteOptions{drop_frame:true,rate:Some(rate)});
  let before=serde_json::to_value(&session.active_sequence().unwrap().caption_tracks).unwrap();
  let path=format!("/captions/talk.{}",format.extension());
  let result=filmcraft_engine::captions::import(&mut session,&path,&bytes,format,custom_name.then_some("Custom captions")).unwrap();
  let after=serde_json::to_value(&session.active_sequence().unwrap().caption_tracks).unwrap();
  session.active_sequence().unwrap().check().unwrap();
  session.execute("edit.undo",json!({})).unwrap();
  assert_eq!(serde_json::to_value(&session.active_sequence().unwrap().caption_tracks).unwrap(),before);
  cases.push(json!({"format":format.extension(),"customName":custom_name,"frameRate":rate,"bytes":bytes,"result":result,"before":before,"after":after,"undoRestored":true}));
 }}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
