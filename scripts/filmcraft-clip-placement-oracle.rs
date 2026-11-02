// Original TrackItem clocks for exactly representable native placements.
use filmcraft_project::TrackItem;use filmcraft_time::Tick;use serde_json::json;
const T:i64=254016000000;
fn main(){let mut cases=vec![];
for start in [0,10*T]{for duration in [T,10*T]{for speed in [-2f64,-0.5,0.5,1.,1.75,2.,16.]{for reverse in [false,true]{for hold in 0..3{
let held=20*T+(duration as f64*speed.abs()).round() as i64/2;
let item:TrackItem=serde_json::from_value(json!({"id":100,"item":1,"name":"Native placement","label":"Iris","start":start,"duration":duration,"source_in":20*T,"speed":speed,"reverse":reverse,"frame_hold":if hold>0{Some(held)}else{None},"hold_filters":hold==2,"enabled":true,"effects":[]})).unwrap();
for offset in [-1,0,1,T/4,T,duration-1,duration]{let at=start+offset;cases.push(json!({"item":item,"at":at,"clocks":[item.source_time_at(Tick(at)).0,item.effect_time_at(Tick(at)).0,at-start]}));}
}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
