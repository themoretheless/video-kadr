//! Active Filmcraft Checkerboard CPU reference at source resolution.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Checkerboard {pub anchor:[Option<f64>;2],pub width:f32,pub height:f32,pub square:bool,pub feather:f32,pub color:[f32;4],pub opacity:f32,pub mode:u32}
impl Default for Checkerboard {fn default()->Self {Self{anchor:[None,None],width:64.,height:64.,square:true,feather:0.,color:[1.;4],opacity:100.,mode:0}}}
impl Checkerboard {
 pub fn validate(&self)->Result<()> {
  ensure!(self.anchor.iter().flatten().all(|v|v.is_finite())&&self.color.iter().all(|v|v.is_finite()),"Checkerboard anchor or color out of range");
  ensure!(self.mode<=5,"Checkerboard mode out of range");
  for(value,max)in [(self.width,4000.),(self.height,4000.),(self.feather,100.),(self.opacity,100.)]{ensure!(value.is_finite()&&(0.0..=max).contains(&value),"Checkerboard parameter out of range");}Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;let(w,_h)=frame.dimensions();let(ax,ay)=(self.anchor[0].unwrap_or(0.)as f32,self.anchor[1].unwrap_or(0.)as f32);
  let cw=self.width.max(1.);let ch=if self.square {cw}else{self.height.max(1.)};let op=self.opacity/100.;let mut index=0;
  let color:[f32;3]=std::array::from_fn(|i|super::basic_grade::linear(self.color[i].clamp(0.,1.)));
  frame.map_premultiplied(&mut|p|{
   let(x,y)=((index%w)as f32+0.5,(index/w)as f32+0.5);index+=1;
   let(u,v)=((x-ax)/cw,(y-ay)/ch);let parity=(u.floor()as i64).wrapping_add(v.floor()as i64).rem_euclid(2)as f32;
   let du=(u-u.round()).abs()*cw;let dv=(v-v.round()).abs()*ch;
   let edge=if self.feather>0. {let t=(du.min(dv)/self.feather.max(1e-6)).clamp(0.,1.);t*t*(3.-2.*t)}else{1.};let k=parity*edge*op;
   if self.mode==0 {return [color[0]*k,color[1]*k,color[2]*k,k];}
   if self.mode==1 {return [color[0]*k+p[0]*(1.-k),color[1]*k+p[1]*(1.-k),color[2]*k+p[2]*(1.-k),k+p[3]*(1.-k)];}
   let base:[f32;3]=if p[3]<=1e-6 {[0.;3]}else{[p[0]/p[3],p[1]/p[3],p[2]/p[3]]};
   let mut out=[0.;4];for c in 0..3 {let(b,s)=(base[c],color[c]);let blended=match self.mode {2=>b+s,3=>b*s,4=>1.-(1.-b.min(1.))*(1.-s.min(1.)),_=>if b<0.5 {2.*b*s}else{1.-2.*(1.-b.min(1.))*(1.-s.min(1.))}};out[c]=(b+(blended-b)*k)*p[3];}out[3]=p[3];out
  },checkpoint)
 }
}
#[cfg(test)]mod tests {
 use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case {parameters:Checkerboard,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(Deserialize)]
        struct Oracle {commit:String,cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-checkerboard.json")).unwrap();
        assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),864);
        for(index,case)in oracle.cases.into_iter().enumerate(){
            let source=GroupSurface{width:case.width,height:case.height,rgba:case.rgba};
            let mut frame=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();
            let bits:Vec<u32>=frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original Checkerboard {index}");
        }
    }
 #[test]fn replacement_transparency_normal_zero_width_clamp_and_cancellation(){
  let source=GroupSurface{width:2,height:1,rgba:vec![255;8]};let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut frame=original.clone();
  Checkerboard{width:0.,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),&[[0.;4],[1.;4]]);
  let mut normal=original.clone();Checkerboard{mode:1,opacity:0.,..Default::default()}.apply_linear(&mut normal,&mut||Ok(())).unwrap();assert_eq!(normal.pixels(),original.pixels());
  assert!(Checkerboard{mode:6,..Default::default()}.validate().is_err());assert!(Checkerboard::default().apply_linear(&mut normal,&mut||anyhow::bail!("cancelled")).is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum CheckerChannel {AnchorX,AnchorY,Width,Height,Feather,Opacity,ColorR,ColorG,ColorB,ColorA}
pub type CheckerAnimation=std::collections::BTreeMap<CheckerChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&CheckerAnimation)->Result<()> {for(channel,curve)in animation {curve.validate()?;let(min,max)=match channel {CheckerChannel::AnchorX|CheckerChannel::AnchorY=>(-f64::MAX,f64::MAX),CheckerChannel::Width|CheckerChannel::Height=>(0.0,4000.0),CheckerChannel::Feather|CheckerChannel::Opacity=>(0.0,100.0),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"Checkerboard animation out of range");}Ok(())}
pub fn sample_animation(mut p:Checkerboard,animation:&CheckerAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Checkerboard> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();
p.validate()?;validate_animation(animation)?;for(channel,curve)in animation {let v=super::nested_composition::sample_value(curve,time)?;match channel {CheckerChannel::AnchorX=>p.anchor[0]=Some(v),CheckerChannel::AnchorY=>p.anchor[1]=Some(v),CheckerChannel::Width=>p.width=v as f32,CheckerChannel::Height=>p.height=v as f32,CheckerChannel::Feather=>p.feather=v as f32,CheckerChannel::Opacity=>p.opacity=v as f32,
CheckerChannel::ColorR=>p.color[0]=v as f32,
CheckerChannel::ColorG=>p.color[1]=v as f32,
CheckerChannel::ColorB=>p.color[2]=v as f32,
CheckerChannel::ColorA=>p.color[3]=v as f32,
}}p.validate()?;Ok(p)}
#[cfg(test)]mod animation_tests {
 use super::*;
 #[test]fn ten_channel_persistence_precision_square_and_mode_retention(){
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"checkerboard","parameters":{"square":false,"mode":5},"animation":{"anchorX":curve(0.0,5.0),"anchorY":curve(1.0,3.0),"width":curve(0.0,4000.0),"height":curve(1.0,3.0),"feather":curve(0.0,100.0),"opacity":curve(0.0,100.0),"colorR":curve(0.0,1.0),"colorG":curve(1.0,0.0),"colorB":curve(-1.0,1.0),"colorA":curve(0.0,1.0)}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let super::super::composition::VideoEffect::Checkerboard{parameters,animation:Some(curves)}=effect else {panic!("Checkerboard")};assert_eq!(curves.len(),10);
  let p=sample_animation(parameters,&curves,250000).unwrap();assert_eq!(p.anchor,[Some(2.5),Some(2.0)]);assert_eq!(p.color,[0.5,0.5,0.0,0.5]);assert_eq!(p.width,2000.0);assert_eq!(p.height,2.0);assert_eq!(p.feather,50.0);assert_eq!(p.opacity,50.0);assert!(!p.square);assert_eq!(p.mode,5);
  let precise=1.23456789012345;let curve:CheckerAnimation=serde_json::from_value(serde_json::json!({"anchorX":{"mode":"constant","value":precise}})).unwrap();assert_eq!(sample_animation(parameters,&curve,0).unwrap().anchor,[Some(precise),None]);
  let bad:CheckerAnimation=serde_json::from_value(serde_json::json!({"width":{"mode":"constant","value":4000.1}})).unwrap();assert!(validate_animation(&bad).is_err());
 }
}
