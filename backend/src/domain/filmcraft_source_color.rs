//! Bounded original-compatible RGBA8 input/working/display color stages.
//! YUV/high-bit-depth decode and product source/editor wiring are separate.
use super::{group_surface::GroupSurface,filmcraft_source_pixels,transition_kernel_pack::{context::RenderContext,image::Image}};
use anyhow::{ensure,Result};
use filmcraft_color::{ColorInfo,ColorSpace,ColorPipeline,InputTransform,OutputTransform,Range};
const TRANSFORM_SCRATCH:usize=128*1024;
pub fn decode_rgba8(context:&RenderContext,surface:&GroupSurface,n:usize,color:ColorInfo,override_space:Option<ColorSpace>,pipe:ColorPipeline,peak_nits:Option<f32>)->Result<Image>{
 surface.validate()?;let n=n.max(1);ensure!(n<=64,"source color decimation budget exceeded");ensure!(peak_nits.is_none_or(|p|p.is_finite()&&p>0.&&p<=10000.),"invalid source color peak");
 let cs=override_space.unwrap_or_else(||ColorSpace::from_info(&color));
 let default_decode=cs==ColorSpace::from_info(&color)&&matches!(cs,ColorSpace::Rec709|ColorSpace::Srgb|ColorSpace::Linear709);
 if pipe.is_plain()&&default_decode{return filmcraft_source_pixels::decode_srgb(context,surface,n)}
 let (width,height)=((surface.width/n).max(1),(surface.height/n).max(1));ensure!(context.canvas_allowed(width,height),"source color pixel budget exceeded");
 context.image_work(&[],||{
  let _scratch=context.reserve_scratch(TRANSFORM_SCRATCH);
  let transform=InputTransform::new(cs,Range::Full,&pipe,peak_nits.map(|v|v.round()as u32).map(f64::from));
  let lut:[f32;256]=std::array::from_fn(|i|transform.table.lookup(i as f32/255.));
  let mut image=Image::new(width,height);let inv=1./(n*n)as f32;
  for y in 0..height{if context.is_cancelled(){return Err("source color decode cancelled".into())}for x in 0..width{
   let mut acc=[0f32;4];for dy in 0..n{let sy=(y*n+dy).min(surface.height-1);for dx in 0..n{let sx=(x*n+dx).min(surface.width-1);let p=&surface.rgba[(sy*surface.width+sx)*4..][..4];let a=p[3]as f32/255.;for c in 0..3{acc[c]+=lut[p[c]as usize]*a;}acc[3]+=a;}}
   for c in 0..4{image.px[(y*width+x)*4+c]=acc[c]*inv;}
  }}
  if transform.has_pixel_stage(){map_rgb(context,&mut image,|c|transform.apply(c))?;}
  Ok(image)
 }).map_err(anyhow::Error::msg)
}
fn map_rgb(context:&RenderContext,image:&mut Image,apply:impl Fn([f32;3])->[f32;3])->Result<(),String>{
 for row in image.px.chunks_mut(image.w*4){if context.is_cancelled(){return Err("source color transform cancelled".into())}for p in row.chunks_exact_mut(4){let a=p[3];if a<=1e-6{continue}let c=apply([p[0]/a,p[1]/a,p[2]/a]);for i in 0..3{p[i]=c[i]*a;}}}Ok(())
}
pub fn to_display(context:&RenderContext,image:&Image,pipe:ColorPipeline)->Result<Image>{
 context.image_work(&[image],||{
  let mut output=image.clone();if pipe.is_plain(){return Ok(output)}
  let _scratch=context.reserve_scratch(TRANSFORM_SCRATCH);let transform=OutputTransform::display(&pipe);
  if !transform.is_identity(){map_rgb(context,&mut output,|c|transform.apply(c))?;}
  Ok(output)
 }).map_err(anyhow::Error::msg)
}
#[cfg(test)]mod tests{
 use super::*;use serde_json::Value;use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
 #[test]fn original_managed_input_and_display_match_every_bit_and_byte(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-source-color.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1584);
  let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,64,4*1024*1024).unwrap();
  for(i,case)in cases.iter().enumerate(){let surface=GroupSurface{width:case["width"].as_u64().unwrap()as usize,height:case["height"].as_u64().unwrap()as usize,rgba:serde_json::from_value(case["rgba"].clone()).unwrap()};let pipe:ColorPipeline=serde_json::from_value(case["pipeline"].clone()).unwrap();let image=decode_rgba8(&context,&surface,case["n"].as_u64().unwrap()as usize,serde_json::from_value(case["color"].clone()).unwrap(),serde_json::from_value(case["override"].clone()).unwrap(),pipe,case["peak"].as_f64().map(|v|v as f32)).unwrap_or_else(|e|panic!("color case {i}: {e}"));assert_eq!((image.w,image.h),(case["outputWidth"].as_u64().unwrap()as usize,case["outputHeight"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"source color {i}");let display=to_display(&context,&image,pipe).unwrap();assert_eq!(serde_json::json!(display.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["displayBits"],"display color {i}");let rgba=filmcraft_source_pixels::encode_srgb(&context,&display).unwrap();assert_eq!(serde_json::json!(rgba.surface().rgba),case["displayRgba"],"display bytes {i}");drop(rgba);drop(display);drop(image);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
 #[test]fn color_budget_invalid_input_and_cancellation_release_storage(){
  let flag=Arc::new(AtomicBool::new(false));let context=RenderContext::new_with_budget(flag.clone(),1,4,TRANSFORM_SCRATCH-1).unwrap();let surface=GroupSurface{width:1,height:1,rgba:vec![128,128,128,255]};let decode=||decode_rgba8(&context,&surface,1,ColorInfo::SRGB_FULL,Some(ColorSpace::Rec2100Pq),ColorPipeline::REC709,None);assert!(decode().err().unwrap().to_string().contains("budget"));assert_eq!(context.pixel_bytes_in_use(),0);
  for peak in [f32::NAN,-1.,0.,10001.]{assert!(decode_rgba8(&context,&surface,1,ColorInfo::SRGB_FULL,None,ColorPipeline::REC709,Some(peak)).is_err());}
  assert!(decode_rgba8(&context,&surface,65,ColorInfo::SRGB_FULL,None,ColorPipeline::REC709,None).is_err());let malformed=GroupSurface{width:1,height:1,rgba:vec![]};assert!(decode_rgba8(&context,&malformed,1,ColorInfo::SRGB_FULL,None,ColorPipeline::REC709,None).is_err());
  flag.store(true,Ordering::Relaxed);assert!(decode().is_err());assert_eq!(context.pixel_bytes_in_use(),0);flag.store(false,Ordering::Relaxed);
  let failed=context.image_work(&[],||{let mut image=Image::new(2,2);for p in image.px.chunks_exact_mut(4){p[3]=1.;}map_rgb(&context,&mut image,|c|{flag.store(true,Ordering::Relaxed);c})?;Ok(image)});assert!(failed.is_err());assert_eq!(context.pixel_bytes_in_use(),0);flag.store(false,Ordering::Relaxed);
  let mut invalid=Image::new(1,1);invalid.px[0]=f32::INFINITY;assert!(to_display(&context,&invalid,ColorPipeline::REC709).is_err());assert_eq!(context.pixel_bytes_in_use(),0);
 }
}
