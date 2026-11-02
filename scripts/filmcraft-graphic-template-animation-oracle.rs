use filmcraft_project::{Param,ParamValue,Keyframe,Interpolation};
use filmcraft_geom::Vec2;
use filmcraft_time::Tick;
use serde_json::json;
fn main(){let mut cases=vec![];
let pairs=vec![
 (ParamValue::Float(-7.25),ParamValue::Float(19.75)),
 (ParamValue::Vec2(Vec2::new(-13.5,2.25)),ParamValue::Vec2(Vec2::new(22.75,-17.5))),
 (ParamValue::Color([0.1,0.2,0.3,0.4]),ParamValue::Color([0.9,0.7,0.6,0.8])),
 (ParamValue::Text("Привет".into()),ParamValue::Text("مرحبا".into())),
 (ParamValue::Bool(false),ParamValue::Bool(true)),
 (ParamValue::Choice(1),ParamValue::Choice(3)),
 (ParamValue::Curve(vec![[0.0,0.0],[1.0,1.0]]),ParamValue::Curve(vec![[0.0,1.0],[1.0,0.0]])),
 (ParamValue::Path(filmcraft_project::mask::MaskPath::rect(-2.5,-3.75,7.25,4.5)),ParamValue::Path(filmcraft_project::mask::MaskPath::ellipse(Vec2::new(4.25,5.75),Vec2::new(8.5,11.25)))),
];
for (kind,(a,b)) in pairs.into_iter().enumerate(){for ia in Interpolation::ALL{for ib in Interpolation::ALL{for influence in [0.0,0.17,0.8,1.5]{
 let mut left=Keyframe::new(Tick(-254016000000),a.clone());left.interp=ia;left.out_influence=influence;
 let mut right=Keyframe::new(Tick(508032000000),b.clone());right.interp=ib;right.in_influence=influence;
 let p=Param{value:a.clone(),keyframes:vec![left,right]};
 for time in [-508032000000i64,-254016000000,-254015999999,-127008000000,0,254016000000,508031999999,508032000000,762048000000]{cases.push(json!({"kind":kind,"param":p,"time":time,"value":p.value_at(Tick(time))}));}
}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
