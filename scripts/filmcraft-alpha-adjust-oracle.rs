//! Actual pinned Filmcraft Alpha Adjust renderer oracle.
use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){
 let inputs:[[f32;4];7]=[[0.0;4],[0.25,0.5,0.75,1.0],[1.0;4],[0.125,0.25,0.5,0.25],[-1.0,0.1,2.0,1.0],[0.0,0.0,0.0,0.000001],[0.5,1.0,2.0,0.0]];
 let mut cases=vec![];
 for opacity in [0.0,25.0,50.0,100.0] { for ignore in [false,true] { for invert in [false,true] { for mask_only in [false,true] {
  let mut effect=filmcraft_project::effect_defs().iter().find(|e|e.id=="alpha_adjust").unwrap().instance();
  effect.params.insert("opacity".into(),Param::new(ParamValue::Float(opacity)));
  for (key,value) in [("ignore",ignore),("invert",invert),("mask_only",mask_only)] {effect.params.insert(key.into(),Param::new(ParamValue::Bool(value)));}
  let parameters=json!({"opacity":opacity,"ignore":ignore,"invert":invert,"maskOnly":mask_only});
  for input in inputs {
   let mut image=Image::filled(1,1,input);
   let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.0,seconds:0.0,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};
   apply(&mut image,&effect,&cx);
   cases.push(json!({"parameters":parameters,"input":input,"bits":image.px[..4].iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
  }
 }
 }
 }
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
