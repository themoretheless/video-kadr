use filmcraft_project::{Param, ParamValue};
use filmcraft_render::{effects::{apply, FxCtx}, image::Image};
use filmcraft_geom::Vec2;
use serde_json::json;
fn main() {
 let mut cases=vec![];
 for (width,height) in [(64usize,32usize),(128,64)] { for alpha in [0u8,128,255] { for kind in ["timecode","clip_name"] { for profile in 0..4 { for scale in [0.5f32,1.,2.] {
  let (size,opacity,position,timecode,clip_name)=match profile {
   0=>(15.,100.,None,"01:00:01:12","Clip"),
   1=>(37.5,42.125,Some([17.25,12.75]),"00:00:00:06","Привет עולם"),
   2=>(100.,0.,Some([-3.5,8.25]),"-00:00:00:01","Line 1\nLine 2"),
   _=>(1.,60.,Some([width as f64+2.25,height as f64-1.75]),"",""),
  };
  let rgba:Vec<u8>=(0..width*height).flat_map(|i|[(i*17%256)as u8,(i*31%256)as u8,(i*7%256)as u8,alpha]).collect();
  let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
  let mut image=Image{w:width,h:height,px};
  let mut effect=filmcraft_project::effect_defs().iter().find(|e|e.id==kind).unwrap().instance();
  effect.params.insert("size".into(),Param::new(ParamValue::Float(size)));
  if kind=="timecode" {effect.params.insert("opacity".into(),Param::new(ParamValue::Float(opacity)));}
  if let Some(p)=position {effect.params.insert("position".into(),Param::new(ParamValue::Vec2(Vec2::new(p[0],p[1]))));}
  let context=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:scale,seconds:0.5,timecode,clip_name,project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};
  apply(&mut image,&effect,&context);
  cases.push(json!({"width":width,"height":height,"rgba":rgba,"kind":kind,"size":size,"opacity":opacity,"position":position.unwrap_or([width as f64/2./scale as f64,height as f64/2./scale as f64]),"autoPosition":position.is_none(),"pixelScale":scale,"timecode":timecode,"clipName":clip_name,"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }}}}}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
