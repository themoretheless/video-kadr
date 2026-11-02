// Unmodified pinned Filmcraft Param evaluation and actual serialized source data.
use filmcraft_project::{Interpolation, Keyframe, Param, ParamValue};
use filmcraft_time::Tick;
use serde_json::json;
const T:i64=254016000000;
fn main(){let mut cases=vec![];
 for origin in [17,20*T+1,35000000*T+17]{for left in Interpolation::ALL{for right in Interpolation::ALL{for custom in [false,true]{
  let mut a=Keyframe::new(Tick(origin),ParamValue::Float(5.));let mut b=Keyframe::new(Tick(origin+T),ParamValue::Float(10.));
  a.interp=left;b.interp=right;
  if custom{a.out_influence=0.75;b.in_influence=0.2;}
  let parameter=Param{value:ParamValue::Float(0.),keyframes:vec![a,b]};
  for offset in [-1,0,1,T/4-1,T/4,T/2+1,T-1,T,T+1]{
   let at=origin+offset;let value=parameter.f64_at(Tick(at));
   cases.push(json!({"parameter":parameter,"at":at.to_string(),"value":value,"bits":value.to_bits()}));
  }
 }}}}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
