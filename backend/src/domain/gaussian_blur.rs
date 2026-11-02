//! Filmcraft Gaussian Blur on premultiplied linear pixels.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,Default,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct GaussianBlur {pub blurriness:f32,pub dimensions:u8,pub repeat_edge:bool}
impl GaussianBlur {
 pub fn validate(&self)->Result<()> {
  ensure!(self.blurriness.is_finite()&&(0.0..=3000.0).contains(&self.blurriness)&&self.dimensions<=2,"Gaussian Blur parameter out of range");Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;self.apply_sigma(frame,self.blurriness*0.5,checkpoint)
 }
 pub(crate) fn apply_sigma(&self,frame:&mut super::linear_color_frame::LinearColorFrame,sigma:f32,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  ensure!(sigma.is_finite()&&sigma>=0.0,"invalid Gaussian sigma");let (w,h)=frame.dimensions();let mut channels=Vec::with_capacity(4);
  for c in 0..4 {
   checkpoint()?;let plane:Vec<f32>=frame.pixels().iter().map(|p|p[c]).collect();
   channels.push(blur_plane(&plane,w,h,sigma,self.dimensions,self.repeat_edge,checkpoint)?);
  }
  let mut i=0;frame.map_premultiplied(&mut|_|{let p=std::array::from_fn(|c|channels[c][i]);i+=1;p},checkpoint)
 }
}
pub(crate) fn blur_plane(plane: &[f32], w: usize, h: usize, sigma: f32, dimensions:u8, repeat:bool,
 checkpoint: &mut dyn FnMut()->Result<()>) -> Result<Vec<f32>> {
 ensure!(w > 0 && h > 0 && w.checked_mul(h) == Some(plane.len()), "invalid Gaussian Blur dimensions");
 let mut out=plane.to_vec();
 if sigma<=0.3 {return Ok(out);}
 fn radii(s: f32, dimension: usize)->Vec<usize> {
  let s=if s.is_finite(){s.min(dimension.max(8) as f32*2.0)}else{0.0};
  if s<=0.3 {return vec![];}
  let mut wl=(12.0*s*s/3.0+1.0).sqrt().floor() as i32;
  if wl%2==0 {wl-=1;}
  let wu=wl+2;
  let m=((12.0*s*s-(3*wl*wl) as f32-4.0*3.0*wl as f32-3.0*3.0)/(-4.0*wl as f32-4.0)).round() as i32;
  (0..3).map(|i|(((if i<m {wl}else{wu})-1)/2).max(0) as usize).collect()
 }
 fn rows(p: &mut [f32], w: usize, r: usize, repeat:bool, checkpoint: &mut dyn FnMut()->Result<()>)->Result<()> {
  if r==0 {return Ok(());}
  for row in p.chunks_mut(w) {
   checkpoint()?;
   let src=row.to_vec();let inv=1.0/(2*r+1) as f32;let mut acc=0.0;
   for i in -(r as isize)..=r as isize {if repeat||(i>=0&&(i as usize)<w) {acc+=src[if repeat {i.clamp(0,w as isize-1) as usize}else{i as usize}];}}
   for (x,value) in row.iter_mut().enumerate() {
    *value=acc*inv;
    let removed=x as isize-r as isize;
    let added=x+r+1;
    if repeat||removed>=0 {acc-=src[if repeat {removed.clamp(0,w as isize-1) as usize}else{removed as usize}];}
    if repeat||added<w {acc+=src[added.min(w-1)];}
   }
  }
  Ok(())
 }
 if dimensions!=2 {for r in radii(sigma,w) {rows(&mut out,w,r,repeat,checkpoint)?;}}
 let ry=if dimensions==1 {vec![]}else{radii(sigma,h)};
 if !ry.is_empty() {
  let mut transposed=vec![0.0;out.len()];
  for y in 0..h {checkpoint()?;for x in 0..w {transposed[x*h+y]=out[y*w+x];}}
  for r in ry {rows(&mut transposed,h,r,repeat,checkpoint)?;}
  for y in 0..h {checkpoint()?;for x in 0..w {out[y*w+x]=transposed[x*h+y];}}
 }
 Ok(out)
}
#[cfg(test)]mod tests {
 use super::*;
 #[test]fn original_working_frame_rgba_bits() {
  #[derive(Deserialize)]struct Case {parameters:GaussianBlur,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
  #[derive(Deserialize)]struct Oracle {commit:String,cases:Vec<Case>}
  let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-gaussian-blur.json")).unwrap();
  assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),120);
  for (index,case) in oracle.cases.iter().enumerate() {
   let source=super::super::group_surface::GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()};
   let mut frame=super::super::linear_color_frame::LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
   case.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();
   let bits:Vec<u32>=frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
   assert_eq!(bits,case.bits,"original Gaussian Blur frame {index}");
  }
 }
 #[test]fn persisted_blurriness_curve_uses_local_time_and_retains_choices() {
  let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"gaussian_blur","parameters":{"dimensions":2,"repeatEdge":true},"animation":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":100}]}}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let super::super::composition::VideoEffect::GaussianBlur{parameters,animation:Some(curve)}=effect else {panic!("Gaussian Blur")};
  assert_eq!(sample_animation(parameters,&curve,250000).unwrap(),GaussianBlur{blurriness:50.0,dimensions:2,repeat_edge:true});
 }
 #[test]fn axis_edge_alpha_and_cancellation() {
  let src=[1.0;9];
  let repeat=blur_plane(&src,3,3,1.0,0,true,&mut||Ok(())).unwrap();assert_eq!(repeat,src);
  let horizontal=blur_plane(&src,3,3,1.0,1,false,&mut||Ok(())).unwrap();assert_eq!(horizontal[1],1.0);assert_eq!(horizontal[0],2.0/3.0);
  let vertical=blur_plane(&src,3,3,1.0,2,false,&mut||Ok(())).unwrap();assert_eq!(vertical[3],1.0);assert_eq!(vertical[0],2.0/3.0);
  assert!(blur_plane(&src,3,3,1.0,0,false,&mut||anyhow::bail!("cancelled")).is_err());
  assert!(GaussianBlur{blurriness:3001.0,..Default::default()}.validate().is_err());
 }
}

pub fn validate_animation(animation:&super::composition::AnimatableValue)->Result<()> {
 animation.validate()?;ensure!(super::composition::animatable_values_in(animation,0.0,3000.0,true),"Gaussian Blur animation out of range");Ok(())
}
pub fn sample_animation(mut parameters:GaussianBlur,animation:&super::composition::AnimatableValue,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<GaussianBlur> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;parameters.blurriness=super::nested_composition::sample_value(animation,time)? as f32;parameters.validate()?;Ok(parameters)
}
