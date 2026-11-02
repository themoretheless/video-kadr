//! Filmcraft active CPU Paint Bucket, source-resolution connected flood fill.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};use serde::{Serialize,Deserialize};use std::collections::VecDeque;
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct PaintBucket {pub point:[Option<f64>;2],pub selector:u32,pub tolerance:f32,pub invert:bool,pub color:[f32;4],pub opacity:f32,pub mode:u32}
impl Default for PaintBucket {fn default()->Self{Self{point:[None,None],selector:0,tolerance:10.,invert:false,color:[1.,0.,0.,1.],opacity:100.,mode:0}}}
impl PaintBucket {
 pub fn validate(&self)->Result<()> {ensure!(self.point.iter().flatten().all(|v|v.is_finite())&&self.color.iter().all(|v|v.is_finite()),"Paint Bucket coordinate/color out of range");ensure!(self.selector<4&&self.mode<5,"Paint Bucket choice out of range");ensure!(self.tolerance.is_finite()&&(0.0..=255.).contains(&self.tolerance)&&self.opacity.is_finite()&&(0.0..=100.).contains(&self.opacity),"Paint Bucket parameter out of range");Ok(())}
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
 self.validate()?;checkpoint()?;let(w,h)=frame.dimensions();let(sx,sy)=(self.point[0].unwrap_or(w as f64/2.).floor()as isize,self.point[1].unwrap_or(h as f64/2.).floor()as isize);if sx<0||sy<0||sx as usize>=w||sy as usize>=h{return Ok(());}
 let px=|i:usize|{let p=frame.pixels()[i];let rgb=straight(p);let c=rgb.map(super::basic_grade::display);[c[0],c[1],c[2],p[3]]};let start=sy as usize*w+sx as usize;let seed=px(start);let tol=self.tolerance/255.;let similar=|q:[f32;4]|match self.selector{0=>(0..4).all(|c|(q[c]-seed[c]).abs()<=tol),1=>(0..3).all(|c|(q[c]-seed[c]).abs()<=tol),2=>q[3]<=tol.max(1e-3),_=>(q[3]-seed[3]).abs()<=tol};
 let mut fill=vec![0.;w*h];let mut queue=VecDeque::new();if similar(seed){fill[start]=1.;queue.push_back(start);}
 while let Some(i)=queue.pop_front(){checkpoint()?;let(x,y)=(i%w,i/w);let mut visit=|j:usize|{if fill[j]==0.&&similar(px(j)){fill[j]=1.;queue.push_back(j);}};if x>0{visit(i-1);}if x+1<w{visit(i+1);}if y>0{visit(i-w);}if y+1<h{visit(i+w);}}
 if self.invert {fill.iter_mut().for_each(|v|*v=1.-*v);}
 let fill=super::gaussian_blur::blur_plane(&fill,w,h,0.5,0,false,checkpoint)?;let col:[f32;3]=std::array::from_fn(|c|super::basic_grade::linear(self.color[c].clamp(0.,1.)));let op=self.opacity/100.;let mut index=0;
 frame.map_premultiplied(&mut|mut p|{let k=fill[index]*op;index+=1;if k<=0.{return p;}let s=[col[0]*k,col[1]*k,col[2]*k,k];match self.mode {1=>std::array::from_fn(|c|p[c]+s[c]*(1.-p[3])),2..=4=>{let base=straight(p);for c in 0..3{let b=base[c];let v=match self.mode{2=>b+col[c],3=>b*col[c],_=>1.-(1.-b.min(1.))*(1.-col[c].min(1.))};p[c]=(b+(v-b)*k)*p[3];}p},_=>std::array::from_fn(|c|s[c]+p[c]*(1.-k))}},checkpoint)
 }
}
fn straight(p:[f32;4])->[f32;3]{if p[3]<=1e-6{[0.;3]}else{[p[0]/p[3],p[1]/p[3],p[2]/p[3]]}}

#[cfg(test)]mod tests{
 use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn offcanvas_is_identity_and_transparency_selector_keeps_opaque_seed(){
 let original=LinearColorFrame::from_surface(&GroupSurface{width:3,height:3,rgba:vec![255;36]},&mut||Ok(())).unwrap();let mut frame=original.clone();PaintBucket{point:[Some(-0.1),None],..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),original.pixels());PaintBucket{selector:2,tolerance:0.,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),original.pixels());assert!(PaintBucket::default().apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());assert!(PaintBucket{selector:4,..Default::default()}.validate().is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum PaintChannel {PointX,PointY,Tolerance,Opacity,ColorR,ColorG,ColorB,ColorA}
pub type PaintAnimation=std::collections::BTreeMap<PaintChannel,super::composition::AnimatableValue>;
pub fn validate_animation(a:&PaintAnimation)->Result<()>{for(c,v)in a {v.validate()?;let(lo,hi)=match c{PaintChannel::PointX|PaintChannel::PointY=>(-f64::MAX,f64::MAX),PaintChannel::Tolerance=>(0.,255.),PaintChannel::Opacity=>(0.,100.),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(v,lo,hi,true),"Paint Bucket animation out of range");}Ok(())}
pub fn sample_animation(mut p:PaintBucket,a:&PaintAnimation,t: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<PaintBucket>{
    let t: super::animation_sample_time::AnimationSampleTime = t.into();
p.validate()?;validate_animation(a)?;for(c,v)in a {let v=super::nested_composition::sample_value(v,t)?;match c {
PaintChannel::PointX=>p.point[0]=Some(v),
PaintChannel::PointY=>p.point[1]=Some(v),
PaintChannel::Tolerance=>p.tolerance=v as f32,
PaintChannel::Opacity=>p.opacity=v as f32,
PaintChannel::ColorR=>p.color[0]=v as f32,
PaintChannel::ColorG=>p.color[1]=v as f32,
PaintChannel::ColorB=>p.color[2]=v as f32,
PaintChannel::ColorA=>p.color[3]=v as f32,
}}p.validate()?;Ok(p)}

#[cfg(test)]mod animation_tests{
 use super::*;
 #[test]fn eight_channels_preserve_choices_precision_and_ranges(){
 let channels=["pointX","pointY","tolerance","opacity","colorR","colorG","colorB","colorA"];let animation:serde_json::Map<String,serde_json::Value>=channels.iter().enumerate().map(|(i,c)|((*c).into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":i as f64},{"tick":500000,"value":i as f64+2.}]}}))).collect();
 let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"paint_bucket","parameters":{"selector":3,"mode":4,"invert":true},"animation":animation})).unwrap();assert_eq!(e,serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap());let super::super::composition::VideoEffect::PaintBucket{parameters,animation:Some(a)}=e else{panic!("Paint Bucket")};assert_eq!(a.len(),8);let p=sample_animation(parameters,&a,250000).unwrap();assert_eq!(p.point,[Some(1.),Some(2.)]);assert_eq!((p.tolerance,p.opacity),(3.,4.));assert_eq!(p.color,[5.,6.,7.,8.]);assert_eq!((p.selector,p.mode,p.invert),(3,4,true));
 let precise=1.23456789012345;let a:PaintAnimation=serde_json::from_value(serde_json::json!({"pointX":{"mode":"constant","value":precise}})).unwrap();assert_eq!(sample_animation(parameters,&a,0).unwrap().point,[Some(precise),None]);let a:PaintAnimation=serde_json::from_value(serde_json::json!({"tolerance":{"mode":"constant","value":255.1}})).unwrap();assert!(validate_animation(&a).is_err());
 }
}

#[cfg(test)]mod original_tests {use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn original_working_frame_rgba_bits(){
 #[derive(Deserialize)]struct Case{parameters:PaintBucket,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let f:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-paint-bucket.json")).unwrap();assert_eq!(f.cases.len(),2880);
 for(i,c)in f.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();c.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original Paint Bucket {i}");}
 }
}
