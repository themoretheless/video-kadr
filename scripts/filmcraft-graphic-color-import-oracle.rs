//! Original graphic color curves, rasterised with every appearance section active.
use filmcraft_project::{Param,ParamValue,Keyframe,Interpolation};
use filmcraft_time::Tick;
use filmcraft_geom::{Vec2,Affine};
use filmcraft_render::{graphic_clip::{layer_matrix,xform,raster_layer,composite_raster},image::Image};
use serde_json::json;
const T:i64=254016000000;
fn main(){let mut cases=vec![];
for id in ["graphic_text","graphic_shape"] {for control in ["fill_color","stroke_color","stroke2_color","background_color","shadow_color"] {for interp in [Interpolation::Linear,Interpolation::Hold,Interpolation::Bezier,Interpolation::AutoBezier,Interpolation::ContinuousBezier,Interpolation::EaseIn,Interpolation::EaseOut] {
 let mut effect=filmcraft_project::effect_defs().iter().find(|d|d.id==id).unwrap().instance();
 for name in ["fill","stroke","stroke2","background","shadow"]{effect.params.insert(name.into(),Param::new(ParamValue::Bool(true)));}
 for(name,value)in [("stroke_width",2.),("stroke2_width",1.),("background_opacity",50.),("background_size",2.),("shadow_opacity",60.),("shadow_distance",2.),("shadow_size",1.),("shadow_blur",1.)]{effect.params.insert(name.into(),Param::new(ParamValue::Float(value)));}
 effect.params.insert("position".into(),Param::new(ParamValue::Vec2(Vec2::new(40.,32.))));
 if id=="graphic_text"{effect.params.insert("text".into(),Param::new(ParamValue::Text("Ab".into())));effect.params.insert("size".into(),Param::new(ParamValue::Float(14.)));}else{effect.params.insert("size".into(),Param::new(ParamValue::Vec2(Vec2::new(24.,16.))));}
 let mut a=Keyframe::new(Tick(T+17),ParamValue::Color([0.1,0.2,0.3,0.4]));a.interp=interp;
 let mut b=Keyframe::new(Tick(2*T+17),ParamValue::Color([0.9,0.8,0.7,0.6]));b.interp=interp;
 effect.params.get_mut(control).unwrap().keyframes=vec![a,b];
 for at in [T+17,T+T/2+17,2*T+17] {
 let spec=filmcraft_project::graphic::eval_layer(&effect,Tick(at),(80,64)).unwrap();let mut image=Image::new(80,64);
 if let Some(r)=raster_layer(&spec,&xform(&Affine::IDENTITY.then_apply(&layer_matrix(&spec))),80,64){composite_raster(&mut image,&r,0,0);}
 cases.push(json!({"effect":effect,"control":control,"at":at.to_string(),"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }
}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
