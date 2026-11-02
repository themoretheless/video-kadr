//! Filmcraft Bevel Alpha CPU reference at source-resolution pixel scale.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct BevelAlpha {pub thickness:f32,pub angle:f32,pub color:[f32;4],pub intensity:f32}
impl Default for BevelAlpha {fn default()->Self {Self{thickness:2.0,angle:-60.0,color:[1.0;4],intensity:40.0}}}
impl BevelAlpha {
 pub fn validate(&self)->Result<()> {
  ensure!(self.thickness.is_finite()&&(0.0..=200.0).contains(&self.thickness),"Bevel Alpha thickness out of range");
  ensure!(self.angle.is_finite()&&self.color.iter().all(|v|v.is_finite()),"Bevel Alpha angle or color out of range");
  ensure!(self.intensity.is_finite()&&(0.0..=100.0).contains(&self.intensity),"Bevel Alpha intensity out of range");Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;let(w,h)=frame.dimensions();let source=frame.pixels().to_vec();let angle=(self.angle as f64).to_radians();let(lx,ly)=(angle.cos()as f32,-angle.sin()as f32);let intensity=self.intensity/100.0;let mut index=0;
  frame.map_premultiplied(&mut |mut p| {
   let(x,y)=((index%w)as f32,(index/w)as f32);index+=1;
   let alpha=|dx:f32,dy:f32|super::mosaic::sample(&source,w,h,x+0.5+dx,y+0.5+dy)[3];
   let gx=alpha(self.thickness,0.0)-alpha(-self.thickness,0.0);let gy=alpha(0.0,self.thickness)-alpha(0.0,-self.thickness);
   let shade=-(gx*lx+gy*ly)*intensity;
   // The upstream CPU path retains Light Color but does not use it in this operation.
   for c in 0..3 {p[c]=(p[c]+shade*p[3]).clamp(0.0,p[3]);}p
  },checkpoint)
 }
}
#[cfg(test)]mod tests {
 use super::*;
 use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case { parameters: BevelAlpha, width: usize, height: usize, rgba: Vec<u8>, bits: Vec<u32> }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-bevel-alpha.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(),128);
        for (index, case) in oracle.cases.into_iter().enumerate() {
            let source = GroupSurface {width:case.width,height:case.height,rgba:case.rgba};
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame, &mut || Ok(())).unwrap();
            let bits: Vec<u32> = frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original BevelAlpha frame {index}");
        }
    }

 #[test]fn persisted_seven_curves_local_sampling_and_admission() {
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"bevel_alpha","parameters":{},"animation":{"colorR":curve(0.0,1.0),"colorG":curve(1.0,0.0),"colorB":curve(-1.0,1.0),"colorA":curve(0.0,1.0),"thickness":curve(0.0,10.0),"angle":curve(-45.0,45.0),"intensity":curve(0.0,100.0)}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let crate::domain::composition::VideoEffect::BevelAlpha{parameters,animation:Some(curves)}=effect else {panic!("Bevel Alpha")};
  assert_eq!(sample_animation(parameters,&curves,250000).unwrap(),BevelAlpha{color:[0.5,0.5,0.0,0.5],thickness:5.0,angle:0.0,intensity:50.0});
  let invalid:BevelAnimation=serde_json::from_value(serde_json::json!({"thickness":curve(0.0,200.1)})).unwrap();assert!(validate_animation(&invalid).is_err());
 }
 #[test]fn zero_thickness_identity_alpha_preservation_and_cancellation() {
  let source=GroupSurface{width:3,height:1,rgba:vec![255,255,255,255,128,128,128,128,0,0,0,0]};let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut frame=original.clone();
  BevelAlpha{thickness:0.0,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),original.pixels());
  BevelAlpha::default().apply_linear(&mut frame,&mut||Ok(())).unwrap();for(p,o)in frame.pixels().iter().zip(original.pixels()){assert_eq!(p[3],o[3]);for c in 0..3 {assert!((0.0..=p[3]).contains(&p[c]));}}
  assert!(BevelAlpha::default().apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());assert!(BevelAlpha{thickness:201.0,..Default::default()}.validate().is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum BevelChannel {ColorR,ColorG,ColorB,ColorA,Thickness,Angle,Intensity}
pub type BevelAnimation=std::collections::BTreeMap<BevelChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&BevelAnimation)->Result<()> {
 for(channel,curve)in animation {curve.validate()?;let(min,max)=match channel {BevelChannel::Thickness=>(0.0,200.0),BevelChannel::Intensity=>(0.0,100.0),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"Bevel Alpha animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:BevelAlpha,animation:&BevelAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<BevelAlpha> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;match channel {BevelChannel::ColorR=>parameters.color[0]=value,BevelChannel::ColorG=>parameters.color[1]=value,BevelChannel::ColorB=>parameters.color[2]=value,BevelChannel::ColorA=>parameters.color[3]=value,BevelChannel::Thickness=>parameters.thickness=value,BevelChannel::Angle=>parameters.angle=value,BevelChannel::Intensity=>parameters.intensity=value}}
 parameters.validate()?;Ok(parameters)
}
