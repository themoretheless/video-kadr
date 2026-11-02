//! Assemble fully prepared source layers; decoding, effects and Motion remain
//! provider responsibilities. This is not a full source-project renderer yet.
use super::{filmcraft_blend::{self,Blend},filmcraft_video_plan::{VideoLayer,PlannedClip},transition_kernel_pack::{context::RenderContext,image::Image,project::EffectInstance}};
use anyhow::{ensure,Result};use serde_json::Value;
pub struct PreparedClip{pub image:Image,pub opacity:f32,pub blend:Blend}
pub enum PreparedLayer{Clip(PreparedClip),Transition{effect:EffectInstance,from:Option<PreparedClip>,to:Option<PreparedClip>,progress:f32,reverse:bool}}
/// Preserve plan order and pass each source's exact independent clocks to the
/// provider. Transition parameters must be resolved by the source adapter.
pub fn prepare(plan:&[VideoLayer],mut clip:impl FnMut(&PlannedClip)->Result<PreparedClip>,mut effect:impl FnMut(&Value)->Result<EffectInstance>)->Result<Vec<PreparedLayer>>{
 ensure!(plan.len()<=256,"video layer budget exceeded");plan.iter().map(|layer|match layer{
  VideoLayer::Clip{clip:source,..}=>Ok(PreparedLayer::Clip(clip(source)?)),
  VideoLayer::Transition{from,to,progress,reverse,effect:raw,..}=>Ok(PreparedLayer::Transition{effect:effect(raw)?,from:from.as_ref().map(&mut clip).transpose()?,to:to.as_ref().map(&mut clip).transpose()?,progress:*progress as f32,reverse:*reverse}),
 }).collect()
}
pub fn compose(context:&RenderContext,layers:&[PreparedLayer],width:usize,height:usize,scale:f32)->Result<Image>{
 ensure!(context.canvas_allowed(width,height),"frame canvas pixel budget exceeded");
 ensure!(width>0&&height>0&&width<=3840&&height<=2160&&layers.len()<=256,"invalid frame canvas/layer budget");ensure!(scale.is_finite()&&scale>0.&&scale<=64.,"invalid frame scale");
 let mut inputs=Vec::new();
 fn admit<'a>(clip:&'a PreparedClip,inputs:&mut Vec<&'a Image>,width:usize,height:usize)->Result<()>{ensure!((clip.image.w,clip.image.h)==(width,height),"prepared layer canvas mismatch");ensure!(clip.opacity.is_finite()&&(0.0..=1.0).contains(&clip.opacity),"invalid layer opacity");inputs.push(&clip.image);Ok(())}
 for layer in layers{match layer{PreparedLayer::Clip(clip)=>admit(clip,&mut inputs,width,height)?,PreparedLayer::Transition{from,to,progress,effect,..}=>{ensure!(progress.is_finite()&&(0.0..=1.0).contains(progress),"invalid transition progress");effect.validate().map_err(anyhow::Error::msg)?;for clip in [from,to].into_iter().flatten(){admit(clip,&mut inputs,width,height)?}}}}
 context.image_work(&inputs,||{
  let mut canvas=Image::new(width,height);
  for layer in layers{apply_layer(context,&mut canvas,layer,scale).map_err(|e|e.to_string())?;}
  Ok(canvas)
 }).map_err(anyhow::Error::msg)
}
/// Run inside context.image_work: mutable canvas is unpublished working storage.
pub(crate) fn apply_layer(context:&RenderContext,canvas:&mut Image,layer:&PreparedLayer,scale:f32)->Result<()>{
 ensure!(!context.is_cancelled(),"frame cancelled");let(width,height)=(canvas.w,canvas.h);
 let side=|clip:&Option<PreparedClip>|->Result<Image>{
  if let Some(clip)=clip{ensure!((clip.image.w,clip.image.h)==(width,height)&&clip.opacity.is_finite()&&(0.0..=1.0).contains(&clip.opacity),"invalid prepared transition side");}
  let mut image=clip.as_ref().map(|c|c.image.clone()).unwrap_or_else(||Image::new(width,height));
  if let Some(clip)=clip{for chunk in image.px.chunks_mut(4096){ensure!(!context.is_cancelled(),"frame cancelled");for value in chunk{*value*=clip.opacity}}}Ok(image)
 };
 match layer{
  PreparedLayer::Clip(clip)=>{ensure!((clip.image.w,clip.image.h)==(width,height),"prepared layer canvas mismatch");filmcraft_blend::composite_checked(canvas,&clip.image,0,0,clip.opacity,clip.blend,||context.is_cancelled())?;}
  PreparedLayer::Transition{effect,from,to,progress,reverse}=>{ensure!(progress.is_finite()&&(0.0..=1.0).contains(progress),"invalid transition progress");let a=side(from)?;let b=side(to)?;let mixed=if *reverse{context.render(effect,&b,&a,1.-progress,scale)}else{context.render(effect,&a,&b,*progress,scale)}.map_err(anyhow::Error::msg)?;filmcraft_blend::composite_checked(canvas,&mixed,0,0,1.,Blend::Normal,||context.is_cancelled())?;}
 }Ok(())
}
#[cfg(test)]mod tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};
 #[test]fn empty_canvas_cancel_and_budget_are_qualified(){
  let flag=Arc::new(AtomicBool::new(false));let context=RenderContext::new_with_budget(flag.clone(),1,32,4096).unwrap();let output=compose(&context,&[],8,4,1.).unwrap();assert!(output.px.iter().all(|v|*v==0.));assert_eq!(context.pixel_bytes_in_use(),512);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);assert!(compose(&context,&[],9,4,1.).is_err());flag.store(true,std::sync::atomic::Ordering::Relaxed);assert!(compose(&context,&[],8,4,1.).is_err());
  let small=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),1,32,64).unwrap();assert!(compose(&small,&[],8,4,1.).is_err());assert_eq!(small.pixel_bytes_in_use(),0);
 }
 #[test]fn original_layer_assembly_matches_all_transitions_and_blends(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-video-compositor.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),753);
  let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,32,16*1024*1024).unwrap();
  let image=|key:&str|Image{w:8,h:4,px:fixture[key].as_array().unwrap().iter().map(|v|f32::from_bits(v.as_u64().unwrap()as u32)).collect()};
  for case in cases{let mut layers=vec![PreparedLayer::Clip(PreparedClip{image:image("base"),opacity:1.,blend:Blend::Normal})];if case["kind"]=="clip"{layers.push(PreparedLayer::Clip(PreparedClip{image:image("a"),opacity:case["opacity"].as_f64().unwrap()as f32,blend:Blend::from_index(case["mode"].as_u64().unwrap()as u32)}));}else{layers.push(PreparedLayer::Transition{effect:serde_json::from_value(case["effect"].clone()).unwrap(),from:Some(PreparedClip{image:image("a"),opacity:0.37,blend:Blend::Multiply}),to:Some(PreparedClip{image:image("b"),opacity:0.73,blend:Blend::Screen}),progress:case["progress"].as_f64().unwrap()as f32,reverse:case["reverse"].as_bool().unwrap()});}let output=compose(&context,&layers,8,4,1.).unwrap();let bits:Vec<_>=output.px.iter().map(|v|v.to_bits()).collect();assert_eq!(serde_json::to_value(bits).unwrap(),case["bits"],"{case}");drop(output);assert_eq!(context.pixel_bytes_in_use(),0);}
 }
}
