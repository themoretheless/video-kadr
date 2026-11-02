//! Filmcraft Grid CPU reference at source-resolution pixel scale.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Grid {pub size:f32,pub border:f32,pub color:[f32;4],pub opacity:f32}
impl Default for Grid {fn default()->Self {Self{size:60.0,border:2.0,color:[1.;4],opacity:100.0}}}
impl Grid {
 pub fn validate(&self)->Result<()> {
  ensure!(self.size.is_finite()&&(1.0..=4000.0).contains(&self.size),"Grid width out of range");
  ensure!(self.border.is_finite()&&(0.0..=100.0).contains(&self.border),"Grid border out of range");
  ensure!(self.color.iter().all(|v|v.is_finite()),"Grid color out of range");
  ensure!(self.opacity.is_finite()&&(0.0..=100.0).contains(&self.opacity),"Grid opacity out of range");Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;let(w,h)=frame.dimensions();let(ox,oy)=(w as f32/2.0,h as f32/2.0);let size=self.size.max(1.0);let op=self.opacity/100.0;
  let color:[f32;3]=std::array::from_fn(|i|{let v=self.color[i].clamp(0.0,1.0);if v<=0.04045 {v/12.92}else{((v+0.055)/1.055).powf(2.4)}});let mut index=0;
  frame.map_premultiplied(&mut|mut p|{
   // The original Grid samples integer coordinates, unlike Circle's pixel centres.
   let(x,y)=((index%w)as f32,(index/w)as f32);index+=1;
   let gx=(x-ox).rem_euclid(size).min(size-(x-ox).rem_euclid(size));let gy=(y-oy).rem_euclid(size).min(size-(y-oy).rem_euclid(size));
   let alpha=(self.border*0.5-gx.min(gy)+0.5).clamp(0.0,1.0)*op;
   if alpha>0.0 {for c in 0..3 {p[c]=color[c]*alpha+p[c]*(1.0-alpha);}p[3]=alpha+p[3]*(1.0-alpha);}p
  },checkpoint)
 }
}
#[cfg(test)]mod tests {
 use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case {parameters:Grid,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(Deserialize)]
        struct Oracle {commit:String,cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-grid.json")).unwrap();
        assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),128);
        for(index,case)in oracle.cases.into_iter().enumerate(){
            let source=GroupSurface{width:case.width,height:case.height,rgba:case.rgba};
            let mut frame=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();
            let bits:Vec<u32>=frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original Grid {index}");
        }
    }
 #[test]fn zero_border_half_coverage_integer_coordinates_and_cancellation(){
  let source=GroupSurface{width:2,height:2,rgba:vec![0;16]};let mut frame=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
  Grid{border:0.0,color:[1.,0.,0.,0.],..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels()[0],[0.;4]);assert_eq!(frame.pixels()[1],[0.5,0.,0.,0.5]);assert_eq!(frame.pixels()[2],[0.5,0.,0.,0.5]);
  let original=frame.clone();Grid{opacity:0.,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),original.pixels());
  assert!(Grid{size:0.9,..Default::default()}.validate().is_err());assert!(Grid{border:100.1,..Default::default()}.validate().is_err());assert!(Grid::default().apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum GridChannel {Size,Border,ColorR,ColorG,ColorB,ColorA,Opacity}
pub type GridAnimation=std::collections::BTreeMap<GridChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&GridAnimation)->Result<()> {for(channel,curve)in animation {curve.validate()?;let(min,max)=match channel {GridChannel::Size=>(1.0,4000.0),GridChannel::Border|GridChannel::Opacity=>(0.0,100.0),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"Grid animation out of range");}Ok(())}
pub fn sample_animation(mut p:Grid,animation:&GridAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Grid> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();
p.validate()?;validate_animation(animation)?;for(channel,curve)in animation {let v=super::nested_composition::sample_value(curve,time)? as f32;match channel {GridChannel::Size=>p.size=v,GridChannel::Border=>p.border=v,GridChannel::Opacity=>p.opacity=v,
GridChannel::ColorR=>p.color[0]=v,
GridChannel::ColorG=>p.color[1]=v,
GridChannel::ColorB=>p.color[2]=v,
GridChannel::ColorA=>p.color[3]=v,
}}p.validate()?;Ok(p)}
#[cfg(test)]mod animation_tests {
 use super::*;
 #[test]fn seven_channel_persistence_and_local_midpoint(){
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"grid","parameters":{},"animation":{"size":curve(1.0,4000.0),"border":curve(0.0,100.0),"colorR":curve(0.0,1.0),"colorG":curve(1.0,0.0),"colorB":curve(-1.0,1.0),"colorA":curve(0.0,1.0),"opacity":curve(0.0,100.0)}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let super::super::composition::VideoEffect::Grid{parameters,animation:Some(curves)}=effect else {panic!("Grid")};assert_eq!(curves.len(),7);
  let p=sample_animation(parameters,&curves,250000).unwrap();assert_eq!(p.size,2000.5);assert_eq!(p.border,50.0);assert_eq!(p.color,[0.5,0.5,0.0,0.5]);assert_eq!(p.opacity,50.0);
  let bad:GridAnimation=serde_json::from_value(serde_json::json!({"size":{"mode":"constant","value":0.99}})).unwrap();assert!(validate_animation(&bad).is_err());
 }
}
