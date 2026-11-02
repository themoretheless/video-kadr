use filmcraft_project::{EffectInstance,Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image,vfx::FxEnv};
use filmcraft_geom::{Affine,Vec2};
use serde_json::json;
struct Env;
impl FxEnv for Env{
 fn frame(&self,_:f64,_:&EffectInstance)->Option<Image>{None} fn source_frame(&self,_:f64,_:f32)->Option<Image>{None} fn track(&self,_:usize)->Option<Image>{None}
 fn layer_to_output(&self)->Affine{Affine::IDENTITY} fn clip_seconds(&self)->f64{2.} fn clip_offset(&self)->f64{0.} fn frame_rate(&self)->f64{24.}
 fn media_timecode(&self)->String{"00:00:00:06".into()} fn file_name(&self)->String{"A001.mov".into()} fn sequence_name(&self)->String{"Seq Привет".into()}
 fn sequence_size(&self)->(u32,u32){(64,32)} fn source_size(&self)->(u32,u32){(64,32)} fn clip_key(&self)->u64{2}
}
fn main(){let mut cases=vec![];let env=Env;
for alpha in[0u8,128,255]{for source in 0..6{for alignment in 0..7{for has_env in[false,true]{
let(width,height)=(64usize,32usize);let size=if has_env{37.5}else{6.};let prefix=if has_env{"FILE"}else{""};let position=[17.25,12.75];let color=if has_env{[0.2,0.7,0.4,0.5]}else{[1.,1.,1.,1.]};let opacity=if has_env{37.5}else{60.};let scale=1.;let seconds=0.5;
let rgba:Vec<u8>=(0..width*height).flat_map(|i|[(i*17%256)as u8,(i*31%256)as u8,(i*7%256)as u8,alpha]).collect();let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
let mut image=Image{w:width,h:height,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="metadata_burnin").unwrap().instance();for(n,v)in[("source",ParamValue::Choice(source)),("alignment",ParamValue::Choice(alignment)),("size",ParamValue::Float(size)),("position",ParamValue::Vec2(Vec2::new(position[0],position[1]))),("color",ParamValue::Color(color)),("opacity",ParamValue::Float(opacity)),("prefix",ParamValue::Text(prefix.into()))]{e.params.insert(n.into(),Param::new(v));}
let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:scale,seconds,timecode:"01:00:01:12",clip_name:"Clip Привет",project:None,env:Some(&env),working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);
cases.push(json!({"width":width,"height":height,"rgba":rgba,"parameters":{"source":source,"alignment":alignment,"size":size,"position":position,"color":color,"opacity":opacity,"prefix":prefix},"hasEnv":true,"pixelScale":scale,"seconds":seconds,"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
