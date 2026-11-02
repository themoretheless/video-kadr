//! Filmcraft active Alpha Glow CPU primitive at source resolution.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};use serde::{Serialize,Deserialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct AlphaGlow {pub glow:f32,pub brightness:f32,pub start_color:[f32;4],pub end_color:[f32;4],pub use_end:bool,pub fade_out:bool}
impl Default for AlphaGlow{fn default()->Self{Self{glow:30.,brightness:252.,start_color:[1.;4],end_color:[1.;4],use_end:false,fade_out:true}}}
impl AlphaGlow{
 pub fn validate(&self)->Result<()>{ensure!(self.glow.is_finite()&&(0.0..=100.).contains(&self.glow)&&self.brightness.is_finite()&&(0.0..=255.).contains(&self.brightness)&&self.start_color.iter().chain(self.end_color.iter()).all(|v|v.is_finite()),"Alpha Glow parameter out of range");Ok(())}
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{self.validate()?;checkpoint()?;let r=self.glow*0.5;if r<=0.3{return Ok(());}let(w,h)=frame.dimensions();let a:Vec<f32>=frame.pixels().iter().map(|p|p[3]).collect();let g=super::gaussian_blur::blur_plane(&a,w,h,r,0,false,checkpoint)?;let bright=self.brightness/255.;let sc=std::array::from_fn::<f32,3,_>(|c|super::basic_grade::linear(self.start_color[c].clamp(0.,1.)));let ec=std::array::from_fn::<f32,3,_>(|c|super::basic_grade::linear(self.end_color[c].clamp(0.,1.)));let mut index=0;
 frame.map_premultiplied(&mut|p|{let gv=(g[index]*2.).min(1.);index+=1;let k=if self.fade_out{gv*bright}else if gv>0.02{bright}else{0.};if k<=0.||p[3]>=1.{return p;}let col=if self.use_end{std::array::from_fn(|c|ec[c]+(sc[c]-ec[c])*gv)}else{sc};let under=[col[0]*k,col[1]*k,col[2]*k,k];std::array::from_fn(|c|p[c]+under[c]*(1.-p[3]))},checkpoint)
 }
}
#[cfg(test)]mod tests{use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
#[test]fn small_glow_identity_opaque_skip_and_cancellation(){let source=GroupSurface{width:3,height:3,rgba:vec![255;36]};let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut f=original.clone();AlphaGlow{glow:0.6,..Default::default()}.apply_linear(&mut f,&mut||Ok(())).unwrap();assert_eq!(f.pixels(),original.pixels());AlphaGlow::default().apply_linear(&mut f,&mut||Ok(())).unwrap();assert_eq!(f.pixels(),original.pixels());assert!(AlphaGlow::default().apply_linear(&mut f,&mut||anyhow::bail!("cancelled")).is_err());assert!(AlphaGlow{brightness:255.1,..Default::default()}.validate().is_err());}
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum AlphaGlowChannel {Glow,Brightness,StartColorR,StartColorG,StartColorB,StartColorA,EndColorR,EndColorG,EndColorB,EndColorA}
pub type AlphaGlowAnimation=std::collections::BTreeMap<AlphaGlowChannel,super::composition::AnimatableValue>;
pub fn validate_animation(a:&AlphaGlowAnimation)->Result<()>{for(c,v)in a{v.validate()?;let(lo,hi)=match c{AlphaGlowChannel::Glow=>(0.,100.),AlphaGlowChannel::Brightness=>(0.,255.),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(v,lo,hi,true),"Alpha Glow animation out of range");}Ok(())}
pub fn sample_animation(mut p:AlphaGlow,a:&AlphaGlowAnimation,t: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<AlphaGlow>{
    let t: super::animation_sample_time::AnimationSampleTime = t.into();
p.validate()?;validate_animation(a)?;for(c,v)in a{let v=super::nested_composition::sample_value(v,t)? as f32;match c{AlphaGlowChannel::Glow=>p.glow=v,AlphaGlowChannel::Brightness=>p.brightness=v,AlphaGlowChannel::StartColorR=>p.start_color[0]=v,AlphaGlowChannel::StartColorG=>p.start_color[1]=v,AlphaGlowChannel::StartColorB=>p.start_color[2]=v,AlphaGlowChannel::StartColorA=>p.start_color[3]=v,AlphaGlowChannel::EndColorR=>p.end_color[0]=v,AlphaGlowChannel::EndColorG=>p.end_color[1]=v,AlphaGlowChannel::EndColorB=>p.end_color[2]=v,AlphaGlowChannel::EndColorA=>p.end_color[3]=v,}}p.validate()?;Ok(p)}
#[cfg(test)]mod persistence_tests{use super::*;
#[test]fn ten_curves_roundtrip_midpoint_and_flags(){let names=["glow","brightness","startColorR","startColorG","startColorB","startColorA","endColorR","endColorG","endColorB","endColorA"];let animation:serde_json::Map<String,serde_json::Value>=names.iter().enumerate().map(|(i,n)|((*n).into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":i as f64},{"tick":500000,"value":i as f64+2.}]}}))).collect();let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"alpha_glow","parameters":{"useEnd":true,"fadeOut":false},"animation":animation})).unwrap();assert_eq!(e,serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap());let super::super::composition::VideoEffect::AlphaGlow{parameters,animation:Some(a)}=e else{panic!("AlphaGlow")};assert_eq!(a.len(),10);let p=sample_animation(parameters,&a,250000).unwrap();assert_eq!((p.glow,p.brightness,p.use_end,p.fade_out),(1.,2.,true,false));assert_eq!(p.start_color,[3.,4.,5.,6.]);assert_eq!(p.end_color,[7.,8.,9.,10.]);let invalid:AlphaGlowAnimation=serde_json::from_value(serde_json::json!({"brightness":{"mode":"constant","value":255.1}})).unwrap();assert!(validate_animation(&invalid).is_err());}
}

#[cfg(test)]mod original_tests {use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn original_working_frame_rgba_bits(){
 #[derive(Deserialize)]struct Case{parameters:AlphaGlow,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let f:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-alpha-glow.json")).unwrap();assert_eq!(f.cases.len(),128);
 for(i,c)in f.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();c.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original AlphaGlow {i}");}
 }
}
