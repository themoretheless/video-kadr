//! Filmcraft Leave Color: display-domain hue selection, linear-domain desaturation.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct LeaveColor {pub amount:f32,pub color:[f32;4],pub tolerance:f32,pub softness:f32}
impl Default for LeaveColor {fn default()->Self {Self{amount:0.0,color:[1.0,0.0,0.0,1.0],tolerance:15.0,softness:0.0}}}
impl LeaveColor {
    pub fn validate(&self)->Result<()> {
        ensure!([self.amount,self.tolerance,self.softness].iter().all(|v|v.is_finite()&&(0.0..=100.0).contains(v)),"Leave Color scalar out of range");
        ensure!(self.color.iter().all(|v|v.is_finite()),"nonfinite Leave Color key");Ok(())
    }
    pub fn linear_rgb(&self,input:[f32;3])->[f32;3] {
        let key=super::tone_curves::rgb_to_hsl(self.color[0],self.color[1],self.color[2])[0];
        let encoded=input.map(|v|super::basic_grade::display(v.max(0.0)));
        let hue=super::tone_curves::rgb_to_hsl(encoded[0],encoded[1],encoded[2])[0];
        let distance=(hue-key).abs().min(1.0-(hue-key).abs())*2.0;
        let keep=1.0-((distance-self.tolerance/100.0)/(self.softness/100.0+1e-4)).clamp(0.0,1.0);
        let l=super::black_white::gray(input);let k=self.amount/100.0*(1.0-keep);
        input.map(|v|v+(l-v)*k)
    }
    pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
        self.validate()?;
        frame.map_premultiplied(&mut|mut pixel|{let a=pixel[3];if a>1e-6 {let rgb=self.linear_rgb([pixel[0]/a,pixel[1]/a,pixel[2]/a]);for i in 0..3 {pixel[i]=rgb[i]*a;}}pixel},checkpoint)
    }
}
#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn original_cpu_rgba_bits() {
  #[derive(serde::Deserialize)]struct Case {parameters:LeaveColor,input:[f32;4],bits:[u32;4]}
  #[derive(serde::Deserialize)]struct Oracle {commit:String,cases:Vec<Case>}
  let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-leave-color.json")).unwrap();
  assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),72);
  for(index,case)in oracle.cases.iter().enumerate(){
   let mut actual=case.input;let a=actual[3];
   if a>1e-6 {let rgb=case.parameters.linear_rgb([actual[0]/a,actual[1]/a,actual[2]/a]);for i in 0..3 {actual[i]=rgb[i]*a;}}
   assert_eq!(actual.map(f32::to_bits),case.bits,"original Leave Color case {index}");
  }
 }
 #[test]
 fn persisted_color_component_local_sampling_keeps_scalars_and_other_components() {
  let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"leave_color","parameters":{"amount":75},"colorAnimation":{"r":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":1},{"tick":1000000,"value":0}]}},"g":{"mode":"constant","value":0.75},"a":{"mode":"constant","value":0}}})).unwrap();
  let restored:super::super::composition::VideoEffect=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let super::super::composition::VideoEffect::LeaveColor{parameters,color_animation:Some(animation),..}=effect else {panic!("Leave Color")};
  let sampled=sample_color(parameters,&animation,500000).unwrap();assert_eq!(sampled.color,[0.5,0.75,0.0,0.0]);assert_eq!(sampled.amount,75.0);assert_eq!(sampled.tolerance,15.0);
 }
 #[test]
 fn default_identity_selected_red_other_hues_linear_gray() {
  assert_eq!(LeaveColor::default().linear_rgb([0.25,0.5,0.75]),[0.25,0.5,0.75]);
  let effect=LeaveColor{amount:100.0,..Default::default()};
  assert_eq!(effect.linear_rgb([1.0,0.0,0.0]),[1.0,0.0,0.0]);
  assert!(effect.linear_rgb([0.0,1.0,0.0]).iter().all(|v|(*v-0.7152).abs()<1e-6));
  assert!(effect.linear_rgb([0.0,0.0,1.0]).iter().all(|v|(*v-0.0722).abs()<1e-6));
  assert!(LeaveColor{softness:101.0,..effect}.validate().is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum LeaveChannel {Amount,Tolerance,Softness}
pub type LeaveAnimation=std::collections::BTreeMap<LeaveChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&LeaveAnimation)->Result<()> {
 for value in animation.values(){value.validate()?;ensure!(super::composition::animatable_values_in(value,0.0,100.0,true),"Leave Color animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:LeaveColor,animation:&LeaveAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<LeaveColor> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;match channel {LeaveChannel::Amount=>parameters.amount=value,LeaveChannel::Tolerance=>parameters.tolerance=value,LeaveChannel::Softness=>parameters.softness=value}}
 parameters.validate()?;Ok(parameters)
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum LeaveColorChannel {R,G,B,A}
pub type LeaveColorAnimation=std::collections::BTreeMap<LeaveColorChannel,super::composition::AnimatableValue>;
pub fn validate_color_animation(animation:&LeaveColorAnimation)->Result<()> {
 for value in animation.values(){value.validate()?;ensure!(super::composition::animatable_values_in(value,-(f32::MAX as f64),f32::MAX as f64,true),"Leave Color animation exceeds finite f32 range");}Ok(())
}
pub fn sample_color(mut parameters:LeaveColor,animation:&LeaveColorAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<LeaveColor> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_color_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;let index=match channel {LeaveColorChannel::R=>0,LeaveColorChannel::G=>1,LeaveColorChannel::B=>2,LeaveColorChannel::A=>3};parameters.color[index]=value;}
 parameters.validate()?;Ok(parameters)
}
