// Capture original move_items transition removal and unrelated preservation.
use filmcraft_project::{Project,TrackItem,TrackKind,Transition};
use filmcraft_time::Tick;
use serde_json::json;
fn item(id:u64,start:i64,duration:i64)->TrackItem{
 serde_json::from_value(json!({"id":id,"item":1,"name":"media","label":"Iris","start":start,"duration":duration,"source_in":100,"speed":1,"enabled":true,"effects":[]})).unwrap()
}
fn main(){let mut cases=vec![];
 for(name,moved,from,to,cross)in[
  ("move-pair",vec![100,101],Some(100),Some(101),false),
  ("move-outgoing",vec![100],Some(100),Some(101),false),
  ("move-incoming",vec![101],Some(100),Some(101),false),
  ("move-fade-out",vec![100],Some(100),None,false),
  ("move-fade-in",vec![101],None,Some(101),false),
  ("move-pair-new-track",vec![100,101],Some(100),Some(101),true),
  ("move-unrelated",vec![102],Some(100),Some(101),false),
 ]{
  let mut p=Project::new("transition insert");let sid=p.new_sequence("s",Default::default(),2,1,None);
  let mut seq=p.sequence(sid).unwrap().clone();let target=seq.tracks(TrackKind::Video)[0].id;
  let track=seq.track_mut(target).unwrap();track.items=vec![item(100,0,10),item(101,10,10),item(102,30,10)];
  let transition:Transition=serde_json::from_value(json!({"id":500,"effect":{"effect":"cross_dissolve","enabled":true,"params":{}},"start":9,"duration":2,"from":from,"to":to,"align":"CenterAtCut"})).unwrap();
  track.transitions.push(transition);let before=serde_json::to_value(&track).unwrap();
  let mut next_id=2000;let mut ctx=filmcraft_edit::EditCtx{next_id:&mut next_id,media_duration:&|_|None,media_start:&|_|Tick::ZERO,min_duration:Tick(1)};
  let dest=if cross{seq.tracks(TrackKind::Video)[1].id}else{target};
  let moves:Vec<_>=moved.iter().map(|id|(serde_json::from_value::<filmcraft_project::ClipId>(json!(*id)).unwrap(),dest,Tick(match id{100=>50,101=>60,_=>80}))).collect();
  filmcraft_edit::move_items(&mut seq,&moves,false,&mut ctx).unwrap();
  cases.push(json!({"name":name,"moves":moves,"before":before,"after":seq.track(target).unwrap(),"destination":seq.track(dest).unwrap()}));
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
