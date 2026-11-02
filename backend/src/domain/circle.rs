//! Filmcraft Circle CPU reference at source-resolution pixel scale.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Circle {pub center:[Option<f64>;2],pub radius:f32,pub color:[f32;4],pub opacity:f32}
impl Default for Circle {fn default()->Self {Self{center:[None,None],radius:75.0,color:[1.;4],opacity:100.0}}}
impl Circle {
 pub fn validate(&self)->Result<()> {
  ensure!(self.center.iter().flatten().all(|v|v.is_finite())&&self.color.iter().all(|v|v.is_finite()),"Circle center or color out of range");
  ensure!(self.radius.is_finite()&&(0.0..=4000.0).contains(&self.radius),"Circle radius out of range");
  ensure!(self.opacity.is_finite()&&(0.0..=100.0).contains(&self.opacity),"Circle opacity out of range");Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;let(w,h)=frame.dimensions();let(cx,cy)=(self.center[0].unwrap_or(w as f64/2.0),self.center[1].unwrap_or(h as f64/2.0));let op=self.opacity/100.0;
  let color: [f32;3]=std::array::from_fn(|i|{let v=self.color[i].clamp(0.0,1.0);if v<=0.04045 {v/12.92}else{((v+0.055)/1.055).powf(2.4)}});let mut index=0;
  frame.map_premultiplied(&mut|mut p|{
   let(x,y)=((index%w)as f64+0.5,(index/w)as f64+0.5);index+=1;
   let distance=(x-cx).hypot(y-cy)as f32;let alpha=(self.radius-distance+0.5).clamp(0.0,1.0)*op;
   if alpha>0.0 {for c in 0..3 {p[c]=color[c]*alpha+p[c]*(1.0-alpha);}p[3]=alpha+p[3]*(1.0-alpha);}p
  },checkpoint)
 }
}
#[cfg(test)]mod tests {
 use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case {parameters:Circle,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(Deserialize)]
        struct Oracle {commit:String,cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-circle.json")).unwrap();
        assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),160);
        for(index,case)in oracle.cases.into_iter().enumerate(){
            let source=GroupSurface{width:case.width,height:case.height,rgba:case.rgba};
            let mut frame=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();
            let bits:Vec<u32>=frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original Circle {index}");
        }
    }
 #[test]fn zero_radius_half_coverage_color_alpha_and_cancellation(){
  let source=GroupSurface{width:1,height:1,rgba:vec![0;4]};let mut frame=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
  Circle{radius:0.0,color:[1.,0.,0.,0.],..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),&[[0.5,0.,0.,0.5]]);
  let original=frame.clone();Circle{opacity:0.,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),original.pixels());
  Circle{radius:4000.,color:[-1.,2.,0.,-1.],..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),&[[0.,1.,0.,1.]]);
  assert!(Circle{radius:4000.1,..Default::default()}.validate().is_err());assert!(Circle::default().apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum CircleChannel {CenterX,CenterY,Radius,ColorR,ColorG,ColorB,ColorA,Opacity}
pub type CircleAnimation=std::collections::BTreeMap<CircleChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&CircleAnimation)->Result<()> {for(channel,curve)in animation {curve.validate()?;let(min,max)=match channel {CircleChannel::CenterX|CircleChannel::CenterY=>(-f64::MAX,f64::MAX),CircleChannel::Radius=>(0.0,4000.0),CircleChannel::Opacity=>(0.0,100.0),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"Circle animation out of range");}Ok(())}
pub fn sample_animation(mut p:Circle,animation:&CircleAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Circle> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();
p.validate()?;validate_animation(animation)?;for(channel,curve)in animation {let v=super::nested_composition::sample_value(curve,time)?;match channel {CircleChannel::CenterX=>p.center[0]=Some(v),CircleChannel::CenterY=>p.center[1]=Some(v),CircleChannel::Radius=>p.radius=v as f32,CircleChannel::Opacity=>p.opacity=v as f32,
CircleChannel::ColorR=>p.color[0]=v as f32,
CircleChannel::ColorG=>p.color[1]=v as f32,
CircleChannel::ColorB=>p.color[2]=v as f32,
CircleChannel::ColorA=>p.color[3]=v as f32,
}}p.validate()?;Ok(p)}
#[cfg(test)]mod animation_tests {
 use super::*;
 #[test]fn eight_channel_persistence_precision_and_auto_axis(){
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"circle","parameters":{},"animation":{"centerX":curve(0.0,5.0),"centerY":curve(1.0,3.0),"colorR":curve(0.0,1.0),"colorG":curve(1.0,0.0),"colorB":curve(-1.0,1.0),"colorA":curve(0.0,1.0),"radius":curve(0.0,100.0),"opacity":curve(0.0,100.0)}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let super::super::composition::VideoEffect::Circle{parameters,animation:Some(curves)}=effect else {panic!("Circle")};assert_eq!(curves.len(),8);
  let p=sample_animation(parameters,&curves,250000).unwrap();assert_eq!(p.center,[Some(2.5),Some(2.0)]);assert_eq!(p.color,[0.5,0.5,0.0,0.5]);assert_eq!(p.radius,50.0);assert_eq!(p.opacity,50.0);
  let precise=1.23456789012345;let curve:CircleAnimation=serde_json::from_value(serde_json::json!({"centerX":{"mode":"constant","value":precise}})).unwrap();assert_eq!(sample_animation(parameters,&curve,0).unwrap().center,[Some(precise),None]);
  let bad:CircleAnimation=serde_json::from_value(serde_json::json!({"radius":{"mode":"constant","value":4000.1}})).unwrap();assert!(validate_animation(&bad).is_err());
 }
}
