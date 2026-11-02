//! Filmcraft Color Pass display-RGB distance selector, linear-gray fallback.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct ColorPass {pub color:[f32;4],pub similarity:f32,pub reverse:bool}
impl Default for ColorPass {fn default()->Self {Self{color:[1.0,0.0,0.0,1.0],similarity:10.0,reverse:false}}}
impl ColorPass {
 pub fn validate(&self)->Result<()> {
  ensure!(self.color.iter().all(|v|v.is_finite()),"nonfinite Color Pass key");
  ensure!(self.similarity.is_finite()&&(0.0..=100.0).contains(&self.similarity),"Color Pass similarity out of range");Ok(())
 }
 pub fn linear_rgb(&self,input:[f32;3])->[f32;3] {
  let encoded=input.map(|v|super::basic_grade::display(v.max(0.0)));
  let distance=((encoded[0]-self.color[0]).powi(2)+(encoded[1]-self.color[1]).powi(2)+(encoded[2]-self.color[2]).powi(2)).sqrt();
  if (distance<=self.similarity/100.0*1.2)!=self.reverse {input}else{[super::black_white::gray(input);3]}
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;frame.map_premultiplied(&mut|mut pixel|{let a=pixel[3];if a>1e-6 {let rgb=self.linear_rgb([pixel[0]/a,pixel[1]/a,pixel[2]/a]);for i in 0..3 {pixel[i]=rgb[i]*a;}}pixel},checkpoint)
 }
}
#[cfg(test)]mod tests {
 use super::*;
 #[test]
 fn persisted_similarity_and_color_sampling_preserves_reverse() {
  let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"color_pass","parameters":{"reverse":true},"animation":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":1000000,"value":100}]}},"colorAnimation":{"g":{"mode":"constant","value":0.75},"a":{"mode":"constant","value":0}}})).unwrap();
  let restored:super::super::composition::VideoEffect=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let super::super::composition::VideoEffect::ColorPass{parameters,animation:Some(scalar),color_animation:Some(colors)}=effect else {panic!("Color Pass")};
  let sampled=sample_color(sample_animation(parameters,&scalar,500000).unwrap(),&colors,500000).unwrap();assert_eq!(sampled.color,[1.0,0.75,0.0,0.0]);assert_eq!(sampled.similarity,50.0);assert!(sampled.reverse);
 }
 #[test]
 fn original_cpu_rgba_bits() {
  #[derive(serde::Deserialize)]struct Case {parameters:ColorPass,input:[f32;4],bits:[u32;4]}
  #[derive(serde::Deserialize)]struct Oracle {commit:String,cases:Vec<Case>}
  let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-color-pass.json")).unwrap();
  assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),144);
  for(index,case)in oracle.cases.iter().enumerate(){let mut actual=case.input;let a=actual[3];if a>1e-6 {let rgb=case.parameters.linear_rgb([actual[0]/a,actual[1]/a,actual[2]/a]);for i in 0..3 {actual[i]=rgb[i]*a;}}assert_eq!(actual.map(f32::to_bits),case.bits,"original Color Pass case {index}");}
 }
 #[test]fn selected_red_gray_fallback_and_reverse() {
  let effect=ColorPass::default();assert_eq!(effect.linear_rgb([1.0,0.0,0.0]),[1.0,0.0,0.0]);assert_eq!(effect.linear_rgb([0.0,1.0,0.0]),[0.7152;3]);
  let reversed=ColorPass{reverse:true,..effect};assert_eq!(reversed.linear_rgb([1.0,0.0,0.0]),[0.2126;3]);assert_eq!(reversed.linear_rgb([0.0,1.0,0.0]),[0.0,1.0,0.0]);
  assert!(ColorPass{similarity:101.0,..effect}.validate().is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum PassColorChannel {R,G,B,A}
pub type PassColorAnimation=std::collections::BTreeMap<PassColorChannel,super::composition::AnimatableValue>;
pub fn validate_color_animation(animation:&PassColorAnimation)->Result<()> {
 for value in animation.values(){value.validate()?;ensure!(super::composition::animatable_values_in(value,-(f32::MAX as f64),f32::MAX as f64,true),"Color Pass animation exceeds finite f32 range");}Ok(())
}
pub fn sample_color(mut parameters:ColorPass,animation:&PassColorAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ColorPass> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_color_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;let index=match channel {PassColorChannel::R=>0,PassColorChannel::G=>1,PassColorChannel::B=>2,PassColorChannel::A=>3};parameters.color[index]=value;}
 parameters.validate()?;Ok(parameters)
}

pub fn validate_animation(value:&super::composition::AnimatableValue)->Result<()> {
 value.validate()?;ensure!(super::composition::animatable_values_in(value,0.0,100.0,true),"Color Pass similarity animation out of range");Ok(())
}
pub fn sample_animation(mut parameters:ColorPass,value:&super::composition::AnimatableValue,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ColorPass> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(value)?;parameters.similarity=super::nested_composition::sample_value(value,time)? as f32;parameters.validate()?;Ok(parameters)
}
