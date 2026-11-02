//! Filmcraft active Block Dissolve CPU primitive at source resolution.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};use serde::{Serialize,Deserialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct BlockDissolve {pub completion:f32,pub block_w:f32,pub block_h:f32,pub feather:f32,pub soft:bool}
impl Default for BlockDissolve{fn default()->Self{Self{completion:0.,block_w:1.,block_h:1.,feather:0.,soft:true}}}
impl BlockDissolve{
 pub fn validate(&self)->Result<()>{for(v,hi)in[(self.completion,100.),(self.block_w,4000.),(self.block_h,4000.),(self.feather,100.)]{ensure!(v.is_finite()&&(0.0..=hi).contains(&v),"Block Dissolve parameter out of range");}Ok(())}
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{self.validate()?;checkpoint()?;let c=self.completion/100.;if c<=0.{return Ok(());}let bw=self.block_w.max(1.);let bh=self.block_h.max(1.);let fe=self.feather/100.*0.5;let(w,_)=frame.dimensions();let mut index=0;
 frame.map_premultiplied(&mut|p|{let(x,y)=(index%w,index/w);index+=1;let r=hash01((x as f32/bw)as i64,(y as f32/bh)as i64,0,41);let a=if self.soft&&fe>0.{let t=((r-(c-fe))/((c+fe)-(c-fe)).max(1e-6)).clamp(0.,1.);t*t*(3.-2.*t)}else if r>=c{1.}else{0.};if c>=1.{[0.;4]}else if a<1.{p.map(|v|v*a)}else{p}},checkpoint)
 }
}
fn hash01(x: i64, y: i64, z: i64, seed: u64) -> f32 {
    (hash_u(x, y, z, seed) >> 40) as f32 / (1u64 << 24) as f32
}
#[cfg(test)]mod tests{use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
#[test]fn endpoints_and_cancellation(){let source=GroupSurface{width:3,height:3,rgba:vec![128;36]};let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut f=original.clone();BlockDissolve::default().apply_linear(&mut f,&mut||Ok(())).unwrap();assert_eq!(f.pixels(),original.pixels());BlockDissolve{completion:100.,..Default::default()}.apply_linear(&mut f,&mut||Ok(())).unwrap();assert!(f.pixels().iter().all(|p|*p==[0.;4]));assert!(BlockDissolve::default().apply_linear(&mut f,&mut||anyhow::bail!("cancelled")).is_err());assert!(BlockDissolve{completion:100.1,..Default::default()}.validate().is_err());}
}

fn hash_u(x: i64, y: i64, z: i64, seed: u64) -> u64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (z as u64).wrapping_mul(0x1656_67B1_9E37_79F9)
        ^ seed.wrapping_mul(0x27D4_EB2F_1656_67C5);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^ (h >> 32)
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum BlockDissolveChannel {Completion,BlockW,BlockH,Feather}
pub type BlockDissolveAnimation=std::collections::BTreeMap<BlockDissolveChannel,super::composition::AnimatableValue>;
pub fn validate_animation(a:&BlockDissolveAnimation)->Result<()>{for(c,v)in a{v.validate()?;let hi=match c{BlockDissolveChannel::Completion=>100.,BlockDissolveChannel::BlockW=>4000.,BlockDissolveChannel::BlockH=>4000.,BlockDissolveChannel::Feather=>100.,};ensure!(super::composition::animatable_values_in(v,0.,hi,true),"Block Dissolve animation out of range");}Ok(())}
pub fn sample_animation(mut p:BlockDissolve,a:&BlockDissolveAnimation,t: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<BlockDissolve>{
    let t: super::animation_sample_time::AnimationSampleTime = t.into();
p.validate()?;validate_animation(a)?;for(c,v)in a{let v=super::nested_composition::sample_value(v,t)? as f32;match c{BlockDissolveChannel::Completion=>p.completion=v,BlockDissolveChannel::BlockW=>p.block_w=v,BlockDissolveChannel::BlockH=>p.block_h=v,BlockDissolveChannel::Feather=>p.feather=v,}}p.validate()?;Ok(p)}
#[cfg(test)]mod persistence_tests{use super::*;
#[test]fn four_curves_roundtrip_midpoint_and_soft(){let names=["completion","blockW","blockH","feather"];let animation:serde_json::Map<String,serde_json::Value>=names.iter().enumerate().map(|(i,n)|((*n).into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":i as f64},{"tick":500000,"value":i as f64+2.}]}}))).collect();let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"block_dissolve","parameters":{"soft":false},"animation":animation})).unwrap();assert_eq!(e,serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap());let super::super::composition::VideoEffect::BlockDissolve{parameters,animation:Some(a)}=e else{panic!("BlockDissolve")};assert_eq!(a.len(),4);let p=sample_animation(parameters,&a,250000).unwrap();assert_eq!((p.completion,p.block_w,p.block_h,p.feather,p.soft),(1.,2.,3.,4.,false));let invalid:BlockDissolveAnimation=serde_json::from_value(serde_json::json!({"completion":{"mode":"constant","value":100.1}})).unwrap();assert!(validate_animation(&invalid).is_err());}
}

#[cfg(test)]mod original_tests {use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn original_working_frame_rgba_bits(){
 #[derive(Deserialize)]struct Case{parameters:BlockDissolve,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let f:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-block-dissolve.json")).unwrap();assert_eq!(f.cases.len(),128);
 for(i,c)in f.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();c.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original BlockDissolve {i}");}
 }
}
