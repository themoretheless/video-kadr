//! Actual pinned Filmcraft Ultra Key renderer oracle.
use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){
 let inputs:[[f32;4];7]=[[0.0;4],[0.25,0.5,0.75,1.0],[1.0;4],[0.125,0.25,0.5,0.25],[-1.0,0.1,2.0,1.0],[0.0,0.0,0.0,0.000001],[0.5,1.0,2.0,0.0]];
 let mut cases=vec![];
 for color in [[0.0,0.8,0.2,1.0],[0.0,0.0,1.0,0.0],[1.0,0.0,0.0,0.25]] { for values in [[45.0,10.0,50.0,50.0,10.0,0.0,0.0,50.0],[0.0;8],[100.0;8],[100.0,0.0,100.0,0.0,100.0,100.0,100.0,0.0],[0.0,100.0,0.0,100.0,0.0,50.0,25.0,100.0]] { for output in [0u32,1,2] {
  let mut effect=filmcraft_project::effect_defs().iter().find(|e|e.id=="ultra_key").unwrap().instance();
  effect.params.insert("key_color".into(),Param::new(ParamValue::Color(color)));
  effect.params.insert("output".into(),Param::new(ParamValue::Choice(output)));
  effect.params.insert("setting".into(),Param::new(ParamValue::Choice(3)));
  let mut parameters=serde_json::Map::new();parameters.insert("keyColor".into(),json!(color));parameters.insert("output".into(),json!(output));parameters.insert("setting".into(),json!(3));
  for (key,value) in ["transparency","highlight","shadow","tolerance","pedestal","choke","soften","spill"].into_iter().zip(values) {effect.params.insert(key.into(),Param::new(ParamValue::Float(value)));parameters.insert(key.into(),json!(value));}
  for input in inputs {
   let mut image=Image::filled(1,1,input);
   let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.0,seconds:0.0,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};
   apply(&mut image,&effect,&cx);
   cases.push(json!({"parameters":parameters,"input":input,"bits":image.px[..4].iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
  }
 }
 }
 }
 let mut spatial=vec![];
 for (w,h) in [(1,7),(7,1),(5,3),(17,9),(2,8),(8,2),(6,4),(18,10)] { for setting in 0..4u32 {for output in 0..3u32 {for config in 0..4 {
  let mut effect=filmcraft_project::effect_defs().iter().find(|e|e.id=="ultra_key").unwrap().instance();
  effect.params.insert("setting".into(),Param::new(ParamValue::Choice(setting)));
  effect.params.insert("output".into(),Param::new(ParamValue::Choice(output)));
  let mut parameters=serde_json::Map::new();parameters.insert("setting".into(),json!(setting));parameters.insert("output".into(),json!(output));
  let extras=match config {
   0=>[0.0,0.0,0.0,50.0,25.0,50.0,50.0,100.0,0.0,100.0],
   1=>[75.0,0.0,35.0,25.0,100.0,100.0,100.0,200.0,120.0,75.0],
   2=>[0.0,80.0,90.0,75.0,0.0,0.0,0.0,0.0,-240.0,200.0],
   _=>[35.0,25.0,20.0,50.0,50.0,25.0,75.0,125.0,720.0,125.0]};
  for ((id,wire),value) in [("choke","choke"),("soften","soften"),("contrast","contrast"),("mid_point","midPoint"),("desaturate","desaturate"),("range","range"),("spill_luma","spillLuma"),("cc_saturation","ccSaturation"),("cc_hue","ccHue"),("cc_luminance","ccLuminance")].into_iter().zip(extras) {
   effect.params.insert(id.into(),Param::new(ParamValue::Float(value)));parameters.insert(wire.into(),json!(value));
  }
  let colors:[[u8;4];7]=[[0,0,0,0],[64,128,192,128],[0,204,51,255],[255,255,255,255],[255,0,0,64],[0,0,255,1],[128,192,64,255]];
  let rgba:Vec<u8>=(0..w*h).flat_map(|i|colors[(i*3+i/w)%colors.len()]).collect();
  let input:Vec<[f32;4]>=rgba.chunks_exact(4).map(|p|{let a=p[3] as f32/255.0;[filmcraft_color::srgb_to_linear(p[0] as f32/255.0)*a,filmcraft_color::srgb_to_linear(p[1] as f32/255.0)*a,filmcraft_color::srgb_to_linear(p[2] as f32/255.0)*a,a]}).collect();
  let mut image=Image{w,h,px:input.iter().flatten().copied().collect()};
  let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.0,seconds:0.0,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};
  apply(&mut image,&effect,&cx);
  spatial.push(json!({"parameters":parameters,"width":w,"height":h,"rgba":rgba,"input":input,"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }}}}
 let mut media=vec![];
 for output in 0..3u32 {for tick in [0u64,250000,500000] {
  let mut effect=filmcraft_project::effect_defs().iter().find(|e|e.id=="ultra_key").unwrap().instance();
  effect.params.insert("output".into(),Param::new(ParamValue::Choice(output)));
  // Sample the authored linear local spill curve independently in the original renderer.
  effect.params.insert("spill".into(),Param::new(ParamValue::Float(tick as f64/500000.0*100.0)));
  let a=128.0f32/255.0;
  let input=[filmcraft_color::srgb_to_linear(64.0/255.0)*a,filmcraft_color::srgb_to_linear(128.0/255.0)*a,filmcraft_color::srgb_to_linear(192.0/255.0)*a,a];
  let mut image=Image::filled(16,16,input);
  let cx=FxCtx{t:filmcraft_time::Tick(tick as i64),px_scale:1.0,seconds:tick as f64/1000000.0,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};
  apply(&mut image,&effect,&cx);
  media.push(json!({"output":output,"localTick":tick,"bits":image.px[..4].iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"spatial":spatial,"media":media}));
}
