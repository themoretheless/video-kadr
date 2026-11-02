//! Actual pinned Filmcraft Channel Mix renderer oracle.
use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){
 let inputs:[[f32;4];6]=[[0.0;4],[0.25,0.5,0.75,1.0],[1.0;4],[0.125,0.25,0.5,0.25],[-1.0,0.1,2.0,1.0],[0.0,0.0,0.0,0.000001]];
 let mut cases=vec![];
 for channels in [[100.0,0.0,0.0,0.0,0.0,100.0,0.0,0.0,0.0,0.0,100.0,0.0],[0.0,0.0,100.0,0.0,0.0,100.0,0.0,0.0,100.0,0.0,0.0,0.0],[0.0;12],[0.0,0.0,0.0,50.0,0.0,0.0,0.0,-50.0,0.0,0.0,0.0,200.0],[200.0,-200.0,50.0,-25.0,80.0,-90.0,40.0,-60.0,100.0,50.0,-75.0,25.0],[33.3,33.3,33.4,0.0,0.0,100.0,0.0,0.0,0.0,0.0,100.0,0.0]] { for monochrome in [false,true] {
  let mut effect=filmcraft_project::effect_defs().iter().find(|e|e.id=="channel_mix").unwrap().instance();
  let mut parameters=serde_json::Map::new();
  for ((source,wire),value) in [("rr","rr"),("rg","rg"),("rb","rb"),("rc","rc"),("gr","gr"),("gg","gg"),("gb","gb"),("gc","gc"),("br","br"),("bg","bg"),("bb","bb"),("bc","bc")].into_iter().zip(channels) {
   effect.params.insert(source.into(),Param::new(ParamValue::Float(value)));
   parameters.insert(wire.into(),json!(value));
  }
  effect.params.insert("monochrome".into(),Param::new(ParamValue::Bool(monochrome)));parameters.insert("monochrome".into(),json!(monochrome));
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
