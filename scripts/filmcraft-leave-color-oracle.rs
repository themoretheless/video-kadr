//! Actual pinned Filmcraft Leave Color renderer oracle.
use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){
 let inputs:[[f32;4];6]=[[0.0;4],[0.25,0.5,0.75,1.0],[1.0;4],[0.125,0.25,0.5,0.25],[-1.0,0.1,2.0,1.0],[0.0,0.0,0.0,0.000001]];
 let mut cases=vec![];
 for color in [[1.0,0.0,0.0,1.0],[0.0,0.0,1.0,0.0],[0.0,1.0,0.0,0.25]] { for (amount,tolerance,softness) in [(0.0,15.0,0.0),(100.0,15.0,0.0),(50.0,0.0,50.0),(100.0,100.0,100.0)] {
  let mut effect=filmcraft_project::effect_defs().iter().find(|e|e.id=="leave_color").unwrap().instance();
  effect.params.insert("color".into(),Param::new(ParamValue::Color(color)));
  for (key,value) in [("amount",amount),("tolerance",tolerance),("softness",softness)] {effect.params.insert(key.into(),Param::new(ParamValue::Float(value)));}
  let parameters=json!({"color":color,"amount":amount,"tolerance":tolerance,"softness":softness});
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
