//! Actual pinned Filmcraft renderer ProcAmp linear-output oracle.
use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
use std::collections::BTreeMap;
fn main(){
 let fields=[("brightness","brightness",0.0,-100.0,100.0),("contrast","contrast",100.0,0.0,200.0),("hue","hue",0.0,-720.0,720.0),("saturation","saturation",100.0,0.0,200.0)];
 let inputs:[[f32;3];6]=[[0.0;3],[0.25,0.5,0.75],[1.0;3],[4.0;3],[-1.0,0.1,2.0],[1.7,0.0,-1.0]];
 let mut cases=vec![];
 for scenario in 0..16 {
  let mut parameters=BTreeMap::new();
  let mut effect=filmcraft_project::effect_defs().iter().find(|e|e.id=="proc_amp").unwrap().instance();
  for (index,(host,source,default,min,max)) in fields.iter().enumerate(){
   let pick=if scenario==0 {None} else if scenario<=3 {Some(scenario-1)} else if (scenario-4)/3==index {Some((scenario-4)%3)} else {None};
   let value=match pick {Some(0)=>*min,Some(1)=>(*min+*max)/2.0,Some(_)=>*max,None=>*default};
   parameters.insert(*host,value);
   effect.params.insert((*source).into(),Param::new(ParamValue::Float(value)));
  }
  for input in inputs {
   let mut image=Image::filled(1,1,[input[0],input[1],input[2],1.0]);
   let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.0,seconds:0.0,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};
   apply(&mut image,&effect,&cx);
   let params=json!(parameters);
   cases.push(json!({"parameters":params,"input":input,"bits":image.px[..3].iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
  }
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
