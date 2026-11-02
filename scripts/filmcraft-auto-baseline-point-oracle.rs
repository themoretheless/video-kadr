// Actual serialized source Vec2/Color Params and independent evaluator outputs.
use filmcraft_project::{Interpolation,Keyframe,Param,ParamValue};
use filmcraft_geom::Vec2;
use filmcraft_time::Tick;
use serde_json::json;
const T:i64=254016000000;
fn main(){let mut cases=vec![];
 for kind in ["point"]{for auto in [0,1,2]{for origin in [17,20*T+1]{for left in Interpolation::ALL{for right in Interpolation::ALL{for custom in [false,true]{
  let (start,end)=if kind=="point"{(ParamValue::Vec2(Vec2::new(-0.3,0.25)),ParamValue::Vec2(Vec2::new(0.5,-0.75)))}else{(ParamValue::Color([0.1,0.2,0.3,0.4]),ParamValue::Color([0.9,0.8,0.7,0.6]))};
  let mut a=Keyframe::new(Tick(origin),start.clone());let mut b=Keyframe::new(Tick(origin+T),end);
  a.interp=left;b.interp=right;if custom{a.out_influence=0.75;b.in_influence=0.2;}
  let baseline=ParamValue::Vec2(Vec2::new(if auto!=1{f64::NAN}else{12.},if auto!=0{f64::NAN}else{34.}));
  let parameter=Param{value:baseline,keyframes:vec![a,b]};
  for offset in [-1,0,1,T/4-1,T/4,T/2+1,T-1,T,T+1]{let at=origin+offset;let value=parameter.value_at(Tick(at));
   let bits:Vec<u64>=match value{ParamValue::Vec2(v)=>vec![v.x.to_bits(),v.y.to_bits()],ParamValue::Color(v)=>v.map(|q|q.to_bits()as u64).to_vec(),_=>unreachable!()};
   cases.push(json!({"kind":kind,"parameter":parameter,"at":at.to_string(),"bits":bits}));
  }
 }}}}}
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
