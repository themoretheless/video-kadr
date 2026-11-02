use filmcraft_project::{Param,ParamValue,Keyframe,Interpolation};
use filmcraft_time::Tick;
use filmcraft_geom::Vec2;
use serde_json::json;
fn main(){let mut cases=vec![];
let values=vec![ParamValue::Float(42.25),ParamValue::Vec2(Vec2::new(-8.5,17.25)),ParamValue::Color([0.1,0.3,0.5,0.7]),ParamValue::Text("Привет".into()),ParamValue::Bool(true),ParamValue::Choice(3),ParamValue::Curve(vec![[0.0,0.3],[1.0,0.7]]),ParamValue::Path(filmcraft_project::mask::MaskPath::rect(-2.5,-3.75,7.25,4.5))];
for (kind,value) in values.into_iter().enumerate() {let old=match kind{0=>ParamValue::Float(-7.5),1=>ParamValue::Vec2(Vec2::new(1.0,2.0)),2=>ParamValue::Color([1.0,0.0,0.0,1.0]),3=>ParamValue::Text("Начало".into()),4=>ParamValue::Bool(false),5=>ParamValue::Choice(1),6=>ParamValue::Curve(vec![[0.0,0.0],[1.0,1.0]]),_=>ParamValue::Path(filmcraft_project::mask::MaskPath::rect(0.0,0.0,10.0,20.0))};for interpolation in Interpolation::ALL{for time in [-508032000000i64,-254016000000,0,254016000000,508032000000]{
 let mut a=Keyframe::new(Tick(-254016000000),old.clone());a.interp=interpolation;a.out_influence=0.17;a.in_influence=0.8;
 let mut b=Keyframe::new(Tick(254016000000),old.clone());b.interp=interpolation;b.out_influence=0.8;b.in_influence=0.17;
 for animated in [false,true]{let param=Param{value:old.clone(),keyframes:if animated{vec![a.clone(),b.clone()]}else{vec![]}};let mut edited=param.clone();edited.set_at(Tick(time),value.clone());cases.push(json!({"param":param,"value":value,"time":time,"edited":edited}));}
}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
