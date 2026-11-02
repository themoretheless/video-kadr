//! Filmcraft ColorEmboss CPU reference, source-resolution pixel scale.
//! MIT source attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct ColorEmboss {pub direction:f32,pub relief:f32,pub contrast:f32,pub blend:f32}
impl Default for ColorEmboss {fn default()->Self {Self{direction:45.0,relief:1.0,contrast:100.0,blend:0.0}}}
impl ColorEmboss {
 pub fn validate(&self)->Result<()> {
  ensure!(self.direction.is_finite(),"ColorEmboss direction out of range");
  ensure!(self.relief.is_finite()&&(0.0..=10.0).contains(&self.relief),"ColorEmboss relief out of range");
  ensure!(self.contrast.is_finite()&&(0.0..=500.0).contains(&self.contrast),"ColorEmboss contrast out of range");
  ensure!(self.blend.is_finite()&&(0.0..=100.0).contains(&self.blend),"ColorEmboss blend out of range");Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;let(w,h)=frame.dimensions();let src=frame.pixels().to_vec();
  let dir=(self.direction as f64).to_radians();let(dx,dy)=((dir.cos()as f32)*self.relief,(-dir.sin()as f32)*self.relief);
  let contrast=self.contrast/100.0;let blend=self.blend/100.0;let mut index=0;
  let straight=|p:[f32;4]|if p[3]<=1e-6{[0.;3]}else{[p[0]/p[3],p[1]/p[3],p[2]/p[3]]};
  let luma=|p:[f32;3]|0.2126*p[0]+0.7152*p[1]+0.0722*p[2];
  frame.map_premultiplied(&mut |mut p|{
   let x=(index%w)as f32;let y=(index/w)as f32;index+=1;if p[3]<=1e-6{return p;}
   let a=super::mosaic::sample(&src,w,h,x+0.5+dx,y+0.5+dy);let b=super::mosaic::sample(&src,w,h,x+0.5-dx,y+0.5-dy);
   let d=luma(straight(a))-luma(straight(b));let v=straight(p).map(super::basic_grade::display);
   for k in 0..3 {let o=(v[k]+d*contrast*2.0).clamp(0.,1.);p[k]=super::basic_grade::linear((o+(v[k]-o)*blend).clamp(0.,1.))*p[3];}p
  },checkpoint)
 }
}
#[cfg(test)]mod tests {
 use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn uniform_color_is_preserved_alpha_retained_and_cancelled(){
 let source=GroupSurface{width:3,height:3,rgba:(0..9).flat_map(|_|[64,128,192,128]).collect()};let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut frame=original.clone();ColorEmboss::default().apply_linear(&mut frame,&mut||Ok(())).unwrap();for(p,q)in frame.pixels().iter().zip(original.pixels()){assert_eq!(p[3],q[3]);for c in 0..3{assert!((p[c]-q[c]).abs()<1e-6);}}assert!(ColorEmboss::default().apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());assert!(ColorEmboss{relief:10.1,..Default::default()}.validate().is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum ColorEmbossChannel {Direction,Relief,Contrast,Blend}
pub type ColorEmbossAnimation=std::collections::BTreeMap<ColorEmbossChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&ColorEmbossAnimation)->Result<()> {
 for(channel,curve)in animation {curve.validate()?;let(min,max)=match channel {ColorEmbossChannel::Direction=>(-(f32::MAX as f64),f32::MAX as f64),ColorEmbossChannel::Relief=>(0.0,10.0),ColorEmbossChannel::Contrast=>(0.0,500.0),ColorEmbossChannel::Blend=>(0.0,100.0)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"ColorEmboss animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:ColorEmboss,animation:&ColorEmbossAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ColorEmboss> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)?as f32;match channel {ColorEmbossChannel::Direction=>parameters.direction=value,ColorEmbossChannel::Relief=>parameters.relief=value,ColorEmbossChannel::Contrast=>parameters.contrast=value,ColorEmbossChannel::Blend=>parameters.blend=value}}
 parameters.validate()?;Ok(parameters)
}

#[cfg(test)]mod persistence_tests {use super::*;
 #[test]fn persisted_four_independent_local_curves() {
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"color_emboss","parameters":{},"animation":{"direction":curve(-45.0,45.0),"relief":curve(0.0,10.0),"contrast":curve(0.0,500.0),"blend":curve(0.0,100.0)}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let crate::domain::composition::VideoEffect::ColorEmboss{parameters,animation:Some(curves)}=effect else {panic!("ColorEmboss")};
  assert_eq!(sample_animation(parameters,&curves,250000).unwrap(),ColorEmboss{direction:0.0,relief:5.0,contrast:250.0,blend:50.0});
  let bad:ColorEmbossAnimation=serde_json::from_value(serde_json::json!({"relief":curve(0.0,10.1)})).unwrap();assert!(validate_animation(&bad).is_err());
 }

}

#[cfg(test)]mod original_tests {use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn original_working_frame_rgba_bits(){
 #[derive(Deserialize)]struct Case{parameters:ColorEmboss,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let f:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-color-emboss.json")).unwrap();assert_eq!(f.cases.len(),64);
 for(i,c)in f.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();c.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original Color Emboss {i}");}
 }
}
