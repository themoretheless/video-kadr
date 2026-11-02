use filmcraft_project::{Param,ParamValue,Keyframe,Interpolation};use filmcraft_project::mask::{MaskPath,MaskVertex};use filmcraft_geom::Vec2;use filmcraft_time::Tick;use serde_json::json;
fn main(){const T:i64=254016000000;let mut cases=vec![];
for shape in 0..4{for outgoing in 0..7{for incoming in 0..7{
 let a=match shape{0|1=>MaskPath::rect(1.25,0.75,6.25,7.25),2=>{let mut p=MaskPath::ellipse(Vec2::new(4.25,3.75),Vec2::new(2.75,3.25));p.closed=false;p},_=>MaskPath::default()};
 let b=match shape{1=>MaskPath::polygon(&[Vec2::new(-0.5,1.25),Vec2::new(7.25,2.75),Vec2::new(3.25,8.5)]),3=>MaskPath{vertices:vec![MaskVertex::corner(Vec2::new(2.25,3.75))],closed:false},_=>MaskPath::ellipse(Vec2::new(3.25,4.5),Vec2::new(3.25,1.75))};
 let c=MaskPath::rect(-1.25,2.25,5.75,6.5);let mut param=Param::new(ParamValue::Path(a.clone()));
 let mut k0=Keyframe::new(Tick(0),ParamValue::Path(a));k0.interp=Interpolation::ALL[outgoing];k0.out_influence=0.17;
 let mut k1=Keyframe::new(Tick(T),ParamValue::Path(b));k1.interp=Interpolation::ALL[incoming];k1.in_influence=0.81;k1.out_influence=0.62;
 let mut k2=Keyframe::new(Tick(3*T),ParamValue::Path(c));k2.interp=Interpolation::ALL[(outgoing+incoming)%7];k2.in_influence=0.29;param.keyframes=vec![k0,k1,k2];
 for at in [-1,0,T/2,T-1,T,T+1,2*T,3*T-1,3*T,3*T+1]{let ParamValue::Path(path)=param.value_at(Tick(at))else{unreachable!()};let flats:Vec<_>=[1f32,0.75,0.5].into_iter().map(|scale|{let s=scale as f64;json!({"scale":scale,"bits":path.flatten(0.05/s.max(1e-6)).into_iter().map(|p|[(p.x*s)as f32,(p.y*s)as f32].map(f32::to_bits)).collect::<Vec<_>>()})}).collect();cases.push(json!({"parameter":param,"at":at,"shape":shape,"outgoing":outgoing,"incoming":incoming,"closed":path.closed,"bits":path.components().into_iter().map(f64::to_bits).collect::<Vec<_>>(),"flat":flats}));}
}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
