//! Bounded sRGB RGBA8 → linear-premultiplied source pixels and Motion placement.
//! Other colour spaces/HDR/YUV, effects, masks and output encoding are separate stages.
use super::{group_surface::GroupSurface,transition_kernel_pack::{context::RenderContext,image::Image}};
use anyhow::{ensure,Result};use std::sync::OnceLock;
pub fn decode_srgb(context:&RenderContext,surface:&GroupSurface,decimation:usize)->Result<Image>{
 surface.validate()?;let n=decimation.max(1);ensure!(n<=64,"source decimation budget exceeded");let width=(surface.width/n).max(1);let height=(surface.height/n).max(1);ensure!(context.canvas_allowed(width,height),"decoded source pixel budget exceeded");
 static LUT:OnceLock<[f32;256]>=OnceLock::new();let lut=LUT.get_or_init(||std::array::from_fn(|i|{let v=i as f32/255.;if v<=0.04045{v/12.92}else{((v+0.055)/1.055).powf(2.4)}}));
 context.image_work(&[],||{let mut image=Image::new(width,height);let inv=1./(n*n)as f32;
  for y in 0..height{if context.is_cancelled(){return Err("source decode cancelled".into())}for x in 0..width{
   let mut acc=[0f32;4];for dy in 0..n{let sy=(y*n+dy).min(surface.height-1);for dx in 0..n{let sx=(x*n+dx).min(surface.width-1);let p=&surface.rgba[(sy*surface.width+sx)*4..][..4];let alpha=p[3]as f32/255.;acc[0]+=lut[p[0]as usize]*alpha;acc[1]+=lut[p[1]as usize]*alpha;acc[2]+=lut[p[2]as usize]*alpha;acc[3]+=alpha;}}
   for c in 0..4{image.px[(y*width+x)*4+c]=acc[c]*inv;}
  }}Ok(image)
 }).map_err(anyhow::Error::msg)
}
pub fn place(context:&RenderContext,image:&Image,canvas:(usize,usize),source_width:u32,motion:[f64;6],scale:f32)->Result<Image>{
 ensure!(context.canvas_allowed(canvas.0,canvas.1)&&canvas.0<=3840&&canvas.1<=2160,"invalid placement canvas budget");ensure!(source_width>0&&scale.is_finite()&&scale>0.&&scale<=64.&&motion.iter().all(|v|v.is_finite()),"invalid source placement");
 context.image_work(&[image],||image.placed_source(canvas.0,canvas.1,motion,scale,source_width)).map_err(anyhow::Error::msg)
}
#[cfg(test)]mod tests{
 use super::*;use serde_json::Value;use std::sync::{Arc,atomic::AtomicBool};
 #[test]fn source_pixel_admission_and_cancellation_are_explicit(){
  let flag=Arc::new(AtomicBool::new(false));let context=RenderContext::new_with_budget(flag.clone(),1,1,1024).unwrap();let surface=GroupSurface{width:1,height:1,rgba:vec![255;4]};assert!(decode_srgb(&context,&surface,65).is_err());let malformed=GroupSurface{width:1,height:1,rgba:vec![]};assert!(decode_srgb(&context,&malformed,1).is_err());let image=decode_srgb(&context,&surface,1).unwrap();assert!(place(&context,&image,(1,1),0,[1.,0.,0.,1.,0.,0.],1.).is_err());flag.store(true,std::sync::atomic::Ordering::Relaxed);assert!(decode_srgb(&context,&surface,1).is_err());assert!(place(&context,&image,(1,1),1,[1.,0.,0.,1.,0.,0.],1.).is_err());drop(image);assert_eq!(context.pixel_bytes_in_use(),0);
 }
 #[test]fn original_rgba_conversion_decimation_and_placement_match_every_bit(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-source-pixels.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),75);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,4096,1024*1024).unwrap();
  for case in cases{let surface=GroupSurface{width:case["w"].as_u64().unwrap()as usize,height:case["h"].as_u64().unwrap()as usize,rgba:serde_json::from_value(case["rgba"].clone()).unwrap()};let image=decode_srgb(&context,&surface,case["n"].as_u64().unwrap()as usize).unwrap();assert_eq!(serde_json::json!(image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"source decode {case}");
   for placement in case["placements"].as_array().unwrap(){let output=place(&context,&image,(placement["w"].as_u64().unwrap()as usize,placement["h"].as_u64().unwrap()as usize),surface.width as u32,serde_json::from_value(placement["matrix"].clone()).unwrap(),placement["scale"].as_f64().unwrap()as f32).unwrap();assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),placement["bits"],"placement n={} scale={} matrix={}",case["n"],placement["scale"],placement["matrix"]);}
   drop(image);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}

/// Straight sRGB RGBA8 output; storage stays admitted until this value drops.
pub struct DisplayFrame {
 surface:GroupSurface,
 _reservation:super::transition_kernel_pack::budget::Reservation,
}
impl DisplayFrame { pub fn surface(&self)->&GroupSurface{&self.surface} }
pub fn encode_srgb(context:&RenderContext,image:&Image)->Result<DisplayFrame>{
 static LUT:OnceLock<[u8;4096]>=OnceLock::new();
 let lut=LUT.get_or_init(||std::array::from_fn(|i|{
  (super::basic_grade::display(i as f32/4095.)*255.+0.5).clamp(0.,255.)as u8
 }));
 let (rgba,reservation)=context.rgba8_work(image,|s|{
  let a=s[3].clamp(0.,1.);if a<=0.{return [0;4]}
  let inv=1./a;
  let encode=|v:f32|lut[((v*inv).clamp(0.,1.)*4095.+0.5)as usize];
  [encode(s[0]),encode(s[1]),encode(s[2]),(a*255.+0.5)as u8]
 }).map_err(anyhow::Error::msg)?;
 Ok(DisplayFrame{surface:GroupSurface{width:image.w,height:image.h,rgba},_reservation:reservation})
}

/// Opaque sRGB over black, matching Filmcraft Image::over_black_rgba8.
/// The image is already premultiplied in linear light, so RGB is encoded directly.
pub fn encode_srgb_over_black(context:&RenderContext,image:&Image)->Result<DisplayFrame>{
 static LUT:OnceLock<[u8;4096]>=OnceLock::new();
 let lut=LUT.get_or_init(||std::array::from_fn(|i|(super::basic_grade::display(i as f32/4095.)*255.+0.5).clamp(0.,255.)as u8));
 let (rgba,reservation)=context.rgba8_work(image,|s|{
  let encode=|v:f32|lut[(v.clamp(0.,1.)*4095.+0.5)as usize];
  [encode(s[0]),encode(s[1]),encode(s[2]),255]
 }).map_err(anyhow::Error::msg)?;
 Ok(DisplayFrame{surface:GroupSurface{width:image.w,height:image.h,rgba},_reservation:reservation})
}

#[cfg(test)]mod display_tests{
 use super::*;use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
 #[test]fn original_display_bytes_match_for_full_lut_alpha_and_boundaries(){
  let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-display.json")).unwrap();
  assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),9);
  let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),1,5000,1024*1024).unwrap();
  for case in cases{
   let mut image=Image::new(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize);
   for (p,b) in image.px.iter_mut().zip(case["bits"].as_array().unwrap()){*p=f32::from_bits(b.as_u64().unwrap()as u32);}
   let output=encode_srgb(&context,&image).unwrap();let expected:Vec<u8>=serde_json::from_value(case["rgba"].clone()).unwrap();assert_eq!(output.surface().rgba,expected);
   assert_eq!(context.pixel_bytes_in_use(),output.surface().rgba.capacity());drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
 #[test]fn display_admission_refusal_nonfinite_and_cancellation_release_storage(){
  let flag=Arc::new(AtomicBool::new(false));let image=Image::new(2,2);
  let tiny=RenderContext::new_with_budget(flag.clone(),1,4,79).unwrap();assert!(encode_srgb(&tiny,&image).err().unwrap().to_string().contains("budget"));assert_eq!(tiny.pixel_bytes_in_use(),0);
  let context=RenderContext::new_with_budget(flag.clone(),1,4,1024).unwrap();let mut invalid=Image::new(2,2);invalid.px[0]=f32::NAN;assert!(encode_srgb(&context,&invalid).is_err());assert_eq!(context.pixel_bytes_in_use(),0);
  flag.store(true,Ordering::Relaxed);assert!(encode_srgb(&context,&image).is_err());assert_eq!(context.pixel_bytes_in_use(),0);
  flag.store(false,Ordering::Relaxed);let result=context.rgba8_work(&image,|_|{flag.store(true,Ordering::Relaxed);[0;4]});assert!(result.is_err());assert_eq!(context.pixel_bytes_in_use(),0);
 }
}
