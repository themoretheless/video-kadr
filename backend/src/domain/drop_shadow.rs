//! Filmcraft Drop Shadow CPU reference at source-resolution pixel scale.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct DropShadow {pub color:[f32;4],pub opacity:f32,pub direction:f32,pub distance:f32,pub softness:f32,pub only:bool}
impl Default for DropShadow {fn default()->Self {Self{color:[0.0,0.0,0.0,1.0],opacity:50.0,direction:135.0,distance:5.0,softness:0.0,only:false}}}
impl DropShadow {
 pub fn validate(&self)->Result<()> {
  ensure!(self.color.iter().all(|v|v.is_finite())&&self.direction.is_finite(),"Drop Shadow color or angle out of range");
  for(value,max)in [(self.opacity,100.0),(self.distance,4000.0),(self.softness,1000.0)] {ensure!(value.is_finite()&&(0.0..=max).contains(&value),"Drop Shadow parameter out of range");}Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.apply_linear_scaled(frame,1.,checkpoint)
 }
 /// Original source pixel scale keeps displacement arithmetic in f64.
 pub fn apply_linear_scaled(&self,frame:&mut super::linear_color_frame::LinearColorFrame,scale:f32,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;ensure!(scale.is_finite()&&scale>0.,"invalid Drop Shadow scale");checkpoint()?;let(w,h)=frame.dimensions();let original=frame.pixels().to_vec();
  let direction=(self.direction as f64).to_radians();let distance=self.distance as f64*scale as f64;let(dx,dy)=(direction.sin()*distance,-direction.cos()*distance);
  let x0=(dx.floor().max(0.0)as usize).min(w);let x1=((dx+w as f64).ceil().max(0.0)as usize).min(w);
  let y0=(dy.floor().max(0.0)as usize).min(h);let y1=((dy+h as f64).ceil().max(0.0)as usize).min(h);
  let color:[f32;3]=std::array::from_fn(|c|super::basic_grade::linear(self.color[c].clamp(0.0,1.0)));let opacity=self.opacity/100.0;let mut index=0;
  let mut shadow=frame.clone();shadow.map_premultiplied(&mut |_| {
   let(x,y)=(index%w,index/w);index+=1;
   let alpha=if x<x0||x>=x1||y<y0||y>=y1 {0.0}else{super::twirl::sample(&original,w,h,(x as f64+0.5-dx)as f32,(y as f64+0.5-dy)as f32)[3]}*opacity;
   [color[0]*alpha,color[1]*alpha,color[2]*alpha,alpha]
  },checkpoint)?;
  let sigma=self.softness*scale*0.5;
  if sigma>0.3 {super::gaussian_blur::GaussianBlur::default().apply_sigma(&mut shadow,sigma,checkpoint)?;}
  if !self.only {let mut i=0;shadow.map_premultiplied(&mut |destination| {let source=original[i];i+=1;if source[3]<=0.0{return destination;}let k=1.0-source[3];std::array::from_fn(|c|source[c]+destination[c]*k)},checkpoint)?;}
  *frame=shadow;Ok(())
 }
}
#[cfg(test)]mod tests {
 use super::*;
 use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case { parameters: DropShadow, width: usize, height: usize, rgba: Vec<u8>, bits: Vec<u32> }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-drop-shadow.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(),128);
        for (index, case) in oracle.cases.into_iter().enumerate() {
            let source = GroupSurface {width:case.width,height:case.height,rgba:case.rgba};
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame, &mut || Ok(())).unwrap();
            let bits: Vec<u32> = frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original DropShadow frame {index}");
        }
    }

 #[test]fn persisted_eight_curves_local_sampling_and_admission() {
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"drop_shadow","parameters":{"only":true},"animation":{"colorR":curve(0.0,1.0),"colorG":curve(1.0,0.0),"colorB":curve(-1.0,1.0),"colorA":curve(0.0,1.0),"opacity":curve(0.0,100.0),"direction":curve(-45.0,45.0),"distance":curve(0.0,10.0),"softness":curve(0.0,20.0)}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let crate::domain::composition::VideoEffect::DropShadow{parameters,animation:Some(curves)}=effect else {panic!("Drop Shadow")};
  assert_eq!(sample_animation(parameters,&curves,250000).unwrap(),DropShadow{color:[0.5,0.5,0.0,0.5],opacity:50.0,direction:0.0,distance:5.0,softness:10.0,only:true});
  let invalid:ShadowAnimation=serde_json::from_value(serde_json::json!({"softness":curve(0.0,1000.1)})).unwrap();assert!(validate_animation(&invalid).is_err());
 }
 #[test]fn only_shadow_alpha_ignored_color_alpha_and_cancellation() {
  let source=GroupSurface{width:1,height:1,rgba:vec![255,255,255,128]};let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut frame=original.clone();
  let effect=DropShadow{distance:0.0,color:[1.0,0.0,0.0,0.0],only:true,..Default::default()};effect.apply_linear(&mut frame,&mut||Ok(())).unwrap();let alpha=original.pixels()[0][3]*0.5;assert_eq!(frame.pixels()[0],[alpha,0.0,0.0,alpha]);
  frame=original.clone();DropShadow{opacity:0.0,distance:0.0,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),original.pixels());
  assert!(effect.apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());assert!(DropShadow{softness:1001.0,..effect}.validate().is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum ShadowChannel {ColorR,ColorG,ColorB,ColorA,Opacity,Direction,Distance,Softness}
pub type ShadowAnimation=std::collections::BTreeMap<ShadowChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&ShadowAnimation)->Result<()> {
 for(channel,curve)in animation {curve.validate()?;let(min,max)=match channel {ShadowChannel::Opacity=>(0.0,100.0),ShadowChannel::Distance=>(0.0,4000.0),ShadowChannel::Softness=>(0.0,1000.0),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"Drop Shadow animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:DropShadow,animation:&ShadowAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<DropShadow> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;match channel {ShadowChannel::ColorR=>parameters.color[0]=value,ShadowChannel::ColorG=>parameters.color[1]=value,ShadowChannel::ColorB=>parameters.color[2]=value,ShadowChannel::ColorA=>parameters.color[3]=value,ShadowChannel::Opacity=>parameters.opacity=value,ShadowChannel::Direction=>parameters.direction=value,ShadowChannel::Distance=>parameters.distance=value,ShadowChannel::Softness=>parameters.softness=value}}
 parameters.validate()?;Ok(parameters)
}
