//! Active Filmcraft Linear Wipe (Legacy); MIT attribution docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};use serde::{Serialize,Deserialize};use super::linear_color_frame::LinearColorFrame;
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct LinearWipe {pub completion:f32,pub angle:f32,pub feather:f32}
impl Default for LinearWipe {fn default()->Self{Self{completion:0.,angle:90.,feather:0.}}}
impl LinearWipe {
 pub fn validate(&self)->Result<()>{ensure!(self.completion.is_finite()&&(0.0..=100.).contains(&self.completion)&&self.angle.is_finite()&&self.feather.is_finite()&&(0.0..=4000.).contains(&self.feather),"Linear Wipe parameter out of range");Ok(())}
 pub fn apply_linear(&self,frame:&mut LinearColorFrame,px_scale:f32,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{self.validate()?;ensure!(px_scale.is_finite()&&px_scale>0.,"invalid Linear Wipe pixel scale");checkpoint()?;let c=self.completion/100.;if c<=0.{return Ok(());}let angle=self.angle.to_radians();let(dx,dy)=(angle.sin(),-angle.cos());let(w,h)=frame.dimensions();let(wf,hf)=(w as f32,h as f32);let corners=[(0.,0.),(wf,0.),(0.,hf),(wf,hf)].map(|(x,y)|x*dx+y*dy);let lo=corners.iter().copied().fold(f32::INFINITY,f32::min);let hi=corners.iter().copied().fold(f32::NEG_INFINITY,f32::max);let ext=(hi-lo).max(1.);let soft=self.feather*px_scale/ext;let mut index=0;frame.map_premultiplied(&mut|p|{let(x,y)=(index%w,index/w);index+=1;let g=((x as f32+0.5)*dx+(y as f32+0.5)*dy-lo)/ext;let a=if soft<=1e-4{if g>=c{1.}else{0.}}else{let t=c*(1.+soft);((g-t+soft)/soft).clamp(0.,1.)};p.map(|v|v*a)},checkpoint)}
}
#[cfg(test)]mod tests {use super::*;use crate::domain::group_surface::GroupSurface;
#[test]fn completion_endpoints_and_cancellation(){let mut f=LinearColorFrame::from_surface(&GroupSurface{width:4,height:2,rgba:vec![128;32]},&mut||Ok(())).unwrap();let before=f.clone();LinearWipe::default().apply_linear(&mut f,1.,&mut||Ok(())).unwrap();assert_eq!(f.pixels(),before.pixels());LinearWipe{completion:100.,..Default::default()}.apply_linear(&mut f,1.,&mut||Ok(())).unwrap();assert!(f.pixels().iter().all(|p|*p==[0.;4]));assert!(LinearWipe::default().apply_linear(&mut f,1.,&mut||anyhow::bail!("cancelled")).is_err());}
}
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum LinearWipeChannel{Completion,Angle,Feather}
pub type LinearWipeAnimation=std::collections::BTreeMap<LinearWipeChannel,super::composition::AnimatableValue>;
pub fn validate_animation(a:&LinearWipeAnimation)->Result<()>{for(c,v)in a{v.validate()?;let(lo,hi)=match c{LinearWipeChannel::Completion=>(0.,100.),LinearWipeChannel::Angle=>(-(f32::MAX as f64),f32::MAX as f64),LinearWipeChannel::Feather=>(0.,4000.)};ensure!(super::composition::animatable_values_in(v,lo,hi,true),"Linear Wipe animation out of range");}Ok(())}
pub fn sample_animation(mut p:LinearWipe,a:&LinearWipeAnimation,t: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<LinearWipe>{
    let t: super::animation_sample_time::AnimationSampleTime = t.into();
p.validate()?;validate_animation(a)?;for(c,v)in a{let v=super::nested_composition::sample_value(v,t)? as f32;match c{LinearWipeChannel::Completion=>p.completion=v,LinearWipeChannel::Angle=>p.angle=v,LinearWipeChannel::Feather=>p.feather=v}}p.validate()?;Ok(p)}
#[cfg(test)]mod persistence_tests{use super::*;
#[test]fn three_curves_roundtrip_and_midpoint(){let names=["completion","angle","feather"];let curves:serde_json::Map<String,serde_json::Value>=names.iter().enumerate().map(|(i,name)|(name.to_string(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":i as f64},{"tick":500000,"value":i as f64+2.}]}}))).collect();let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"linear_wipe","parameters":{},"animation":curves})).unwrap();assert_eq!(e,serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap());let super::super::composition::VideoEffect::LinearWipe{parameters,animation:Some(a)}=e else{panic!("LinearWipe")};let p=sample_animation(parameters,&a,250000).unwrap();assert_eq!((p.completion,p.angle,p.feather),(1.,2.,3.));}
}
#[cfg(test)]mod original_tests{use super::*;use crate::domain::group_surface::GroupSurface;
#[test]fn original_working_frame_bits(){
 #[derive(Deserialize)]struct Case{parameters:LinearWipe,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-linear-wipe.json")).unwrap();assert_eq!(fixture.cases.len(),512);
 for(i,c)in fixture.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();c.parameters.apply_linear(&mut frame,c.scale,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original Linear Wipe {i}");}
}
}
