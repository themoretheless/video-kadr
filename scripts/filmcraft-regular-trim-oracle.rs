use filmcraft_project::{Project,SequenceSettings,TrackItem,ClipId};
use filmcraft_edit::{EditCtx,Edge,TrimMode};
use filmcraft_time::{Tick,FrameRate};
use serde_json::json;
fn item(id:u64,start:i64,duration:i64)->TrackItem{serde_json::from_value(json!({"id":id,"item":999,"name":"clip","label":"Iris","start":start,"duration":duration,"source_in":200,"speed":1.,"reverse":false,"enabled":true,"effects":[]})).unwrap()}
fn main(){let mut cases=vec![];
for (scale,origin,minimum) in [(1i64,0i64,10i64),(10584000000,0,10584000000),(10584000000,9007199254740993,10584000000),(10584000000,i64::MAX/4-10584000000000,10584000000)] {
for speed in [0.,0.5,-1.25] {for reverse in [false,true] {for hold in [false,true] {for edge in [Edge::In,Edge::Out] {for delta in [-200,-50,-1,0,1,50,200] {for end in [None,Some(Tick(origin+500*scale))] {
let delta=delta*scale+if scale>1 {1}else{0};
let mut p=Project::new("Trim");let seq=p.new_sequence("Sequence",SequenceSettings{frame_rate:FrameRate::new(24,1),..Default::default()},0,1,None);let mut target=item(12,origin+100*scale,100*scale);target.source_in=Tick(origin+200*scale);target.speed=speed;target.reverse=reverse;target.frame_hold=hold.then_some(Tick(origin+123*scale));p.sequence_mut(seq).unwrap().audio_tracks[0].items=vec![item(11,origin,60*scale),target,item(13,origin+250*scale,100*scale)];
let mut document=json!({"format":"filmcraft.project","schema_version":12,"project":p,"opaque":{"large":18446744073709551615u64}});document["project"]["items"][seq.0.to_string()]["kind"]["Sequence"]["audio_tracks"][0]["items"][1]["extension"]=json!({"tick":9007199254740993u64});
let mut next=1000;let duration=|_|end;let start=|_|Tick(origin+100*scale);let mut ctx=EditCtx{next_id:&mut next,media_duration:&duration,media_start:&start,min_duration:Tick(minimum)};
let result=filmcraft_edit::trim(p.sequence_mut(seq).unwrap(),ClipId(12),edge,TrimMode::Regular,Tick(delta),&mut ctx);
let mut clip=serde_json::to_value(&p.sequence(seq).unwrap().audio_tracks[0].items[1]).unwrap();clip["extension"]=json!({"tick":9007199254740993u64});
cases.push(json!({"document":document,"sequence":seq,"edge":if edge==Edge::In{"in"}else{"out"},"delta":delta,"media_start":origin+100*scale,"minimum":minimum,"media_end":end,"error":result.is_err(),"applied":result.ok().map(|v|v.0),"clip":clip}));
}}}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
