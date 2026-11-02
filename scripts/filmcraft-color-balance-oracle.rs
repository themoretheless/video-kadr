//! Actual pinned Filmcraft Color Balance renderer oracle.
use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){
 let inputs:[[f32;4];6]=[[0.0;4],[0.25,0.5,0.75,1.0],[1.0;4],[0.125,0.25,0.5,0.25],[-1.0,0.1,2.0,1.0],[0.0,0.0,0.0,0.000001]];
 let mut cases=vec![];
 for channels in [[0.0;9],[100.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0],[0.0,0.0,0.0,0.0,-100.0,0.0,0.0,0.0,0.0],[0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,100.0],[100.0,-75.0,50.0,-25.0,80.0,-90.0,40.0,-60.0,100.0]] { for preserve in [false,true] {
  let mut effect=filmcraft_project::effect_defs().iter().find(|e|e.id=="color_balance").unwrap().instance();
  let mut parameters=serde_json::Map::new();
  for ((source,wire),value) in [("shadow_r","shadowR"),("shadow_g","shadowG"),("shadow_b","shadowB"),("mid_r","midR"),("mid_g","midG"),("mid_b","midB"),("hi_r","hiR"),("hi_g","hiG"),("hi_b","hiB")].into_iter().zip(channels) {
   effect.params.insert(source.into(),Param::new(ParamValue::Float(value)));
   parameters.insert(wire.into(),json!(value));
  }
  effect.params.insert("preserve".into(),Param::new(ParamValue::Bool(preserve)));parameters.insert("preserve".into(),json!(preserve));
  for input in inputs {
   let mut image=Image::filled(1,1,input);
   let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.0,seconds:0.0,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};
   apply(&mut image,&effect,&cx);
   cases.push(json!({"parameters":parameters,"input":input,"bits":image.px[..4].iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
  }
 }
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
