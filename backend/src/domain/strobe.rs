//! Filmcraft Strobe Light CPU reference, with clip-local microsecond clock.
//! MIT source attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Strobe {pub color:[f32;4],pub blend:f32,pub duration:f32,pub period:f32}
impl Default for Strobe {fn default()->Self {Self{color:[1.0;4],blend:0.0,duration:0.05,period:0.5}}}
impl Strobe {
 pub fn validate(&self)->Result<()> {
  ensure!(self.color.iter().all(|v|v.is_finite()),"Strobe color out of range");
  ensure!(self.blend.is_finite()&&(0.0..=100.0).contains(&self.blend),"Strobe blend out of range");
  ensure!([self.duration,self.period].iter().all(|v|v.is_finite()&&(0.0..=30.0).contains(v)),"Strobe timing out of range");Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,local_tick:u64,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
 self.apply_linear_at_seconds(frame,local_tick as f64/1_000_000.0,checkpoint)
 }
 pub fn apply_linear_at_seconds(&self,frame:&mut super::linear_color_frame::LinearColorFrame,seconds:f64,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
 ensure!(seconds.is_finite() && seconds.abs() <= i64::MAX as f64 / 254016000000.0,"invalid signed generator time");
  self.validate()?;checkpoint()?;
  let period=self.period.max(0.001)as f64;let duration=self.duration as f64;
  if seconds%period>=duration {return Ok(());}
  let blend=self.blend/100.0;let color=self.color.map(|v|super::basic_grade::linear(v.clamp(0.0,1.0)));
  frame.map_premultiplied(&mut|pixel|{
   let alpha=pixel[3];if alpha<=1e-6 {return pixel;}
   let mut out=pixel;for k in 0..3 {let original=pixel[k]/alpha;out[k]=(color[k]+(original-color[k])*blend)*alpha;}out
  },checkpoint)
 }
}
#[cfg(test)]mod tests {
 use super::*;
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        #[serde(rename_all="camelCase")]
        struct Case { local_tick:u64, parameters: Strobe, width: usize, height: usize, rgba: Vec<u8>, bits: Vec<u32> }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-strobe.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(),112);
        for (index, case) in oracle.cases.into_iter().enumerate() {
            let source = GroupSurface {width:case.width,height:case.height,rgba:case.rgba};
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame, case.local_tick, &mut || Ok(())).unwrap();
            let bits: Vec<u32> = frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original Strobe frame {index}");
        }
    }

 #[test]fn persisted_timing_blend_and_color_curves() {
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"strobe","parameters":{},"animation":{"colorR":curve(0.0,1.0),"colorA":curve(1.0,0.0),"blend":curve(0.0,100.0),"duration":curve(0.0,0.25),"period":curve(0.0,0.5)}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let crate::domain::composition::VideoEffect::Strobe{parameters,animation:Some(curves)}=effect else {panic!("Strobe")};
  assert_eq!(sample_animation(parameters,&curves,250000).unwrap(),Strobe{color:[0.5,1.0,1.0,0.5],blend:50.0,duration:0.125,period:0.25});
  let bad:StrobeAnimation=serde_json::from_value(serde_json::json!({"period":curve(0.0,30.1)})).unwrap();assert!(validate_animation(&bad).is_err());
 }

 use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn pulse_boundary_zero_period_duration_and_alpha(){
  let source=GroupSurface{width:2,height:1,rgba:vec![64,128,192,128,255,0,0,0]};
  let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut frame=original.clone();
  Strobe{duration:0.125,period:0.5,..Default::default()}.apply_linear(&mut frame,124999,&mut||Ok(())).unwrap();let a=128.0/255.0;assert_eq!(frame.pixels()[0],[a,a,a,a]);assert_eq!(frame.pixels()[1],original.pixels()[1]);
  let mut frame=original.clone();Strobe{duration:0.125,period:0.5,..Default::default()}.apply_linear(&mut frame,125000,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),original.pixels());
  let mut frame=original.clone();Strobe{duration:0.0,period:0.0,..Default::default()}.apply_linear(&mut frame,0,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),original.pixels());
  assert!(Strobe::default().apply_linear(&mut frame,0,&mut||anyhow::bail!("cancelled")).is_err());assert!(Strobe{period:30.1,..Default::default()}.validate().is_err());
 }
}
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum StrobeChannel {ColorR,ColorG,ColorB,ColorA,Blend,Duration,Period}
pub type StrobeAnimation=std::collections::BTreeMap<StrobeChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&StrobeAnimation)->Result<()> {
 for(channel,curve)in animation {curve.validate()?;let(min,max)=match channel {StrobeChannel::Blend=>(0.0,100.0),StrobeChannel::Duration|StrobeChannel::Period=>(0.0,30.0),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"Strobe animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:Strobe,animation:&StrobeAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Strobe> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)?as f32;match channel {StrobeChannel::Blend=>parameters.blend=value,StrobeChannel::Duration=>parameters.duration=value,StrobeChannel::Period=>parameters.period=value,StrobeChannel::ColorR=>parameters.color[0]=value,StrobeChannel::ColorG=>parameters.color[1]=value,StrobeChannel::ColorB=>parameters.color[2]=value,StrobeChannel::ColorA=>parameters.color[3]=value}}
 parameters.validate()?;Ok(parameters)
}
