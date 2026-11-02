use filmcraft_time::{FrameRate,Tick};use serde_json::json;
__DURATION__
fn main(){let mut cases=vec![];for (num,den) in [(24,1),(24000,1001),(25,1),(30,1),(30000,1001),(60,1),(7,3),(0,0)]{for unit in ["frames","seconds","other"]{for value in [-100.,-1.,0.,0.000001,0.01,0.5,0.5001,0.999,1.,1.5,2.5,23.976,24.,29.97,30.,100.,100000.]{let rate=FrameRate::new(num,den);cases.push(json!({"num":num,"den":den,"unit":unit,"value":value,"expected":duration_in(value,unit,rate)}));}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
