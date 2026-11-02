//! Filmcraft Emboss CPU reference, source-resolution pixel scale.
//! MIT source attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Emboss {pub direction:f32,pub relief:f32,pub contrast:f32,pub blend:f32}
impl Default for Emboss {fn default()->Self {Self{direction:45.0,relief:1.8,contrast:100.0,blend:0.0}}}
impl Emboss {
 pub fn validate(&self)->Result<()> {
  ensure!(self.direction.is_finite(),"Emboss direction out of range");
  ensure!(self.relief.is_finite()&&(0.0..=10.0).contains(&self.relief),"Emboss relief out of range");
  ensure!(self.contrast.is_finite()&&(0.0..=500.0).contains(&self.contrast),"Emboss contrast out of range");
  ensure!(self.blend.is_finite()&&(0.0..=100.0).contains(&self.blend),"Emboss blend out of range");Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;let(w,h)=frame.dimensions();let src=frame.pixels().to_vec();
  let dir=(self.direction as f64).to_radians();let(dx,dy)=((dir.cos()as f32)*self.relief,(-dir.sin()as f32)*self.relief);
  let contrast=self.contrast/100.0;let blend=self.blend/100.0;let mut index=0;
  let luma=|p:[f32;4]|0.2126*p[0]+0.7152*p[1]+0.0722*p[2];
  frame.map_premultiplied(&mut |_|{
   let x=(index%w)as f32;let y=(index/w)as f32;let original=src[index];index+=1;
   let a=super::mosaic::sample(&src,w,h,x+0.5+dx,y+0.5+dy);
   let b=super::mosaic::sample(&src,w,h,x+0.5-dx,y+0.5-dy);
   let value=super::basic_grade::linear((0.5+(luma(a)-luma(b))*contrast*2.0).clamp(0.0,1.0));
   let mut out=original;for k in 0..3 {out[k]=value*original[3]*(1.0-blend)+original[k]*blend;}out
  },checkpoint)
 }
}
#[cfg(test)]mod tests {
 use super::*;
 #[test]fn persisted_four_independent_local_curves() {
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"emboss","parameters":{},"animation":{"direction":curve(-45.0,45.0),"relief":curve(0.0,10.0),"contrast":curve(0.0,500.0),"blend":curve(0.0,100.0)}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let crate::domain::composition::VideoEffect::Emboss{parameters,animation:Some(curves)}=effect else {panic!("Emboss")};
  assert_eq!(sample_animation(parameters,&curves,250000).unwrap(),Emboss{direction:0.0,relief:5.0,contrast:250.0,blend:50.0});
  let bad:EmbossAnimation=serde_json::from_value(serde_json::json!({"relief":curve(0.0,10.1)})).unwrap();assert!(validate_animation(&bad).is_err());
 }

    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case { parameters: Emboss, width: usize, height: usize, rgba: Vec<u8>, bits: Vec<u32> }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-emboss.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(),64);
        for (index, case) in oracle.cases.into_iter().enumerate() {
            let source = GroupSurface {width:case.width,height:case.height,rgba:case.rgba};
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame, &mut || Ok(())).unwrap();
            let bits: Vec<u32> = frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original Emboss frame {index}");
        }
    }

 use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn zero_relief_gray_alpha_fullblend_and_cancellation() {
  let source=GroupSurface{width:1,height:3,rgba:vec![255,0,0,255,0,255,0,128,0,0,255,0]};
  let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut gray=original.clone();
  Emboss{relief:0.0,..Default::default()}.apply_linear(&mut gray,&mut||Ok(())).unwrap();
  let value=super::super::basic_grade::linear(0.5);
  for(p,o)in gray.pixels().iter().zip(original.pixels()) {assert_eq!(*p,[value*o[3],value*o[3],value*o[3],o[3]]);}
  let mut identity=original.clone();Emboss{blend:100.0,..Default::default()}.apply_linear(&mut identity,&mut||Ok(())).unwrap();assert_eq!(identity.pixels(),original.pixels());
  assert!(Emboss::default().apply_linear(&mut identity,&mut||anyhow::bail!("cancelled")).is_err());assert!(Emboss{relief:10.1,..Default::default()}.validate().is_err());
 }
}
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum EmbossChannel {Direction,Relief,Contrast,Blend}
pub type EmbossAnimation=std::collections::BTreeMap<EmbossChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&EmbossAnimation)->Result<()> {
 for(channel,curve)in animation {curve.validate()?;let(min,max)=match channel {EmbossChannel::Direction=>(-(f32::MAX as f64),f32::MAX as f64),EmbossChannel::Relief=>(0.0,10.0),EmbossChannel::Contrast=>(0.0,500.0),EmbossChannel::Blend=>(0.0,100.0)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"Emboss animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:Emboss,animation:&EmbossAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Emboss> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)?as f32;match channel {EmbossChannel::Direction=>parameters.direction=value,EmbossChannel::Relief=>parameters.relief=value,EmbossChannel::Contrast=>parameters.contrast=value,EmbossChannel::Blend=>parameters.blend=value}}
 parameters.validate()?;Ok(parameters)
}
