//! Filmcraft legacy Camera Blur: sigma=percent*.3, both axes, repeated edges.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,Default,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct CameraBlur {pub percent:f32}
impl CameraBlur {
 pub fn validate(&self)->Result<()> {ensure!(self.percent.is_finite()&&(0.0..=100.0).contains(&self.percent),"Camera Blur percent out of range");Ok(())}
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;super::gaussian_blur::GaussianBlur{dimensions:0,repeat_edge:true,blurriness:0.0}.apply_sigma(frame,self.percent*0.3,checkpoint)
 }
}
pub fn validate_animation(animation:&super::composition::AnimatableValue)->Result<()> {
 animation.validate()?;ensure!(super::composition::animatable_values_in(animation,0.0,100.0,true),"Camera Blur animation out of range");Ok(())
}
pub fn sample_animation(mut parameters:CameraBlur,animation:&super::composition::AnimatableValue,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<CameraBlur> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;parameters.percent=super::nested_composition::sample_value(animation,time)? as f32;parameters.validate()?;Ok(parameters)
}

#[cfg(test)]mod tests {
 use super::*;
 #[test]fn persisted_local_percent_curve_and_range() {
  let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"camera_blur","parameters":{},"animation":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":100}]}}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let super::super::composition::VideoEffect::CameraBlur{parameters,animation:Some(curve)}=effect else {panic!("Camera Blur")};
  assert_eq!(sample_animation(parameters,&curve,250000).unwrap(),CameraBlur{percent:50.0});
  assert!(CameraBlur{percent:100.1}.validate().is_err());
 }
 #[test]fn original_working_frame_rgba_bits() {
  #[derive(Deserialize)]struct Case {parameters:CameraBlur,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
  #[derive(Deserialize)]struct Oracle {commit:String,cases:Vec<Case>}
  let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-camera-blur.json")).unwrap();
  assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),20);
  for (index,case) in oracle.cases.iter().enumerate() {
   let source=super::super::group_surface::GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()};
   let mut frame=super::super::linear_color_frame::LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
   case.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();
   let bits:Vec<u32>=frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
   assert_eq!(bits,case.bits,"original Camera Blur frame {index}");
  }
 }
}
