// Original Filmcraft add_transition interval replacement, MIT.
use filmcraft_project::{Project,TrackKind,Transition};
use filmcraft_time::Tick;
use serde_json::json;
fn transition(id:u64,start:i64,from:Option<u64>,to:Option<u64>)->Transition{
 serde_json::from_value(json!({"id":id,"effect":{"effect":"cross_dissolve","enabled":true,"params":{}},"start":start,"duration":4,"from":from,"to":to,"align":"CenterAtCut"})).unwrap()
}
fn main(){
 let sides=[(Some(100),Some(101)),(Some(100),Some(102)),(Some(102),Some(101)),(Some(102),Some(103)),(Some(100),None),(Some(102),None),(None,Some(101)),(None,Some(103))];
 let mut cases=vec![];
 for (old_index,(old_from,old_to)) in sides.iter().copied().enumerate(){
  for (new_index,(from,to)) in sides.iter().copied().enumerate(){
   for start in [5,6,7,9,12,13]{
    let mut p=Project::new("transition add");let sid=p.new_sequence("s",Default::default(),1,1,None);
    let mut seq=p.sequence(sid).unwrap().clone();let target=seq.tracks(TrackKind::Video)[0].id;
    let old=transition(500,9,old_from,old_to);seq.track_mut(target).unwrap().transitions.push(old.clone());
    let added=transition(999,start,from,to);
    let mut next_id=2000;let mut ctx=filmcraft_edit::EditCtx{next_id:&mut next_id,media_duration:&|_|None,media_start:&|_|Tick::ZERO,min_duration:Tick(1)};
    let id=filmcraft_edit::add_transition(&mut seq,target,added.clone(),&mut ctx).unwrap();
    cases.push(json!({"name":format!("{old_index}-{new_index}-{start}"),"before":old,"added":added,"allocated":id,"after":seq.track(target).unwrap().transitions}));
   }
  }
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
