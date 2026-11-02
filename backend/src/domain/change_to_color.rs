//! Filmcraft Change to Color display-domain hue replacement (MIT pinned CPU renderer).
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct ChangeToColor {pub from:[f32;4],pub to:[f32;4],pub hue_tol:f32,pub softness:f32}
impl Default for ChangeToColor {fn default()->Self {Self{from:[1.0,0.0,0.0,1.0],to:[0.0,0.0,1.0,1.0],hue_tol:5.0,softness:50.0}}}
impl ChangeToColor {
 pub fn validate(&self)->Result<()> {
  ensure!(self.from.iter().chain(self.to.iter()).all(|v|v.is_finite()),"nonfinite Change to Color endpoint");
  ensure!([self.hue_tol,self.softness].iter().all(|v|v.is_finite()&&(0.0..=100.0).contains(v)),"Change to Color scalar out of range");Ok(())
 }
 pub fn signal(&self,input:[f32;3])->[f32;3] {
  let hue=|c:[f32;4]|super::tone_curves::rgb_to_hsl(c[0],c[1],c[2])[0];
  let from=hue(self.from);let to=hue(self.to);
  let encoded=input.map(|v|super::basic_grade::display(v.max(0.0)));
  let mut hsl=super::tone_curves::rgb_to_hsl(encoded[0],encoded[1],encoded[2]);
  let distance=(hsl[0]-from).abs().min(1.0-(hsl[0]-from).abs());
  let weight=1.0-((distance-self.hue_tol/100.0)/(self.softness/100.0*0.3+1e-4)).clamp(0.0,1.0);
  hsl[0]=(hsl[0]+(to-from)*weight).rem_euclid(1.0);
  super::tone_curves::hsl_to_rgb(hsl[0],hsl[1],hsl[2])
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;frame.map_linear_to_signal(&mut|input,_,_|Ok(self.signal(input)),checkpoint)
 }
}
#[cfg(test)]mod tests {
 use super::*;
 #[test]
 fn persisted_endpoint_components_preserve_scalar_and_other_color_channels() {
  let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"change_to_color","parameters":{"softness":25},"colorAnimation":{"fromR":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":1},{"tick":1000000,"value":0}]}},"toG":{"mode":"constant","value":0.75},"toA":{"mode":"constant","value":0}}})).unwrap();
  let restored:super::super::composition::VideoEffect=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let super::super::composition::VideoEffect::ChangeToColor{parameters,color_animation:Some(animation),..}=effect else {panic!("Change to Color")};
  let sampled=sample_colors(parameters,&animation,500000).unwrap();assert_eq!(sampled.from,[0.5,0.0,0.0,1.0]);assert_eq!(sampled.to,[0.0,0.75,1.0,0.0]);assert_eq!(sampled.softness,25.0);assert_eq!(sampled.hue_tol,5.0);
 }
 #[test]
 fn original_cpu_rgba_bits() {
  #[derive(serde::Deserialize)]struct Case {parameters:ChangeToColor,input:[f32;4],bits:[u32;4]}
  #[derive(serde::Deserialize)]struct Oracle {commit:String,cases:Vec<Case>}
  let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-change-to-color.json")).unwrap();
  assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),72);
  for(index,case)in oracle.cases.iter().enumerate(){
   let mut actual=case.input;let a=actual[3];
   if a>1e-6 {let signal=case.parameters.signal([actual[0]/a,actual[1]/a,actual[2]/a]);for i in 0..3 {actual[i]=super::super::basic_grade::linear(signal[i].clamp(0.0,1.0))*a;}}
   assert_eq!(actual.map(f32::to_bits),case.bits,"original Change to Color case {index}");
  }
 }
 #[test]fn defaults_change_red_to_blue_and_preserve_gray() {
  let effect=ChangeToColor::default();let blue=effect.signal([1.0,0.0,0.0]);assert!(blue[0].abs()<1e-6&&blue[1].abs()<1e-6&&(blue[2]-1.0).abs()<1e-6);
  let gray=effect.signal([0.25;3]);assert!((gray[0]-gray[1]).abs()<1e-6&&(gray[1]-gray[2]).abs()<1e-6);
  assert!(ChangeToColor{hue_tol:101.0,..effect}.validate().is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum ChangeChannel {HueTol,Softness}
pub type ChangeAnimation=std::collections::BTreeMap<ChangeChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&ChangeAnimation)->Result<()> {
 for value in animation.values(){value.validate()?;ensure!(super::composition::animatable_values_in(value,0.0,100.0,true),"Change to Color animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:ChangeToColor,animation:&ChangeAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ChangeToColor> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;match channel {ChangeChannel::HueTol=>parameters.hue_tol=value,ChangeChannel::Softness=>parameters.softness=value}}
 parameters.validate()?;Ok(parameters)
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum ChangeColorChannel {FromR,FromG,FromB,FromA,ToR,ToG,ToB,ToA}
pub type ChangeColorAnimation=std::collections::BTreeMap<ChangeColorChannel,super::composition::AnimatableValue>;
pub fn validate_color_animation(animation:&ChangeColorAnimation)->Result<()> {
    for value in animation.values() {value.validate()?;ensure!(super::composition::animatable_values_in(value,-(f32::MAX as f64),f32::MAX as f64,true),"Change to Color endpoint animation exceeds finite f32 range");}
    Ok(())
}
pub fn sample_colors(mut parameters:ChangeToColor,animation:&ChangeColorAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ChangeToColor> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    validate_color_animation(animation)?;
    for (channel,value) in animation {
        let value=super::nested_composition::sample_value(value,time)? as f32;
        let (color,index)=match channel {
            ChangeColorChannel::FromR=>(&mut parameters.from,0),ChangeColorChannel::FromG=>(&mut parameters.from,1),
            ChangeColorChannel::FromB=>(&mut parameters.from,2),ChangeColorChannel::FromA=>(&mut parameters.from,3),
            ChangeColorChannel::ToR=>(&mut parameters.to,0),ChangeColorChannel::ToG=>(&mut parameters.to,1),
            ChangeColorChannel::ToB=>(&mut parameters.to,2),ChangeColorChannel::ToA=>(&mut parameters.to,3),
        };color[index]=value;
    }
    parameters.validate()?;Ok(parameters)
}
