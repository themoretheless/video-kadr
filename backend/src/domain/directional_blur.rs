//! Filmcraft directional blur: centered edge-clamped bilinear taps on linear RGBA.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,Default,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct DirectionalBlur {pub direction:f32,pub length:f32}
impl DirectionalBlur {
 pub fn validate(&self)->Result<()> {
  ensure!(self.direction.is_finite()&&self.length.is_finite()&&(0.0..=1000.0).contains(&self.length),"Directional Blur parameter out of range");Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;
  let len=self.length*2.0;
  if len<0.5 {return Ok(());}
  let dir=(self.direction as f64).to_radians();let steps=(len.ceil() as usize).clamp(2,64);
  let (dx,dy)=((dir.sin() as f32)*len,(-dir.cos() as f32)*len);
  let (w,h)=frame.dimensions();let src=frame.pixels().to_vec();
  let sample=|x:f32,y:f32|-> [f32;4] {
   let fx=(x-0.5).clamp(0.0,w as f32-1.0);let fy=(y-0.5).clamp(0.0,h as f32-1.0);
   let x0=fx.floor() as usize;let y0=fy.floor() as usize;
   let x1=(x0+1).min(w-1);let y1=(y0+1).min(h-1);
   let tx=fx-x0 as f32;let ty=fy-y0 as f32;
   let (a,b,c,d)=(src[y0*w+x0],src[y0*w+x1],src[y1*w+x0],src[y1*w+x1]);
   std::array::from_fn(|k|{let top=a[k]+(b[k]-a[k])*tx;let bot=c[k]+(d[k]-c[k])*tx;top+(bot-top)*ty})
  };
  let mut i=0;
  frame.map_premultiplied(&mut|_|{
   let (x,y)=(i%w,i/w);i+=1;let mut acc=[0.0;4];
   for s in 0..steps {let t=s as f32/(steps-1) as f32-0.5;let p=sample(x as f32+0.5+dx*t,y as f32+0.5+dy*t);for k in 0..4 {acc[k]+=p[k];}}
   acc.map(|v|v/steps as f32)
  },checkpoint)
 }
}
#[cfg(test)]mod tests {
 use super::*;
 #[test]fn persisted_independent_angle_and_length_curves() {
  let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"directional_blur","parameters":{},"animation":{"direction":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":-45},{"tick":500000,"value":45}]}},"length":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":100}]}}}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let super::super::composition::VideoEffect::DirectionalBlur{parameters,animation:Some(curves)}=effect else {panic!("Directional Blur")};
  assert_eq!(sample_animation(parameters,&curves,250000).unwrap(),DirectionalBlur{direction:0.0,length:50.0});
 }
 #[test]fn original_working_frame_rgba_bits() {
  #[derive(Deserialize)]struct Case {parameters:DirectionalBlur,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
  #[derive(Deserialize)]struct Oracle {commit:String,cases:Vec<Case>}
  let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-directional-blur.json")).unwrap();
  assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),100);
  for (index,case) in oracle.cases.iter().enumerate() {
   let source=super::super::group_surface::GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()};
   let mut frame=super::super::linear_color_frame::LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
   case.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();
   let bits:Vec<u32>=frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
   assert_eq!(bits,case.bits,"original Directional Blur frame {index}");
  }
 }
 #[test]fn no_op_threshold_ranges_and_cancellation() {
  let source=super::super::group_surface::GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)};
  let mut frame=super::super::linear_color_frame::LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let before=frame.pixels().to_vec();
  DirectionalBlur{length:0.2,direction:45.0}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),before);
  assert!(DirectionalBlur::default().apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());
  assert!(DirectionalBlur{length:1000.1,..Default::default()}.validate().is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum DirectionalChannel {Direction,Length}
pub type DirectionalAnimation=std::collections::BTreeMap<DirectionalChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&DirectionalAnimation)->Result<()> {
 for (channel,value) in animation {value.validate()?;let (min,max)=match channel {DirectionalChannel::Length=>(0.0,1000.0),DirectionalChannel::Direction=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(value,min,max,true),"Directional Blur animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:DirectionalBlur,animation:&DirectionalAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<DirectionalBlur> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for (channel,curve) in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;match channel {DirectionalChannel::Direction=>parameters.direction=value,DirectionalChannel::Length=>parameters.length=value}}
 parameters.validate()?;Ok(parameters)
}
