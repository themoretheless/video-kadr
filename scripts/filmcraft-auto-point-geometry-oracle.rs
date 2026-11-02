// Independent original project-loader automatic baseline resolution.
use filmcraft_project::{ParamKind,ParamValue,Keyframe};
use filmcraft_geom::Vec2;
use filmcraft_time::Tick;
use serde_json::json;
fn main(){let mut cases=vec![];let mut controls=0;
for def in filmcraft_project::effect_defs(){for control in def.params.iter().filter(|p|p.animatable&&matches!(p.kind,ParamKind::Point)){
controls+=1;
for (frame,source) in [((4,4),(4,4)),((1920,1080),(720,480)),((123,57),(31,17))]{for pattern in 0..4{for keyed in [false,true]{
let mut effect=def.instance();let p=effect.params.get_mut(control.id).unwrap();
p.value=ParamValue::Vec2(Vec2::new(if pattern&1!=0{f64::NAN}else{1.25000000000001},if pattern&2!=0{f64::NAN}else{-2.50000000000002}));
if keyed{p.keyframes=vec![Keyframe::new(Tick(17),ParamValue::Vec2(Vec2::new(0.25,-0.75))),Keyframe::new(Tick(254016000017),ParamValue::Vec2(Vec2::new(3.25,5.75)))];}
let before=effect.params.get(control.id).unwrap().clone();
filmcraft_project::resolve_auto_points(&mut effect,frame,source);
let after=effect.params.get(control.id).unwrap();
cases.push(json!({"effect":def.id,"parameter":control.id,"frame":frame,"source":source,"before":before,"after":after}));
}}}
}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","controls":controls,"cases":cases}));}
