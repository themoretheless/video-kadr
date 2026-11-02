// Execute the pinned upstream global insert, including cross-track link allocation.
use filmcraft_project::{Project, Track, TrackId, TrackItem, TrackKind};
use filmcraft_time::Tick;
use serde_json::json;
fn item(id:u64,start:i64,duration:i64,reverse:bool,hold:bool)->TrackItem {
 serde_json::from_value(json!({"id":id,"item":1,"name":"media","label":"Iris","start":start,"duration":duration,"source_in":100,"speed":2,"reverse":reverse,"frame_hold":if hold {Some(150)}else{None},"link":77,"enabled":true,"effects":[]})).unwrap()
}
fn main(){
 let mut cases=vec![];
 for (name,locked,sync_lock,reverse,hold) in [
  ("active",false,true,false,false),("locked-other",true,true,false,false),
  ("sync-off-other",false,false,false,false),("reverse",false,true,true,false),
  ("frame-hold",false,true,false,true),("target-sync-off",false,false,false,false),
 ] {
  let mut p=Project::new("insert");let sid=p.new_sequence("s",Default::default(),1,1,None);
  let mut seq=p.sequence(sid).unwrap().clone();
  let target=seq.tracks(TrackKind::Video)[0].id;
  let target_track=seq.track_mut(target).unwrap();
  target_track.sync_lock=name!="target-sync-off";
  target_track.items=vec![item(100,0,10,reverse,hold),item(101,12,4,false,false)];
  let mut other=Track::new(TrackId(900),TrackKind::Audio,"partner".into());
  other.locked=locked;other.sync_lock=sync_lock;
  other.items=vec![item(102,0,10,reverse,hold),item(103,12,4,false,false)];
  seq.tracks_mut(TrackKind::Audio).push(other);
  let before=serde_json::to_value(seq.all_tracks().collect::<Vec<_>>()).unwrap();
  let mut placement=item(100000,3,2,false,false);placement.link=None;
  let mut next_id=2000;
  let mut ctx=filmcraft_edit::EditCtx{next_id:&mut next_id,media_duration:&|_|None,media_start:&|_|Tick::ZERO,min_duration:Tick(1)};
  let ids=filmcraft_edit::insert(&mut seq,vec![(target,placement)],&mut ctx).unwrap();
  cases.push(json!({"name":name,"at":"3","duration":"2","target":target,"before":before,"after":seq.all_tracks().collect::<Vec<_>>(),"insertedIds":ids,"nextId":next_id}));
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
