//! Filmcraft active CPU Ellipse generator at source resolution.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Ellipse {pub center:[Option<f64>;2],pub width:f32,pub height:f32,pub thickness:f32,pub softness:f32,pub inside:[f32;4],pub outside:[f32;4],pub composite:bool}
impl Default for Ellipse {fn default()->Self{Self{center:[None,None],width:200.,height:200.,thickness:8.,softness:15.,inside:[1.;4],outside:[0.,0.5,1.,1.],composite:false}}}
impl Ellipse {
 pub fn validate(&self)->Result<()> {
 ensure!(self.center.iter().flatten().all(|v|v.is_finite())&&self.inside.iter().chain(self.outside.iter()).all(|v|v.is_finite()),"Ellipse coordinate or color out of range");
 for(v,max)in[(self.width,4000.),(self.height,4000.),(self.thickness,1000.),(self.softness,100.)]{ensure!(v.is_finite()&&(0.0..=max).contains(&v),"Ellipse parameter out of range");}Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
 self.validate()?;checkpoint()?;let(w,h)=frame.dimensions();let(cx,cy)=(self.center[0].unwrap_or(w as f64/2.)as f32,self.center[1].unwrap_or(h as f64/2.)as f32);
 let(rx,ry)=((self.width/2.).max(0.5),(self.height/2.).max(0.5));let th=self.thickness.max(0.5);let soft=self.softness/100.;let rmin=rx.min(ry);
 let inc:[f32;3]=std::array::from_fn(|c|super::basic_grade::linear(self.inside[c].clamp(0.,1.)));let outc:[f32;3]=std::array::from_fn(|c|super::basic_grade::linear(self.outside[c].clamp(0.,1.)));let mut index=0;
 frame.map_premultiplied(&mut|p|{let(x,y)=((index%w)as f32+0.5,(index/w)as f32+0.5);index+=1;
 let(dx,dy)=((x-cx)/rx,(y-cy)/ry);let d=((dx*dx+dy*dy).sqrt()-1.).abs()*rmin;let half=th/2.;let a=1.-smoothstep(half*(1.-soft)-0.5,half+0.5,d);let inner=1.-smoothstep(0.,half,d);
 let col:[f32;3]=std::array::from_fn(|c|outc[c]+(inc[c]-outc[c])*inner);let s=[col[0]*a,col[1]*a,col[2]*a,a];if self.composite {std::array::from_fn(|c|s[c]+p[c]*(1.-a))}else{s}
 },checkpoint)
 }
}
fn smoothstep(a:f32,b:f32,x:f32)->f32 {let t=((x-a)/(b-a).max(1e-6)).clamp(0.,1.);t*t*(3.-2.*t)}
#[cfg(test)]mod tests {
 use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn replacement_and_composite_preserve_source_clamps_ignore_color_alpha_and_cancel(){
 let src=GroupSurface{width:3,height:3,rgba:vec![255;36]};let original=LinearColorFrame::from_surface(&src,&mut||Ok(())).unwrap();let mut a=original.clone();let p=Ellipse{center:[Some(100.),Some(100.)],width:0.,height:0.,thickness:0.,..Default::default()};p.apply_linear(&mut a,&mut||Ok(())).unwrap();assert!(a.pixels().iter().all(|p|p==&[0.;4]));
 let mut b=original.clone();Ellipse{composite:true,..p}.apply_linear(&mut b,&mut||Ok(())).unwrap();assert_eq!(b.pixels(),original.pixels());
 assert!(p.apply_linear(&mut b,&mut||anyhow::bail!("cancelled")).is_err());assert!(Ellipse{softness:100.1,..p}.validate().is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum EllipseChannel {CenterX,CenterY,Width,Height,Thickness,Softness,InsideR,InsideG,InsideB,InsideA,OutsideR,OutsideG,OutsideB,OutsideA}
pub type EllipseAnimation=std::collections::BTreeMap<EllipseChannel,super::composition::AnimatableValue>;
pub fn validate_animation(a:&EllipseAnimation)->Result<()> {for(c,v)in a {v.validate()?;let(lo,hi)=match c {EllipseChannel::CenterX|EllipseChannel::CenterY=>(-f64::MAX,f64::MAX),EllipseChannel::Width|EllipseChannel::Height=>(0.,4000.),EllipseChannel::Thickness=>(0.,1000.),EllipseChannel::Softness=>(0.,100.),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(v,lo,hi,true),"Ellipse animation out of range");}Ok(())}
pub fn sample_animation(mut p:Ellipse,a:&EllipseAnimation,t: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Ellipse>{
    let t: super::animation_sample_time::AnimationSampleTime = t.into();
p.validate()?;validate_animation(a)?;for(c,v)in a {let v=super::nested_composition::sample_value(v,t)?;match c {
EllipseChannel::CenterX=>p.center[0]=Some(v),
EllipseChannel::CenterY=>p.center[1]=Some(v),
EllipseChannel::Width=>p.width=v as f32,
EllipseChannel::Height=>p.height=v as f32,
EllipseChannel::Thickness=>p.thickness=v as f32,
EllipseChannel::Softness=>p.softness=v as f32,
EllipseChannel::InsideR=>p.inside[0]=v as f32,
EllipseChannel::InsideG=>p.inside[1]=v as f32,
EllipseChannel::InsideB=>p.inside[2]=v as f32,
EllipseChannel::InsideA=>p.inside[3]=v as f32,
EllipseChannel::OutsideR=>p.outside[0]=v as f32,
EllipseChannel::OutsideG=>p.outside[1]=v as f32,
EllipseChannel::OutsideB=>p.outside[2]=v as f32,
EllipseChannel::OutsideA=>p.outside[3]=v as f32,
}}p.validate()?;Ok(p)}

#[cfg(test)]mod animation_tests {
 use super::*;
 #[test]fn fourteen_channels_roundtrip_precision_composite_and_range(){
 let channels=["centerX","centerY","width","height","thickness","softness","insideR","insideG","insideB","insideA","outsideR","outsideG","outsideB","outsideA"];
 let animation:serde_json::Map<String,serde_json::Value>=channels.iter().enumerate().map(|(i,c)|((*c).into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":i as f64},{"tick":500000,"value":i as f64+2.0}]}}))).collect();
 let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"ellipse","parameters":{"composite":true},"animation":animation})).unwrap();assert_eq!(effect,serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap());
 let super::super::composition::VideoEffect::Ellipse{parameters,animation:Some(a)}=effect else{panic!("Ellipse")};assert_eq!(a.len(),14);let p=sample_animation(parameters,&a,250000).unwrap();assert!(p.composite);assert_eq!(p.center,[Some(1.),Some(2.)]);assert_eq!((p.width,p.height,p.thickness,p.softness),(3.,4.,5.,6.));assert_eq!(p.inside,[7.,8.,9.,10.]);assert_eq!(p.outside,[11.,12.,13.,14.]);
 let precise=1.23456789012345;let a:EllipseAnimation=serde_json::from_value(serde_json::json!({"centerX":{"mode":"constant","value":precise}})).unwrap();assert_eq!(sample_animation(parameters,&a,0).unwrap().center,[Some(precise),None]);
 let a:EllipseAnimation=serde_json::from_value(serde_json::json!({"thickness":{"mode":"constant","value":1000.1}})).unwrap();assert!(validate_animation(&a).is_err());
 }
}

#[cfg(test)]mod original_tests {use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn original_working_frame_rgba_bits(){
 #[derive(Deserialize)]struct Case{parameters:Ellipse,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let f:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-ellipse.json")).unwrap();assert_eq!(f.cases.len(),144);
 for(i,c)in f.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();c.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original Ellipse {i}");}
 }
}
