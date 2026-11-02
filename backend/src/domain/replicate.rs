//! Filmcraft Replicate CPU reference. MIT: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Replicate {pub count:f32}
impl Default for Replicate {fn default()->Self {Self{count:2.0}}}
impl Replicate {
 pub fn validate(&self)->Result<()> {ensure!(self.count.is_finite()&&(2.0..=16.0).contains(&self.count),"Replicate count out of range");Ok(())}
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;let(w,h)=frame.dimensions();let src=frame.pixels().to_vec();let n=self.count.clamp(1.0,16.0)as usize;let mut index=0;
  frame.map_premultiplied(&mut |_|{let x=index%w;let y=index/w;index+=1;let u=(x*n)%w;let v=(y*n)%h;super::mosaic::sample(&src,w,h,(u as f32+0.5)+(n as f32-1.0)*0.5/n as f32,v as f32+0.5)},checkpoint)
 }
}
pub fn validate_animation(animation:&super::composition::AnimatableValue)->Result<()> {animation.validate()?;ensure!(super::composition::animatable_values_in(animation,2.0,16.0,true),"Replicate animation out of range");Ok(())}
pub fn sample_animation(mut parameters:Replicate,animation:&super::composition::AnimatableValue,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Replicate> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();
parameters.validate()?;validate_animation(animation)?;parameters.count=super::nested_composition::sample_value(animation,time)?as f32;parameters.validate()?;Ok(parameters)}
#[cfg(test)]mod tests {
 use super::*;
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case { parameters: Replicate, width: usize, height: usize, rgba: Vec<u8>, bits: Vec<u32> }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-replicate.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(),24);
        for (index, case) in oracle.cases.into_iter().enumerate() {
            let source = GroupSurface {width:case.width,height:case.height,rgba:case.rgba};
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame, &mut || Ok(())).unwrap();
            let bits: Vec<u32> = frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original Replicate frame {index}");
        }
    }
    #[test]
    fn persisted_count_curve_uses_local_clock_and_checks_range() {
        let effect: crate::domain::composition::VideoEffect = serde_json::from_value(serde_json::json!({
            "kind":"replicate", "parameters":{"count":2}, "animation":{
                "mode":"keyframes", "track":{"timeBase":1000000,"interpolation":"linear",
                "keyframes":[{"tick":0,"value":2},{"tick":500000,"value":8}]}}
        })).unwrap();
        let restored = serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
        assert_eq!(effect, restored);
        let crate::domain::composition::VideoEffect::Replicate {parameters, animation:Some(curve)} = effect else {panic!("Replicate")};
        assert_eq!(sample_animation(parameters, &curve, 250000).unwrap().count, 5.0);
        assert!(Replicate {count:16.1}.validate().is_err());
    }

 use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn fractional_count_truncation_tile_period_and_cancellation(){
  let source=GroupSurface{width:4,height:2,rgba:(0..8).flat_map(|i|[i*30,255-i*20,i*20,128]).collect()};
  let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut two=original.clone();let mut fractional=original.clone();
  Replicate{count:2.0}.apply_linear(&mut two,&mut||Ok(())).unwrap();Replicate{count:2.99}.apply_linear(&mut fractional,&mut||Ok(())).unwrap();assert_eq!(two.pixels(),fractional.pixels());
  assert_eq!(two.pixels()[0],two.pixels()[2]);assert_eq!(two.pixels()[0],two.pixels()[4]);
  assert!(Replicate::default().apply_linear(&mut two,&mut||anyhow::bail!("cancelled")).is_err());assert!(Replicate{count:1.0}.validate().is_err());
 }
}
