//! Original evaluated responsive layers, including hidden targets and signed resized shapes.
use filmcraft_project::{Param,ParamValue,graphic_design::{Pin,PinTarget,LayerExtra}};
use filmcraft_geom::{Vec2,Affine};
use filmcraft_render::{graphic_clip::{apply_pins,layer_bounds,layer_matrix,xform,raster_layer,composite_raster},image::Image};
use serde_json::json;
fn main(){let mut cases=vec![];
for anchored in [false,true] {for text in ["Ab","A longer title","Привет","مرحبا"] {for profile in 0..4 {for target_enabled in [true,false] {for reverse in [false,true] {for prefix in [[1.,0.,0.,1.,0.,0.],[0.8,0.2,-0.15,1.1,12.25,3.75]] {
 let mut effects=vec![];
 for index in 0..3 {let id=if index==1{"graphic_text"}else{"graphic_shape"};let mut effect=filmcraft_project::effect_defs().iter().find(|d|d.id==id).unwrap().instance();effect.enabled=index!=1||target_enabled;
 let mut set=|name:&str,value:ParamValue|{effect.params.insert(name.into(),Param::new(value));};
 if index==1{set("text",ParamValue::Text(text.into()));set("size",ParamValue::Float(14.));}else{set("shape",ParamValue::Choice(if index==0{0}else{1}));set("size",ParamValue::Vec2(Vec2::new(24.,16.)));}
 if anchored {set("anchor",ParamValue::Vec2(Vec2::new([3.25,-2.5,7.125][index],[-4.75,6.25,1.875][index])));}
 set("position",ParamValue::Vec2(Vec2::new([20.,18.,30.][index],[20.,28.,22.][index])));set("rotation",ParamValue::Float([0.,17.,-13.][index]));set("fill_color",ParamValue::Color([[0.9,0.1,0.2,0.75],[0.1,0.8,0.3,0.7],[0.2,0.1,0.9,0.8]][index]));
 let pin=if index==0{Some(Pin{to:PinTarget::Layer(2),left:true,top:true,right:true,bottom:true,offsets:[[-4.,-3.,4.,3.],[25.,0.,-25.,0.],[0.,30.,0.,-30.],[30.,30.,-30.,-30.]][profile]})}else if index==2{Some(Pin{to:if profile==3{PinTarget::Layer(1)}else{PinTarget::Frame},left:false,top:true,right:true,bottom:false,offsets:[0.,2.,-3.,0.]})}else{None};
 effect.layer=Some(Box::new(LayerExtra{uid:index as u64+1,pin,..Default::default()}));effects.push(effect);
 }
 if reverse{effects.reverse()}
 let mut specs:Vec<_>=effects.iter().filter_map(|e|filmcraft_project::graphic::eval_layer(e,filmcraft_time::Tick::ZERO,(64,48))).collect();apply_pins(&mut specs,(64,48));
 let bounds:Vec<_>=specs.iter().map(|s|layer_bounds(s).map(f64::to_bits)).collect();
 let matrices:Vec<_>=specs.iter().map(|s|{let a=layer_matrix(s);[a.a,a.b,a.c,a.d,a.e,a.f].map(f64::to_bits)}).collect();
 let mut image=Image::new(80,64);let transform=Affine{a:prefix[0],b:prefix[1],c:prefix[2],d:prefix[3],e:prefix[4],f:prefix[5]};
 for spec in &specs {if let Some(r)=raster_layer(spec,&xform(&(transform.then_apply(&layer_matrix(spec)))),80,64){composite_raster(&mut image,&r,0,0)}}
 cases.push(json!({"effects":effects,"prefix":prefix,"boundsBits":bounds,"matrixBits":matrices,"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }}}}}}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
