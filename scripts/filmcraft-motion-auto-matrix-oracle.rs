// Actual original intrinsic Motion matrix with automatic keyed points.
use filmcraft_project::{Project,SequenceSettings,TrackItem,ParamValue,Keyframe,Interpolation};
use filmcraft_geom::Vec2;
use filmcraft_time::Tick;
use serde_json::json;
const T:i64=254016000000;
fn value(pattern:u8,x:f64,y:f64)->ParamValue{ParamValue::Vec2(Vec2::new(if pattern>0{f64::NAN}else{x},if pattern==2{f64::NAN}else{y}))}
fn main(){let mut cases=vec![];
let definition=filmcraft_project::effect_defs().iter().find(|d|d.id=="motion").unwrap();
for control in ["position","anchor"]{for (frame,source) in [((8,6),(4,2)),((123,57),(31,17))]{for interp in Interpolation::ALL{for ap in 0..3{for bp in 0..3{
let mut instance=definition.instance();
for (id,v) in [("scale",73.25),("scale_width",123.75),("rotation",31.5)]{instance.params.get_mut(id).unwrap().value=ParamValue::Float(v);}
instance.params.get_mut("uniform_scale").unwrap().value=ParamValue::Bool(false);
let mut a=Keyframe::new(Tick(17),value(ap,1.25,0.5));let mut b=Keyframe::new(Tick(T+17),value(bp,3.25,2.75));a.interp=interp;b.interp=interp;a.out_influence=0.75;b.in_influence=0.2;
instance.params.get_mut(control).unwrap().keyframes=vec![a,b];
filmcraft_project::resolve_auto_points(&mut instance,frame,source);
let mut p=Project::new("automatic Motion");let sid=p.new_sequence("s",SequenceSettings{width:frame.0,height:frame.1,..Default::default()},1,0,None);
let item:TrackItem=serde_json::from_value(json!({"id":100,"item":1,"name":"test","label":"Iris","start":0,"duration":10*T,"source_in":0,"speed":1,"enabled":true,"effects":[instance.clone()]})).unwrap();
for at in [16,17,18,T/4+16,T/4+17,T/2+18,T+16,T+17,T+18]{
let m=filmcraft_render::motion_matrix(p.sequence(sid).unwrap(),&item,source,Tick(at));
cases.push(json!({"parameters":instance.params,"frame":frame,"source":source,"at":at.to_string(),"bits":([m.a,m.b,m.c,m.d,m.e,m.f].map(f64::to_bits))}));
}
}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
