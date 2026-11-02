// Capture explicit transition intervals through original global insert.
use filmcraft_project::{Project,TrackItem,TrackKind,Transition};
use filmcraft_time::Tick;
use serde_json::json;
fn item(id:u64,start:i64,duration:i64)->TrackItem{
 serde_json::from_value(json!({"id":id,"item":1,"name":"media","label":"Iris","start":start,"duration":duration,"source_in":100,"speed":1,"enabled":true,"effects":[]})).unwrap()
}
fn main(){let mut cases=vec![];
 for(name,at,start,duration,from,to)in[
  ("before-pair",3,9,2,Some(100),Some(101)),
  ("at-pair-cut",10,9,2,Some(100),Some(101)),
  ("inside-incoming",11,9,2,Some(100),Some(101)),
  ("at-transition-start",9,9,2,Some(100),Some(101)),
  ("after-pair",20,9,2,Some(100),Some(101)),
  ("split-outgoing-fade",3,2,4,Some(100),None),
  ("split-incoming-fade",3,2,4,None,Some(100)),
  ("fade-in-spans-cut",10,9,2,None,Some(101)),
 ]{
  let mut p=Project::new("transition insert");let sid=p.new_sequence("s",Default::default(),1,1,None);
  let mut seq=p.sequence(sid).unwrap().clone();let target=seq.tracks(TrackKind::Video)[0].id;
  let track=seq.track_mut(target).unwrap();track.items=vec![item(100,0,10),item(101,10,10)];
  let transition:Transition=serde_json::from_value(json!({"id":500,"effect":{"effect":"cross_dissolve","enabled":true,"params":{}},"start":start,"duration":duration,"from":from,"to":to,"align":"CenterAtCut"})).unwrap();
  track.transitions.push(transition);let before=serde_json::to_value(&track).unwrap();
  let mut next_id=2000;let mut ctx=filmcraft_edit::EditCtx{next_id:&mut next_id,media_duration:&|_|None,media_start:&|_|Tick::ZERO,min_duration:Tick(1)};
  filmcraft_edit::insert(&mut seq,vec![(target,item(100000,at,2))],&mut ctx).unwrap();
  cases.push(json!({"name":name,"at":at,"duration":2,"before":before,"after":seq.track(target).unwrap()}));
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
