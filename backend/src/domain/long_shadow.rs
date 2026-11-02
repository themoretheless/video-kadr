//! Filmcraft active Long Shadow CPU primitive at source resolution.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};use serde::{Serialize,Deserialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct LongShadow {pub angle:f32,pub length:f32,pub color:[f32;4],pub opacity:f32,pub fade:bool,pub only:bool}
impl Default for LongShadow{fn default()->Self{Self{angle:135.,length:100.,color:[0.,0.,0.,1.],opacity:50.,fade:true,only:false}}}
impl LongShadow{
 pub fn validate(&self)->Result<()>{ensure!(self.angle.is_finite()&&self.color.iter().all(|v|v.is_finite())&&self.length.is_finite()&&(0.0..=4000.).contains(&self.length)&&self.opacity.is_finite()&&(0.0..=100.).contains(&self.opacity),"Long Shadow parameter out of range");Ok(())}
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
 self.validate()?;checkpoint()?;let len=self.length;if len<0.5{return Ok(());}let ang=(self.angle as f64).to_radians();let(dx,dy)=(ang.sin()as f32,-ang.cos()as f32);let(w,h)=frame.dimensions();let mut d:Vec<f32>=frame.pixels().iter().map(|p|if p[3]>0.5{0.}else{f32::INFINITY}).collect();let mut s=1f32;
 while s<=len*2.{checkpoint()?;let prev=d.clone();for(y,row)in d.chunks_mut(w).enumerate(){checkpoint()?;for(x,v)in row.iter_mut().enumerate(){let(sx,sy)=((x as f32-dx*s).round(),(y as f32-dy*s).round());if sx>=0.&&sy>=0.&&(sx as usize)<w&&(sy as usize)<h{let q=prev[sy as usize*w+sx as usize]+s;if q<*v{*v=q;}}}}s*=2.;}
 let op=self.opacity/100.;let sh:Vec<f32>=d.iter().map(|&v|if v<=len{(if self.fade{1.-v/len}else{1.})*op}else{0.}).collect();let sh=super::gaussian_blur::blur_plane(&sh,w,h,0.6,0,false,checkpoint)?;let col=std::array::from_fn::<f32,3,_>(|c|super::basic_grade::linear(self.color[c].clamp(0.,1.)));let mut index=0;
 frame.map_premultiplied(&mut|p|{let a=sh[index];index+=1;let o=[col[0]*a,col[1]*a,col[2]*a,a];if self.only{o}else{std::array::from_fn(|c|p[c]+o[c]*(1.-p[3]))}},checkpoint)
 }
}
#[cfg(test)]mod tests{use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
#[test]fn short_shadow_identity_strict_threshold_and_cancellation(){let make=|alpha|LinearColorFrame::from_surface(&GroupSurface{width:3,height:3,rgba:(0..9).flat_map(|i|if i==4{[255,255,255,alpha]}else{[0,0,0,0]}).collect()},&mut||Ok(())).unwrap();let source=make(128);let mut frame=source.clone();LongShadow{length:0.49,only:true,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),source.pixels());let mut low=make(127);LongShadow{only:true,..Default::default()}.apply_linear(&mut low,&mut||Ok(())).unwrap();assert!(low.pixels().iter().all(|p|*p==[0.;4]));assert!(LongShadow::default().apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());assert!(LongShadow{length:4000.1,..Default::default()}.validate().is_err());}
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum LongShadowChannel {Angle,Length,Opacity,ColorR,ColorG,ColorB,ColorA}
pub type LongShadowAnimation=std::collections::BTreeMap<LongShadowChannel,super::composition::AnimatableValue>;
pub fn validate_animation(a:&LongShadowAnimation)->Result<()>{for(c,v)in a{v.validate()?;let(lo,hi)=match c{LongShadowChannel::Length=>(0.,4000.),LongShadowChannel::Opacity=>(0.,100.),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(v,lo,hi,true),"Long Shadow animation out of range");}Ok(())}
pub fn sample_animation(mut p:LongShadow,a:&LongShadowAnimation,t: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<LongShadow>{
    let t: super::animation_sample_time::AnimationSampleTime = t.into();
p.validate()?;validate_animation(a)?;for(c,v)in a{let v=super::nested_composition::sample_value(v,t)? as f32;match c{LongShadowChannel::Angle=>p.angle=v,LongShadowChannel::Length=>p.length=v,LongShadowChannel::Opacity=>p.opacity=v,LongShadowChannel::ColorR=>p.color[0]=v,LongShadowChannel::ColorG=>p.color[1]=v,LongShadowChannel::ColorB=>p.color[2]=v,LongShadowChannel::ColorA=>p.color[3]=v,}}p.validate()?;Ok(p)}
#[cfg(test)]mod persistence_tests{use super::*;
#[test]fn seven_curves_roundtrip_midpoint_and_flags(){let names=["angle","length","opacity","colorR","colorG","colorB","colorA"];let animation:serde_json::Map<String,serde_json::Value>=names.iter().enumerate().map(|(i,n)|((*n).into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":i as f64},{"tick":500000,"value":i as f64+2.}]}}))).collect();let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"long_shadow","parameters":{"fade":false,"only":true},"animation":animation})).unwrap();assert_eq!(e,serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap());let super::super::composition::VideoEffect::LongShadow{parameters,animation:Some(a)}=e else{panic!("LongShadow")};assert_eq!(a.len(),7);let p=sample_animation(parameters,&a,250000).unwrap();assert_eq!((p.angle,p.length,p.opacity,p.fade,p.only),(1.,2.,3.,false,true));assert_eq!(p.color,[4.,5.,6.,7.]);let invalid:LongShadowAnimation=serde_json::from_value(serde_json::json!({"length":{"mode":"constant","value":4000.1}})).unwrap();assert!(validate_animation(&invalid).is_err());}
}

#[cfg(test)]mod original_tests {use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn original_working_frame_rgba_bits(){
 #[derive(Deserialize)]struct Case{parameters:LongShadow,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let f:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-long-shadow.json")).unwrap();assert_eq!(f.cases.len(),512);
 for(i,c)in f.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();c.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original LongShadow {i}");}
 }
}
