// Actual original Param interpolation with NaN resolution after sampling.
use filmcraft_project::{Interpolation,Keyframe,Param,ParamValue};
use filmcraft_geom::Vec2;
use filmcraft_time::Tick;
use serde_json::json;
const T:i64=254016000000;
fn point(pattern:u8,x:f64,y:f64)->ParamValue {ParamValue::Vec2(Vec2::new(if pattern&1!=0{f64::NAN}else{x},if pattern&2!=0{f64::NAN}else{y}))}
fn main(){let mut cases=vec![];
for origin in [17,20*T+1]{for left in Interpolation::ALL{for right in Interpolation::ALL{for custom in [false,true]{for ap in 0..4{for bp in 0..4{
let mut a=Keyframe::new(Tick(origin),point(ap,-0.3,0.25));let mut b=Keyframe::new(Tick(origin+T),point(bp,0.5,-0.75));
a.interp=left;b.interp=right;if custom{a.out_influence=0.75;b.in_influence=0.2;}
let parameter=Param{value:point(0,1.,2.),keyframes:vec![a,b]};let fallback=[7.25,-3.5];
for offset in [-1,0,1,T/4-1,T/4,T/2+1,T-1,T,T+1]{let at=origin+offset;let v=parameter.vec2_at(Tick(at));
let sampled=[if v.x.is_nan(){fallback[0]}else{v.x},if v.y.is_nan(){fallback[1]}else{v.y}];
cases.push(json!({"parameter":parameter,"fallback":fallback,"at":at.to_string(),"bits":sampled.map(f64::to_bits)}));
}
}}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
