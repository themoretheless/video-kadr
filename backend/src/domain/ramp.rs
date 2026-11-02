//! Filmcraft Ramp CPU reference at source-resolution pixel scale.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize,Default)]
#[serde(rename_all="snake_case")]
pub enum RampShape {#[default] Linear,Radial}
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Ramp {
 pub start:[Option<f64>;2],pub end:[Option<f64>;2],
 pub start_color:[f32;4],pub end_color:[f32;4],pub shape:RampShape,pub blend:f32,
}
impl Default for Ramp {fn default()->Self {Self{start:[None,Some(0.0)],end:[None,None],start_color:[0.,0.,0.,1.],end_color:[1.;4],shape:RampShape::Linear,blend:0.0}}}
impl Ramp {
 pub fn validate(&self)->Result<()> {
  ensure!(self.start.iter().chain(self.end.iter()).flatten().all(|v|v.is_finite()),"Ramp point out of range");
  ensure!(self.start_color.iter().chain(self.end_color.iter()).all(|v|v.is_finite()),"Ramp color out of range");
  ensure!(self.blend.is_finite()&&(0.0..=100.0).contains(&self.blend),"Ramp blend out of range");Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;let(w,h)=frame.dimensions();
  let resolve=|p:[Option<f64>;2]|[p[0].unwrap_or(w as f64/2.0),p[1].unwrap_or(h as f64/2.0)];
  let(start,end)=(resolve(self.start),resolve(self.end));let(vx,vy)=(end[0]-start[0],end[1]-start[1]);let length=(vx*vx+vy*vy).max(1e-9);
  let op=1.0-self.blend/100.0;let mut index=0;
  frame.map_premultiplied(&mut|mut p|{
   let(x,y)=((index%w)as f32+0.5,(index/w)as f32+0.5);index+=1;
   let t=match self.shape {RampShape::Radial=>((x as f64-start[0]).hypot(y as f64-start[1])/length.sqrt())as f32,RampShape::Linear=>(((x as f64-start[0])*vx+(y as f64-start[1])*vy)/length)as f32}.clamp(0.0,1.0);
   for c in 0..3 {
    let encoded=(self.start_color[c]+(self.end_color[c]-self.start_color[c])*t).clamp(0.0,1.0);
    let linear=if encoded<=0.04045 {encoded/12.92}else{((encoded+0.055)/1.055).powf(2.4)};
    p[c]=linear*op+p[c]*(1.0-op);
   }
   // Original CPU retains color alpha but uses fill opacity for coverage.
   p[3]=op+p[3]*(1.0-op);p
  },checkpoint)
 }
}
#[cfg(test)]mod tests {
 use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case {parameters:Ramp,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(Deserialize)]
        struct Oracle {commit:String,cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-ramp.json")).unwrap();
        assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),192);
        for(index,case)in oracle.cases.into_iter().enumerate(){
            let source=GroupSurface{width:case.width,height:case.height,rgba:case.rgba};
            let mut frame=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();
            let bits:Vec<u32>=frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original Ramp {index}");
        }
    }
 #[test]fn defaults_axis_auto_radial_alpha_and_bounds(){
  let source=GroupSurface{width:2,height:2,rgba:vec![0;16]};let mut frame=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
  Ramp::default().apply_linear(&mut frame,&mut||Ok(())).unwrap();assert!(frame.pixels().iter().all(|p|p[3]==1.0));assert_eq!(frame.pixels()[2],[1.;4]);
  let original=frame.clone();Ramp{blend:100.,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),original.pixels());
  Ramp{start:[Some(0.),Some(0.)],end:[Some(1.),Some(0.)],shape:RampShape::Radial,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels()[3],[1.;4]);
  assert!(Ramp{blend:100.1,..Default::default()}.validate().is_err());assert!(Ramp::default().apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum RampChannel {StartX,StartY,EndX,EndY,StartColorR,StartColorG,StartColorB,StartColorA,EndColorR,EndColorG,EndColorB,EndColorA,Blend}
pub type RampAnimation=std::collections::BTreeMap<RampChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&RampAnimation)->Result<()> {
 for(channel,curve)in animation {curve.validate()?;let(min,max)=match channel {RampChannel::Blend=>(0.0,100.0),RampChannel::StartX|RampChannel::StartY|RampChannel::EndX|RampChannel::EndY=>(-f64::MAX,f64::MAX),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"Ramp animation out of range");}Ok(())
}
pub fn sample_animation(mut p:Ramp,animation:&RampAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Ramp> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();
p.validate()?;validate_animation(animation)?;for(channel,curve)in animation {let v=super::nested_composition::sample_value(curve,time)?;match channel {
RampChannel::StartX=>p.start[0]=Some(v),
RampChannel::StartY=>p.start[1]=Some(v),
RampChannel::EndX=>p.end[0]=Some(v),
RampChannel::EndY=>p.end[1]=Some(v),
RampChannel::StartColorR=>p.start_color[0]=v as f32,
RampChannel::StartColorG=>p.start_color[1]=v as f32,
RampChannel::StartColorB=>p.start_color[2]=v as f32,
RampChannel::StartColorA=>p.start_color[3]=v as f32,
RampChannel::EndColorR=>p.end_color[0]=v as f32,
RampChannel::EndColorG=>p.end_color[1]=v as f32,
RampChannel::EndColorB=>p.end_color[2]=v as f32,
RampChannel::EndColorA=>p.end_color[3]=v as f32,
RampChannel::Blend=>p.blend=v as f32}}p.validate()?;Ok(p)}
#[cfg(test)]mod animation_tests {
 use super::*;
 #[test]fn persisted_point_precision_auto_axes_and_local_curves(){
  let scalar=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"ramp","parameters":{"shape":"radial","start":[null,0.0],"end":[null,null]},"animation":{"startX":scalar(0.0,5.0),"endY":scalar(1.0,3.0),"startColorR":scalar(0.0,1.0),"endColorA":scalar(0.0,1.0),"blend":scalar(0.0,100.0)}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let super::super::composition::VideoEffect::Ramp{parameters,animation:Some(curves)}=effect else {panic!("Ramp")};
  let p=sample_animation(parameters,&curves,250000).unwrap();assert_eq!(p.start,[Some(2.5),Some(0.0)]);assert_eq!(p.end,[None,Some(2.0)]);assert_eq!(p.start_color[0],0.5);assert_eq!(p.end_color[3],0.5);assert_eq!(p.blend,50.0);assert_eq!(p.shape,RampShape::Radial);
  let precise=1.23456789012345;let curve:RampAnimation=serde_json::from_value(serde_json::json!({"endX":{"mode":"constant","value":precise}})).unwrap();assert_eq!(sample_animation(parameters,&curve,0).unwrap().end,[Some(precise),None]);
  let bad:RampAnimation=serde_json::from_value(serde_json::json!({"blend":scalar(0.0,100.1)})).unwrap();assert!(validate_animation(&bad).is_err());
 }
}
