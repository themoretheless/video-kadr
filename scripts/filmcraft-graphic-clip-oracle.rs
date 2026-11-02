//! Original multi-layer transparent graphic canvas, draw order, visibility and affine fixtures.
use filmcraft_project::{Param,ParamValue};
use filmcraft_geom::{Vec2,Affine};
use filmcraft_render::{graphic_clip::{layer_matrix,xform,raster_layer,composite_raster},image::Image};
use serde_json::json;
fn main(){let mut cases=vec![];
for prefix in [[1.,0.,0.,1.,0.,0.],[0.8,0.2,-0.15,1.1,12.25,3.75],[-1.,0.,0.,1.,70.,0.],[1.25,0.,0.,0.75,-10.,8.]] {for reverse in [false,true] {for enabled in 0..8 {
 let mut effects=vec![];
 for index in 0..3 {let id=if index==1{"graphic_text"}else{"graphic_shape"};let mut effect=filmcraft_project::effect_defs().iter().find(|d|d.id==id).unwrap().instance();effect.enabled=enabled&(1<<index)!=0;
 let mut set=|name:&str,value:ParamValue|{assert!(effect.params.contains_key(name));effect.params.insert(name.into(),Param::new(value));};
 if index==1{set("text",ParamValue::Text("Ab".into()));set("size",ParamValue::Float(14.));}else{set("shape",ParamValue::Choice(if index==0{0}else{1}));set("size",ParamValue::Vec2(Vec2::new(if index==0{24.}else{18.},if index==0{16.}else{20.})));}
 set("position",ParamValue::Vec2(Vec2::new([20.,18.,30.][index],[20.,28.,22.][index])));set("rotation",ParamValue::Float([0.,17.,-13.][index]));set("fill_color",ParamValue::Color([[0.9,0.1,0.2,0.75],[0.1,0.8,0.3,0.7],[0.2,0.1,0.9,0.8]][index]));effects.push(effect);
 }
 if reverse{effects.reverse();}
 let mut image=Image::new(80,64);let transform=Affine{a:prefix[0],b:prefix[1],c:prefix[2],d:prefix[3],e:prefix[4],f:prefix[5]};
 for effect in &effects {let spec=filmcraft_project::graphic::eval_layer(effect,filmcraft_time::Tick::ZERO,(64,48)).unwrap();if let Some(r)=raster_layer(&spec,&xform(&(transform.then_apply(&layer_matrix(&spec)))),80,64){composite_raster(&mut image,&r,0,0);}}
 cases.push(json!({"prefix":prefix,"effects":effects,"width":80,"height":64,"canvas":[64,48],"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }}}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
