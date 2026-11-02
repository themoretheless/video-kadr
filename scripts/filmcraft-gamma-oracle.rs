//! Actual pinned Filmcraft Gamma Correction renderer oracle.
use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){
 let inputs:[[f32;3];6]=[[0.0;3],[0.25,0.5,0.75],[1.0;3],[4.0;3],[-1.0,0.1,2.0],[1.7,0.0,-1.0]];
 let mut cases=vec![];
 for gamma in [10.0,1.0,14.5,28.0] {
  let mut effect=filmcraft_project::effect_defs().iter().find(|e|e.id=="gamma_correction").unwrap().instance();
  effect.params.insert("gamma".into(),Param::new(ParamValue::Float(gamma)));
  for input in inputs {
   let mut image=Image::filled(1,1,[input[0],input[1],input[2],1.0]);
   let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.0,seconds:0.0,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};
   apply(&mut image,&effect,&cx);
   cases.push(json!({"parameters":{"gamma":gamma},"input":input,"bits":image.px[..3].iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
  }
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
