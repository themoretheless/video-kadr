// Original held clocks deliberately do not evaluate the overflowing moving clock.
use filmcraft_project::TrackItem;use filmcraft_time::Tick;use serde_json::json;
const T:i64=254016000000;
fn main(){let mut cases=vec![];
for speed in [1e100,-1e100,0.]{for held in [0,-17,25*T+17,i64::MAX]{
let item:TrackItem=serde_json::from_value(json!({"id":100,"item":1,"name":"Held","label":"Iris","start":17,"duration":T,"source_in":i64::MAX,"speed":speed,"reverse":true,"frame_hold":held,"hold_filters":true,"enabled":true,"effects":[]})).unwrap();
for at in [16,17,T+18]{cases.push(json!({"item":item,"at":at,"clocks":[item.source_time_at(Tick(at)).0,item.effect_time_at(Tick(at)).0,at-item.start.0]}));}
}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
