//! Actual pinned Filmcraft Color Key renderer oracle.
use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){
 let inputs:[[f32;4];7]=[[0.0;4],[0.25,0.5,0.75,1.0],[1.0;4],[0.125,0.25,0.5,0.25],[-1.0,0.1,2.0,1.0],[0.0,0.0,0.0,0.000001],[0.5,1.0,2.0,0.0]];
 let mut cases=vec![];
 for color in [[0.0,0.0,1.0,1.0],[1.0,0.0,0.0,0.0],[0.0,0.8,0.2,0.25]] { for tolerance in [0.0,64.0,128.0,255.0] { for feather in [0.0,25.0,50.0] { for thin in [-5.0,5.0] {
  let mut effect=filmcraft_project::effect_defs().iter().find(|e|e.id=="color_key").unwrap().instance();
  effect.params.insert("color".into(),Param::new(ParamValue::Color(color)));
  for (key,value) in [("tolerance",tolerance),("feather",feather),("thin",thin)] {effect.params.insert(key.into(),Param::new(ParamValue::Float(value)));}
  let parameters=json!({"color":color,"tolerance":tolerance,"feather":feather,"thin":thin});
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
