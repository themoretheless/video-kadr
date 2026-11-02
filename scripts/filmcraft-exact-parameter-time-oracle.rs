use filmcraft_project::{Interpolation,Keyframe,Param,ParamValue};
use filmcraft_time::Tick;
use serde_json::json;
const T:i64=254016000000;
fn main(){let mut cases=vec![];
 for origin in [0,20*T,35000000*T]{for interpolation in Interpolation::ALL{for custom in [false,true]{
  let mut a=Keyframe::new(Tick(origin),ParamValue::Float(5.));let mut b=Keyframe::new(Tick(origin+T),ParamValue::Float(10.));
  a.interp=interpolation;b.interp=interpolation;
  if custom{a.out_influence=0.75;b.in_influence=0.2;}
  let out_ease=matches!(a.interp,Interpolation::Bezier|Interpolation::AutoBezier|Interpolation::ContinuousBezier|Interpolation::EaseOut);
  let in_ease=matches!(b.interp,Interpolation::Bezier|Interpolation::AutoBezier|Interpolation::ContinuousBezier|Interpolation::EaseIn)||matches!(a.interp,Interpolation::Bezier|Interpolation::ContinuousBezier);
  let native=if interpolation==Interpolation::Hold{json!("hold")}else if !out_ease&&!in_ease{json!("linear")}else{json!({"kind":"bezier","p1":[if out_ease{a.out_influence.clamp(0.01,1.)}else{1./3.},if out_ease{0.}else{1./3.}],"p2":[if in_ease{1.-b.in_influence.clamp(0.01,1.)}else{2./3.},if in_ease{1.}else{2./3.}]})};
  let parameter=Param{value:ParamValue::Float(0.),keyframes:vec![a,b]};
  for offset in [-1,0,1,T/4-1,T/4,T/2+1,T-1,T,T+1]{
   let at=origin+offset;let value=parameter.f64_at(Tick(at));
   cases.push(json!({"sourceInterpolation":interpolation,"custom":custom,"origin":origin.to_string(),"at":at.to_string(),"nativeInterpolation":native,"value":value,"bits":value.to_bits()}));
  }
 }}}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
