// Full original source sequence with precise authored timing and legacy items.
use filmcraft_project::{Project,SequenceSettings,ItemKind,Label,TrackItem};
use filmcraft_time::{Tick,FrameRate};use serde_json::json;
const T:i64=254016000000;
fn main(){let mut p=Project::new("Precise source sequence");let source=p.add_item("Graphic",Label::Iris,ItemKind::Graphic{width:80,height:64,rate:FrameRate::new(24,1)},None);let sid=p.new_sequence("Sequence",SequenceSettings{width:80,height:64,..Default::default()},1,1,None);let mut cases=vec![];let mut items=vec![];
for start in [-T+17,17,10*T+1,9007199254740993]{for speed in [-2.,-0.5,0.,0.5,1.,1.75,2.]{for reverse in [false,true]{for hold in 0..3{for duration in [-T-3,0,T+3]{
let id=1000+items.len() as u64;let item:TrackItem=serde_json::from_value(json!({"id":id,"item":source,"name":"Preserved clip","label":"Iris","start":start,"duration":duration,"source_in":20*T+1,"speed":speed,"reverse":reverse,"frame_hold":if hold>0{Some(25*T+17)}else{None},"hold_filters":hold==2,"enabled":true,"effects":[]})).unwrap();
for offset in [-1,0,1,T/4,T,T+3,T+4]{let at=start+offset;cases.push(json!({"id":id,"index":items.len(),"at":at,"clocks":[item.source_time_at(Tick(at)).0,item.effect_time_at(Tick(at)).0,at-start]}));}
items.push(item);
}}}}}
p.sequence_mut(sid).unwrap().video_tracks[0].items=items;p.next_id=10000;
let track=&p.sequence(sid).unwrap().video_tracks[0];let lookup:Vec<_>=[-T,0,17,18,T,10*T,9007199254740993,9007199254740993+T+4].into_iter().map(|at|json!({"at":at,"id":track.item_at(Tick(at)).map(|item|item.id)})).collect();
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","document":{"format":"filmcraft.project","schema_version":12,"project":p},"sequence":sid,"cases":cases,"lookup":lookup}));}
