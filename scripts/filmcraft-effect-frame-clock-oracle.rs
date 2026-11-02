// Source-owned clock matrix, independent of the host clock implementation.
use filmcraft_project::TrackItem;
use filmcraft_time::Tick;
use serde_json::json;
const T:i64=254016000000;
fn main(){
 let mut cases=vec![];
 for speed in [0.5,1.,2.]{for reverse in [false,true]{for hold in 0..3{
  let frame_hold=if hold>0{Some(25*T)}else{None};
  let item:TrackItem=serde_json::from_value(json!({"id":100,"item":1,"name":"clock","label":"Iris","start":10*T,"duration":10*T,"source_in":20*T,"speed":speed,"reverse":reverse,"frame_hold":frame_hold,"hold_filters":hold==2,"enabled":true,"effects":[]})).unwrap();
  for at in [9*T,10*T-1,10*T,10*T+1,11*T,20*T,21*T]{
   cases.push(json!({"input":{"timeline_start":10*T,"source_in":20*T,"duration":10*T,"speed":speed,"reverse":reverse,"frame_hold":frame_hold,"hold_filters":hold==2},"at":at,"frame":item.source_time_at(Tick(at)).0,"parameters":item.effect_time_at(Tick(at)).0,"generator":at-item.start.0}));
  }
 }}}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
