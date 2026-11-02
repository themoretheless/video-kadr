//! Filmcraft vfx Noise CPU reference at source-resolution pixel scale.
//! MIT source attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Noise {pub amount:f32,pub color:bool,pub clip:bool,pub grain_size:f32}
impl Default for Noise {fn default()->Self {Self{amount:0.0,color:true,clip:true,grain_size:1.0}}}
fn hash3(x:usize,y:usize,z:u64)->f32 {
 let mut h=(x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)^(y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)^z.wrapping_mul(0x1656_67B1_9E37_79F9);
 h^=h>>31;h=h.wrapping_mul(0xBF58_476D_1CE4_E5B9);h^=h>>29;(h>>40)as f32/(1u64<<24)as f32
}
impl Noise {
 pub fn validate(&self)->Result<()> {ensure!(self.amount.is_finite()&&(0.0..=100.0).contains(&self.amount),"Noise amount out of range");ensure!(self.grain_size.is_finite()&&(1.0..=10.0).contains(&self.grain_size),"Noise grain size out of range");Ok(())}
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,local_tick:u64,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
 self.apply_linear_at_seconds(frame,local_tick as f64/1_000_000.0,checkpoint)
 }
 pub fn apply_linear_at_seconds(&self,frame:&mut super::linear_color_frame::LinearColorFrame,seconds:f64,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
 ensure!(seconds.is_finite() && seconds.abs() <= i64::MAX as f64 / 254016000000.0,"invalid signed generator time");
  self.validate()?;checkpoint()?;let amount=self.amount/100.0;if amount<=0.0{return Ok(());}
  let(w,_)=frame.dimensions();let grain=self.grain_size.max(1.0);let seed=(seconds*30.0)as u64;let mut index=0;
  frame.map_premultiplied(&mut|pixel| {
   let x=index%w;let y=index/w;index+=1;let alpha=pixel[3];if alpha<=0.0{return pixel;}
   let(gx,gy)=((x as f32/grain)as usize,(y as f32/grain)as usize);let h=|z|hash3(gx,gy,z)-0.5;let n=h(seed);
   let ns=if self.color {[n,h(seed+7777),h(seed+99991)]}else{[n;3]};let mut out=pixel;
   for k in 0..3 {let c=super::basic_grade::display((pixel[k]/alpha).max(0.0));let mut v=c+ns[k]*amount;if self.clip {v=v.clamp(0.0,1.0);}out[k]=super::basic_grade::linear(v.clamp(0.0,1.0))*alpha;}out
  },checkpoint)
 }
}
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum NoiseChannel {Amount,GrainSize}
pub type NoiseAnimation=std::collections::BTreeMap<NoiseChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&NoiseAnimation)->Result<()> {for(channel,curve)in animation {curve.validate()?;let(min,max)=match channel {NoiseChannel::Amount=>(0.0,100.0),NoiseChannel::GrainSize=>(1.0,10.0)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"Noise animation out of range");}Ok(())}
pub fn sample_animation(mut parameters:Noise,animation:&NoiseAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Noise> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();
parameters.validate()?;validate_animation(animation)?;for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)?as f32;match channel {NoiseChannel::Amount=>parameters.amount=value,NoiseChannel::GrainSize=>parameters.grain_size=value}}parameters.validate()?;Ok(parameters)}
#[cfg(test)]mod tests {
 use super::*;
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        #[serde(rename_all="camelCase")]
        struct Case { local_tick:u64, parameters: Noise, width: usize, height: usize, rgba: Vec<u8>, bits: Vec<u32> }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-noise.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(),96);
        for (index, case) in oracle.cases.into_iter().enumerate() {
            let source = GroupSurface {width:case.width,height:case.height,rgba:case.rgba};
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame, case.local_tick, &mut || Ok(())).unwrap();
            let bits: Vec<u32> = frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original Noise frame {index}");
        }
    }

 #[test]fn persisted_amount_and_grain_curves() {
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"noise","parameters":{"color":false,"clip":false},"animation":{"amount":curve(0.0,100.0),"grainSize":curve(1.0,10.0)}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let crate::domain::composition::VideoEffect::Noise{parameters,animation:Some(curves)}=effect else {panic!("Noise")};
  assert_eq!(sample_animation(parameters,&curves,250000).unwrap(),Noise{amount:50.0,grain_size:5.5,color:false,clip:false});
  let bad:NoiseAnimation=serde_json::from_value(serde_json::json!({"grainSize":curve(1.0,10.1)})).unwrap();assert!(validate_animation(&bad).is_err());
 }

use crate::domain::{linear_color_frame::LinearColorFrame,group_surface::GroupSurface};
 #[test]fn deterministic_grain_cells_local_frame_clock_alpha_and_cancellation(){
  let source=GroupSurface{width:4,height:2,rgba:[128,128,128,128].repeat(8)};let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
  let noise=Noise{amount:100.0,color:false,grain_size:2.0,..Default::default()};let mut a=original.clone();let mut b=original.clone();let mut c=original.clone();
  noise.apply_linear(&mut a,0,&mut||Ok(())).unwrap();noise.apply_linear(&mut b,33333,&mut||Ok(())).unwrap();noise.apply_linear(&mut c,33334,&mut||Ok(())).unwrap();assert_eq!(a.pixels(),b.pixels());assert_ne!(a.pixels(),c.pixels());assert_eq!(a.pixels()[0],a.pixels()[1]);assert_eq!(a.pixels()[0],a.pixels()[4]);
  assert!(a.pixels().iter().all(|p|p[0]==p[1]&&p[1]==p[2]&&p[3]==128.0/255.0));
  let mut identity=original.clone();Noise::default().apply_linear(&mut identity,0,&mut||Ok(())).unwrap();assert_eq!(identity.pixels(),original.pixels());assert!(noise.apply_linear(&mut identity,0,&mut||anyhow::bail!("cancelled")).is_err());
 }
}
