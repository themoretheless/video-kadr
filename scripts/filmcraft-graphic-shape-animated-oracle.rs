//! Original vector graphic shape layers, including complete appearance rendering.
use filmcraft_project::{Param,ParamValue};
use filmcraft_geom::Vec2;
use filmcraft_render::{graphic_clip::{layer_matrix,xform,raster_layer,composite_raster},image::Image};
use serde_json::json;
fn main(){let mut cases=vec![];
for shape in 0..4{for profile in 0..3{for appearance in 0..3{for enabled in[false,true]{for seconds in[-0.25,0.5,1.25]{
 let mut effect=filmcraft_project::effect_defs().iter().find(|d|d.id=="graphic_shape").unwrap().instance();effect.enabled=enabled;
 let(position,scale,rotation,opacity)=match profile{0=>(Vec2::new(32.,24.),100.,0.,100.),1=>(Vec2::new(12.25,18.75),75.,33.,42.5),_=>(Vec2::new(-4.,46.),125.,-17.,100.)};
 for(n,v)in[("shape",ParamValue::Choice(shape)),("size",ParamValue::Vec2(Vec2::new(20.5,12.25))),("position",ParamValue::Vec2(position)),("anchor",ParamValue::Vec2(Vec2::new(1.25,-0.75))),("scale",ParamValue::Float(scale)),("rotation",ParamValue::Float(rotation)),("opacity",ParamValue::Float(opacity)),("sides",ParamValue::Float(5.75)),("corner_radius",ParamValue::Float(3.5)),("points",ParamValue::Curve(vec![[-10.,-5.],[7.,-8.],[12.,3.],[-4.,6.]])),("fill_color",ParamValue::Color([0.2,0.7,0.4,0.65])),("stroke",ParamValue::Bool(appearance>0)),("stroke_width",ParamValue::Float(1.75)),("stroke_color",ParamValue::Color([0.8,0.1,0.4,0.75])),("stroke_type",ParamValue::Choice(appearance)),("stroke2",ParamValue::Bool(appearance>0)),("stroke2_width",ParamValue::Float(3.25)),("stroke2_type",ParamValue::Choice(0)),("background",ParamValue::Bool(appearance==2)),("background_size",ParamValue::Float(2.5)),("background_radius",ParamValue::Float(3.)),("background_opacity",ParamValue::Float(35.)),("shadow",ParamValue::Bool(appearance==2)),("shadow_distance",ParamValue::Float(4.5)),("shadow_size",ParamValue::Float(1.25)),("shadow_blur",ParamValue::Float(5.)),("shadow_opacity",ParamValue::Float(60.))]{assert!(effect.params.contains_key(n),"unknown graphic parameter {n}");effect.params.insert(n.into(),Param::new(v));}
 for definition in filmcraft_project::effect_defs().iter().find(|d|d.id=="graphic_shape").unwrap().params.iter().filter(|p|p.animatable){
  let param=effect.params.get_mut(definition.id).unwrap();let scaled=|factor:f64|match &param.value{
   ParamValue::Float(value)=>{let value=value*factor;let value=if let filmcraft_project::ParamKind::Float{min,max,..}=definition.kind{value.clamp(min,max)}else{value};ParamValue::Float(value)},
   ParamValue::Vec2(value)=>ParamValue::Vec2(Vec2::new(value.x*factor,value.y*factor)),
   ParamValue::Color(value)=>ParamValue::Color(value.map(|v|(v*factor as f32).clamp(0.,1.))),
   other=>panic!("unhandled graphic animation {other:?}"),
  };
  let a=scaled(0.75);let b=scaled(1.25);param.keyframes=vec![filmcraft_project::Keyframe::new(filmcraft_time::Tick::from_seconds_f64(-0.25),a),filmcraft_project::Keyframe::new(filmcraft_time::Tick::from_seconds_f64(1.25),b)];
 }
 let mut out=Image::new(64,48);let mut evaluated=json!(null);
 if let Some(spec)=filmcraft_project::graphic::eval_layer(&effect,filmcraft_time::Tick::from_seconds_f64(seconds),(64,48)){
  let filmcraft_project::graphic::LayerContent::Shape(sh)=&spec.content else{panic!("shape")};let ap=&spec.appearance;
  evaluated=json!({"layer":{"enabled":spec.enabled,"shape":{"shape":sh.shape,"size":[sh.size.0,sh.size.1],"sides":sh.sides,"cornerRadius":sh.corner_radius,"points":sh.points},"appearance":{"fill":ap.fill,"strokes":ap.strokes,"background":ap.background,"shadow":ap.shadow.as_ref().map(|s|json!({"color":s.color,"offset":s.offset,"size":s.size,"blur":s.blur}))},"opacity":spec.transform.opacity},"matrix":xform(&layer_matrix(&spec))});
  if let Some(raster)=raster_layer(&spec,&xform(&layer_matrix(&spec)),64,48){composite_raster(&mut out,&raster,0,0);}
 }
 cases.push(json!({"seconds":seconds,"shape":shape,"profile":profile,"appearance":appearance,"enabled":enabled,"effect":effect,"evaluated":evaluated,"width":64,"height":48,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
