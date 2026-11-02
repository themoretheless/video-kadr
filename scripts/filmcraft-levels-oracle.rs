//! Actual pinned Filmcraft Levels renderer oracle.
use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){
 let inputs:[[f32;3];6]=[[0.0;3],[0.25,0.5,0.75],[1.0;3],[4.0;3],[-1.0,0.1,2.0],[1.7,0.0,-1.0]];
 let mut cases=vec![];
 for parameters in [[0.0,255.0,0.0,255.0,100.0],[32.0,192.0,16.0,220.0,200.0],[128.0,64.0,255.0,0.0,100.0],[128.0,128.0,0.0,255.0,10.0],[255.0,0.0,0.0,255.0,1000.0],[0.0,255.0,255.0,0.0,100.0]] {
  let mut effect=filmcraft_project::effect_defs().iter().find(|e|e.id=="levels").unwrap().instance();
  for (key,value) in ["in_black","in_white","out_black","out_white","gamma"].into_iter().zip(parameters) {effect.params.insert(key.into(),Param::new(ParamValue::Float(value)));}
  for input in inputs {
   let mut image=Image::filled(1,1,[input[0],input[1],input[2],1.0]);
   let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.0,seconds:0.0,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};
   apply(&mut image,&effect,&cx);
   cases.push(json!({"parameters":{"inBlack":parameters[0],"inWhite":parameters[1],"outBlack":parameters[2],"outWhite":parameters[3],"gamma":parameters[4]},"input":input,"bits":image.px[..3].iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
  }
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
