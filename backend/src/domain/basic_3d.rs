//! Filmcraft Basic 3D inverse-perspective CPU reference at source resolution.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,Default,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Basic3D {pub swivel:f32,pub tilt:f32,pub distance:f32}
impl Basic3D {
 pub fn validate(&self)->Result<()> {
  ensure!(self.swivel.is_finite()&&self.tilt.is_finite(),"Basic 3D angle out of range");
  ensure!(self.distance.is_finite()&&(-10000.0..=10000.0).contains(&self.distance),"Basic 3D distance out of range");Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;
  let(w,h)=frame.dimensions();let source=frame.pixels().to_vec();let(wf,hf)=(w as f64,h as f64);
  let focal=wf.max(hf)*1.2;
  let(ss,cs)=(self.swivel as f64).to_radians().sin_cos();
  let(st,ct)=(self.tilt as f64).to_radians().sin_cos();
  let n=[ss*ct,-st,cs*ct];let u=[cs,0.0,-ss];let v=[ss*st,ct,cs*st];
  let z0=focal+self.distance as f64*10.0;let mut index=0;
  frame.map_premultiplied(&mut |_| {
   let(x,y)=((index%w)as f64+0.5,(index/w)as f64+0.5);index+=1;
   let dir=[x-wf/2.0,y-hf/2.0,focal];
   let denominator=n[0]*dir[0]+n[1]*dir[1]+n[2]*dir[2];
   if denominator.abs()<1e-9 {return [0.0;4];}
   let t=n[2]*z0/denominator;if t<=0.0 {return [0.0;4];}
   let hit=[dir[0]*t,dir[1]*t,dir[2]*t-z0];
   let su=hit[0]*u[0]+hit[1]*u[1]+hit[2]*u[2];
   let sv=hit[0]*v[0]+hit[1]*v[1]+hit[2]*v[2];
   super::twirl::sample(&source,w,h,(su+wf/2.0)as f32,(sv+hf/2.0)as f32)
  },checkpoint)
 }
}
#[cfg(test)]mod tests {
 use super::*;
 use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case { parameters: Basic3D, width: usize, height: usize, rgba: Vec<u8>, bits: Vec<u32> }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-basic-3d.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(),64);
        for (index, case) in oracle.cases.into_iter().enumerate() {
            let source = GroupSurface {width:case.width,height:case.height,rgba:case.rgba};
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame, &mut || Ok(())).unwrap();
            let bits: Vec<u32> = frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original Basic3D frame {index}");
        }
    }

 #[test]fn persisted_three_curves_local_sampling_and_admission() {
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"basic_3d","parameters":{},"animation":{"swivel":curve(-45.0,45.0),"tilt":curve(-30.0,30.0),"distance":curve(0.0,10.0)}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let crate::domain::composition::VideoEffect::Basic3D{parameters,animation:Some(curves)}=effect else {panic!("Basic 3D")};
  assert_eq!(sample_animation(parameters,&curves,250000).unwrap(),Basic3D{swivel:0.0,tilt:0.0,distance:5.0});
  let invalid:Basic3DAnimation=serde_json::from_value(serde_json::json!({"distance":curve(0.0,10000.1)})).unwrap();assert!(validate_animation(&invalid).is_err());
 }
 #[test]fn identity_edge_on_and_behind_camera_are_transparent() {
  let source=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[i*10,255-i*8,i*7,128]).collect()};
  let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut identity=original.clone();
  Basic3D::default().apply_linear(&mut identity,&mut||Ok(())).unwrap();assert_eq!(identity.pixels(),original.pixels());
  let mut behind=original.clone();Basic3D{distance:-10000.0,..Default::default()}.apply_linear(&mut behind,&mut||Ok(())).unwrap();assert!(behind.pixels().iter().all(|p|*p==[0.0;4]));
  let mut edge=LinearColorFrame::from_surface(&GroupSurface{width:1,height:1,rgba:vec![255,255,255,255]},&mut||Ok(())).unwrap();Basic3D{swivel:90.0,..Default::default()}.apply_linear(&mut edge,&mut||Ok(())).unwrap();assert!(edge.pixels().iter().all(|p|*p==[0.0;4]));
  assert!(Basic3D::default().apply_linear(&mut edge,&mut||anyhow::bail!("cancelled")).is_err());
  assert!(Basic3D{distance:10001.0,..Default::default()}.validate().is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum Basic3DChannel {Swivel,Tilt,Distance}
pub type Basic3DAnimation=std::collections::BTreeMap<Basic3DChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&Basic3DAnimation)->Result<()> {
 for(channel,curve)in animation {curve.validate()?;let(min,max)=if *channel==Basic3DChannel::Distance {(-10000.0,10000.0)}else{(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"Basic 3D animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:Basic3D,animation:&Basic3DAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Basic3D> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;match channel {Basic3DChannel::Swivel=>parameters.swivel=value,Basic3DChannel::Tilt=>parameters.tilt=value,Basic3DChannel::Distance=>parameters.distance=value}}
 parameters.validate()?;Ok(parameters)
}
