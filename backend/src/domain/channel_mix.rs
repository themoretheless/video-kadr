//! Filmcraft Channel Mix encoded RGB matrix and monochrome contract.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct ChannelMix {
 pub rr:f32,
 pub rg:f32,
 pub rb:f32,
 pub rc:f32,
 pub gr:f32,
 pub gg:f32,
 pub gb:f32,
 pub gc:f32,
 pub br:f32,
 pub bg:f32,
 pub bb:f32,
 pub bc:f32,
 pub monochrome:bool,
}
impl Default for ChannelMix {fn default()->Self {Self{rr:100.0,rg:0.0,rb:0.0,rc:0.0,gr:0.0,gg:100.0,gb:0.0,gc:0.0,br:0.0,bg:0.0,bb:100.0,bc:0.0,monochrome:false}}}
impl ChannelMix {
 pub fn validate(&self)->Result<()> {
  ensure!([self.rr,self.rg,self.rb,self.rc,self.gr,self.gg,self.gb,self.gc,self.br,self.bg,self.bb,self.bc].iter().all(|v|v.is_finite()&&(-200.0..=200.0).contains(v)),"Channel Mix coefficient out of range");Ok(())
 }
 pub fn signal(&self,input:[f32;3])->[f32;3] {
  let v=input.map(|v|super::basic_grade::display(v.max(0.0)));
  let mut m=[[self.rr,self.rg,self.rb,self.rc],[self.gr,self.gg,self.gb,self.gc],[self.br,self.bg,self.bb,self.bc]].map(|row|row.map(|v|v/100.0));
  if self.monochrome {m=[m[0];3];}
  [0,1,2].map(|k|(m[k][0]*v[0]+m[k][1]*v[1]+m[k][2]*v[2]+m[k][3]).clamp(0.0,1.0))
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;frame.map_linear_to_signal(&mut|input,_,_|Ok(self.signal(input)),checkpoint)
 }
}
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="lowercase")]
pub enum MixChannel {Rr,Rg,Rb,Rc,Gr,Gg,Gb,Gc,Br,Bg,Bb,Bc}
pub type MixAnimation=std::collections::BTreeMap<MixChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&MixAnimation)->Result<()> {
 for value in animation.values(){value.validate()?;ensure!(super::composition::animatable_values_in(value,-200.0,200.0,true),"Channel Mix animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:ChannelMix,animation:&MixAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ChannelMix> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;match channel {MixChannel::Rr=>parameters.rr=value,MixChannel::Rg=>parameters.rg=value,MixChannel::Rb=>parameters.rb=value,MixChannel::Rc=>parameters.rc=value,MixChannel::Gr=>parameters.gr=value,MixChannel::Gg=>parameters.gg=value,MixChannel::Gb=>parameters.gb=value,MixChannel::Gc=>parameters.gc=value,MixChannel::Br=>parameters.br=value,MixChannel::Bg=>parameters.bg=value,MixChannel::Bb=>parameters.bb=value,MixChannel::Bc=>parameters.bc=value}}
 parameters.validate()?;Ok(parameters)
}
#[cfg(test)]mod tests {
 use super::*;
 #[test]fn original_cpu_rgba_bits() {
  #[derive(Deserialize)]struct Case {parameters:ChannelMix,input:[f32;4],bits:[u32;4]}
  #[derive(Deserialize)]struct Oracle {commit:String,cases:Vec<Case>}
  let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-channel-mix.json")).unwrap();
  assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),72);
  for(index,case)in oracle.cases.iter().enumerate(){let mut actual=case.input;let a=actual[3];if a>1e-6 {let signal=case.parameters.signal([actual[0]/a,actual[1]/a,actual[2]/a]);for i in 0..3 {actual[i]=super::super::basic_grade::linear(signal[i])*a;}}assert_eq!(actual.map(f32::to_bits),case.bits,"original Channel Mix case {index}");}
 }
 #[test]fn independent_persisted_coefficients_sample_local_clock_and_keep_monochrome() {
  let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"channel_mix","parameters":{"monochrome":true},"animation":{"rr":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":1000000,"value":200}]}},"rc":{"mode":"constant","value":-50},"gb":{"mode":"constant","value":25}}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let super::super::composition::VideoEffect::ChannelMix{parameters,animation:Some(animation)}=effect else {panic!("Channel Mix")};
  let sampled=sample_animation(parameters,&animation,500000).unwrap();assert_eq!(sampled.rr,100.0);assert_eq!(sampled.rc,-50.0);assert_eq!(sampled.gb,25.0);assert_eq!(sampled.gg,100.0);assert!(sampled.monochrome);
 }
 #[test]fn channel_swap_monochrome_constants_and_bounds() {
  let effect=ChannelMix{rr:0.0,rb:100.0,bb:0.0,br:100.0,..Default::default()};
  let encoded=super::super::basic_grade::display(1.0);
  let result=effect.signal([0.0,1.0,0.0]);assert_eq!(result,[0.0,encoded,0.0]);
  assert_eq!(effect.signal([1.0,0.0,0.0]),[0.0,0.0,encoded]);
  assert_eq!(ChannelMix{monochrome:true,..effect}.signal([0.0,0.0,1.0]),[encoded;3]);
  assert_eq!(ChannelMix{rc:200.0,gc:-200.0,..Default::default()}.signal([0.0,1.0,0.0]),[1.0,0.0,0.0]);
  assert!(ChannelMix{rr:201.0,..Default::default()}.validate().is_err());
 }
}
