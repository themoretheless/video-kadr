//! Actual pinned Filmcraft Extract renderer oracle.
use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){
 let inputs:[[f32;4];6]=[[0.0;4],[0.25,0.5,0.75,1.0],[1.0;4],[0.125,0.25,0.5,0.25],[-1.0,0.1,2.0,1.0],[0.0,0.0,0.0,0.000001]];
 let mut cases=vec![];
 for parameters in [[0.0,255.0,0.0],[32.0,192.0,25.0],[200.0,50.0,50.0],[128.0,128.0,100.0],[0.0,255.0,100.0],[128.0,255.0,75.0]] { for invert in [false,true] {
  let mut effect=filmcraft_project::effect_defs().iter().find(|e|e.id=="extract").unwrap().instance();
  for (key,value) in ["black","white","softness"].into_iter().zip(parameters) {effect.params.insert(key.into(),Param::new(ParamValue::Float(value)));}
  effect.params.insert("invert".into(),Param::new(ParamValue::Bool(invert)));
  for input in inputs {
   let mut image=Image::filled(1,1,input);
   let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.0,seconds:0.0,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};
   apply(&mut image,&effect,&cx);
   cases.push(json!({"parameters":{"black":parameters[0],"white":parameters[1],"softness":parameters[2],"invert":invert},"input":input,"bits":image.px[..4].iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
  }
 }
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
