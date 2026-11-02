//! Source-resolution Cell Pattern generator from Filmcraft active CPU dispatch.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase")]
pub struct CellPattern { pub pattern:u32,pub invert:bool,pub contrast:f32,pub overflow:u32,pub disperse:f32,pub size:f32,pub offset:[Option<f64>;2],pub evolution:f32,pub seed:f32 }
impl Default for CellPattern {fn default()->Self {Self{pattern:0,invert:false,contrast:100.,overflow:0,disperse:1.,size:60.,offset:[Some(0.),Some(0.)],evolution:0.,seed:0.}}}
impl CellPattern {
 pub fn validate(&self)->Result<()> {
  ensure!(self.pattern<8&&self.overflow<3,"Cell Pattern choice out of range");
  ensure!(self.offset.iter().flatten().all(|v|v.is_finite())&&self.evolution.is_finite(),"Cell Pattern coordinate out of range");
  for(v,max)in[(self.contrast,10000.),(self.disperse,1.5),(self.size,2000.),(self.seed,10000.)]{ensure!(v.is_finite()&&(0.0..=max).contains(&v),"Cell Pattern parameter out of range");}Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;
  let pat=self.pattern;let invert=self.invert;let contrast=self.contrast/100.;let overflow=self.overflow;let disperse=self.disperse.clamp(0.,1.5);let size=self.size.max(1.);let off=Offset{x:self.offset[0].unwrap_or(0.),y:self.offset[1].unwrap_or(0.)};let evo=if pat==3 {0.}else{self.evolution.to_radians()};let seed=self.seed.max(0.)as u64;
    let (w,_) = frame.dimensions(); let mut index=0;
    frame.map_premultiplied(&mut |_| {
        let(x,y)=((index%w)as f32+0.5,(index/w)as f32+0.5); index+=1;
        let (u, v) = ((x + off.x as f32) / size, (y + off.y as f32) / size);
        let (ci, cj) = (u.floor() as i64, v.floor() as i64);
        let (mut f1, mut f2, mut id) = (f32::INFINITY, f32::INFINITY, 0.0f32);
        for dj in -1..=1 {
            for di in -1..=1 {
                let (gi, gj) = (ci + di, cj + dj);
                let ph = hash01(gi, gj, 9, seed) * std::f32::consts::TAU;
                let jx = 0.5 + (hash01(gi, gj, 0, seed) - 0.5) * disperse + (evo + ph).cos() * 0.15 * disperse;
                let jy = 0.5 + (hash01(gi, gj, 1, seed) - 0.5) * disperse + (evo + ph).sin() * 0.15 * disperse;
                let d = ((gi as f32 + jx - u).powi(2) + (gj as f32 + jy - v).powi(2)).sqrt();
                if d < f1 {
                    f2 = f1;
                    f1 = d;
                    id = hash01(gi, gj, 2, seed);
                } else if d < f2 {
                    f2 = d;
                }
            }
        }
        let mut val = match pat {
            0 => (1.0 - f1 * 1.4).max(0.0),
            1 => (f2 - f1) * 2.0,
            2 | 3 => id,
            4 => id * smoothstep(0.0, 0.12, f2 - f1),
            5 => (f2 - f1) / (f2 + f1).max(1e-4) * 2.0,
            6 => ((f2 - f1) * 2.0 + id) * 0.5,
            _ => 1.0 - smoothstep(0.0, 0.12, (f1 - 0.35).abs()),
        };
        val = (val - 0.5) * contrast + 0.5;
        val = match overflow {
            1 => 1.0 / (1.0 + (-(val - 0.5) * 4.0).exp()),
            2 => {
                let m = val.rem_euclid(2.0);
                if m > 1.0 { 2.0 - m } else { m }
            }
            _ => val.clamp(0.0, 1.0),
        };
        if invert {
            val = 1.0 - val;
        }
        let g = super::basic_grade::linear(val);
        [g, g, g, 1.0]
    }, checkpoint)
 }
}
struct Offset{x:f64,y:f64}
fn smoothstep(a:f32,b:f32,x:f32)->f32 {let t=((x-a)/(b-a).max(1e-6)).clamp(0.,1.);t*t*(3.-2.*t)}
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
/// Uniform 0..1 from integer coordinates.
#[inline]
fn hash01(x: i64, y: i64, z: i64, seed: u64) -> f32 {
    (hash_u(x, y, z, seed) >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]mod tests {
 use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn static_plates_ignore_evolution_and_fractional_seed_truncates(){
  let source=GroupSurface{width:7,height:5,rgba:vec![0;7*5*4]};let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
  let mut a=original.clone();let mut b=original.clone();
  CellPattern{pattern:3,seed:2.5,size:2.5,..Default::default()}.apply_linear(&mut a,&mut||Ok(())).unwrap();
  CellPattern{pattern:3,seed:2.,size:2.5,evolution:1234.,..Default::default()}.apply_linear(&mut b,&mut||Ok(())).unwrap();assert_eq!(a.pixels(),b.pixels());
  assert!(a.pixels().iter().all(|p|p[3]==1.&&p[0]==p[1]&&p[1]==p[2]));
  let mut inverse=original.clone();CellPattern{contrast:0.,invert:true,..Default::default()}.apply_linear(&mut inverse,&mut||Ok(())).unwrap();assert!(inverse.pixels().iter().all(|p|p==&[crate::domain::basic_grade::linear(0.5),crate::domain::basic_grade::linear(0.5),crate::domain::basic_grade::linear(0.5),1.]));
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum CellChannel {OffsetX,OffsetY,Contrast,Disperse,Size,Evolution,Seed}
pub type CellAnimation=std::collections::BTreeMap<CellChannel,super::composition::AnimatableValue>;
pub fn validate_animation(a:&CellAnimation)->Result<()> {for(c,v)in a {v.validate()?;let(lo,hi)=match c {CellChannel::OffsetX|CellChannel::OffsetY=>(-f64::MAX,f64::MAX),CellChannel::Evolution=>(-(f32::MAX as f64),f32::MAX as f64),CellChannel::Contrast|CellChannel::Seed=>(0.,10000.),CellChannel::Disperse=>(0.,1.5),CellChannel::Size=>(0.,2000.)};ensure!(super::composition::animatable_values_in(v,lo,hi,true),"Cell Pattern animation out of range");}Ok(())}
pub fn sample_animation(mut p:CellPattern,a:&CellAnimation,t: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<CellPattern>{
    let t: super::animation_sample_time::AnimationSampleTime = t.into();
p.validate()?;validate_animation(a)?;for(c,v)in a {let v=super::nested_composition::sample_value(v,t)?;match c {CellChannel::OffsetX=>p.offset[0]=Some(v),CellChannel::OffsetY=>p.offset[1]=Some(v),CellChannel::Contrast=>p.contrast=v as f32,CellChannel::Disperse=>p.disperse=v as f32,CellChannel::Size=>p.size=v as f32,CellChannel::Evolution=>p.evolution=v as f32,CellChannel::Seed=>p.seed=v as f32}}p.validate()?;Ok(p)}

#[cfg(test)]mod oracle_tests {
 use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn original_working_frame_rgba_bits(){
 #[derive(Deserialize)]struct Case{parameters:CellPattern,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let f:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-cell-pattern.json")).unwrap();assert_eq!(f.cases.len(),1152);
 for(i,c)in f.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();c.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original Cell Pattern {i}");}
 }
 #[test]fn seven_channel_roundtrip_precision_choices_and_ranges(){
 let scalar=|v:f64|serde_json::json!({"mode":"constant","value":v});let precise=1.23456789012345;
 let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"cell_pattern","parameters":{"pattern":7,"overflow":2,"invert":true},"animation":{"offsetX":scalar(precise),"offsetY":scalar(-2.125),"contrast":scalar(10000.),"disperse":scalar(1.5),"size":scalar(0.),"evolution":scalar(-1234.),"seed":scalar(2.5)}})).unwrap();assert_eq!(e,serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap());
 let super::super::composition::VideoEffect::CellPattern{parameters,animation:Some(a)}=e else{panic!("Cell Pattern")};assert_eq!(a.len(),7);let p=sample_animation(parameters,&a,250000).unwrap();assert_eq!(p.offset,[Some(precise),Some(-2.125)]);assert_eq!((p.pattern,p.overflow,p.invert),(7,2,true));assert_eq!((p.contrast,p.disperse,p.size,p.evolution,p.seed),(10000.,1.5,0.,-1234.,2.5));
 let bad:CellAnimation=serde_json::from_value(serde_json::json!({"seed":scalar(10000.1)})).unwrap();assert!(validate_animation(&bad).is_err());
 }
}
