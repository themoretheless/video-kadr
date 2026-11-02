//! Active Filmcraft vfx Lens Flare CPU reference at source resolution.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct LensFlare {pub center:[Option<f64>;2],pub brightness:f32,pub blend:f32,pub lens_type:u32,pub color:[f32;4],pub size:f32,pub ghosts:f32}
impl Default for LensFlare {fn default()->Self {Self{center:[None,None],brightness:100.,blend:0.,lens_type:0,color:[1.,0.9,0.75,1.],size:100.,ghosts:3.}}}
impl LensFlare {
 pub fn validate(&self)->Result<()> {
  ensure!(self.center.iter().flatten().all(|v|v.is_finite())&&self.color.iter().all(|v|v.is_finite()),"Lens Flare point or color out of range");
  ensure!(self.lens_type<=3,"Lens Flare lens type out of range");
  for(value,min,max)in [(self.brightness,0.,300.),(self.blend,0.,100.),(self.size,10.,400.),(self.ghosts,0.,8.)] {ensure!(value.is_finite()&&(min..=max).contains(&value),"Lens Flare parameter out of range");}Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;let(width,height)=frame.dimensions();
  let center=[self.center[0].unwrap_or(width as f64/2.),self.center[1].unwrap_or(height as f64/2.)];
  let(br,blend,lens)=(self.brightness/100.,self.blend/100.,self.lens_type);
  let tintc:[f32;3]=std::array::from_fn(|i|super::basic_grade::linear(self.color[i].clamp(0.,1.)));
    let size = (self.size / 100.0).max(0.05) as f64;
    let ghosts = self.ghosts.clamp(0.0, 8.0) as usize;
    let (w, h) = (width as f64, height as f64);
    let (mx, my) = (w / 2.0, h / 2.0);
    let scale = w.max(h) * size;
    let core_k = [18.0, 26.0, 14.0, 22.0][lens.min(3) as usize];
    let ghost_list: Vec<(f64, f64, [f32; 3])> = (0..ghosts)
        .map(|i| {
            let t = 0.35 + i as f64 * 0.42 + hash01(i as i64, lens as i64, 0, 7) as f64 * 0.2;
            let r = 0.02 + hash01(i as i64, lens as i64, 1, 7) as f64 * 0.07;
            let hue = [[0.4, 0.6, 1.0], [1.0, 0.5, 0.3], [0.4, 1.0, 0.5], [0.9, 0.4, 1.0]][i % 4];
            (t, r, hue)
        })
        .collect();
    let mut index=0;
    frame.map_premultiplied(&mut |mut p| {
            let (x,y)=(index%width,index/width);index+=1;
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let d = (px - center[0]).hypot(py - center[1]) / scale;
            let core = 0.35 * (-d * core_k).exp() + 0.08 * (-d * 3.0).exp();
            let mut add = [core as f32; 3];
            if lens == 1 || lens == 2 {
                let ring = (1.0 - ((d - 0.12) / 0.012).abs()).max(0.0) * 0.08;
                for (k, v) in add.iter_mut().enumerate() {
                    *v += (ring * [0.6, 0.8, 1.0][k]) as f32;
                }
            }
            if lens == 3 {
                // anamorphic: long horizontal streak
                let s = (-((py - center[1]).abs() / scale) * 160.0).exp() * (-((px - center[0]).abs() / scale) * 2.0).exp() * 0.5;
                add[0] += s as f32 * 0.5;
                add[1] += s as f32 * 0.7;
                add[2] += s as f32;
            }
            for (t, rad, hue) in &ghost_list {
                let gx = center[0] + (mx - center[0]) * 2.0 * t;
                let gy = center[1] + (my - center[1]) * 2.0 * t;
                let dd = (px - gx).hypot(py - gy) / scale;
                let g = ((1.0 - ((dd - rad) / 0.01).abs()).max(0.0) * 0.12 + if dd < *rad { 0.05 } else { 0.0 }) as f32;
                for k in 0..3 {
                    add[k] += hue[k] * g;
                }
            }
            let k = br * (1.0 - blend);
            let light=[add[0]*tintc[0]*k,add[1]*tintc[1]*k,add[2]*tintc[2]*k];
            let coverage=light[0].max(light[1]).max(light[2]).clamp(0.0,1.0);
            for c in 0..3 {p[c]+=light[c];}
            p[3]=p[3]+(1.0-p[3])*coverage;p
    },checkpoint)
 }
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
/// Uniform 0..1 from integer coordinates.
#[inline]
fn hash01(x: i64, y: i64, z: i64, seed: u64) -> f32 {
    (hash_u(x, y, z, seed) >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]mod tests {
 use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case {parameters:LensFlare,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(Deserialize)]
        struct Oracle {commit:String,cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-lens-flare.json")).unwrap();
        assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),512);
        for(index,case)in oracle.cases.into_iter().enumerate(){
            let source=GroupSurface{width:case.width,height:case.height,rgba:case.rgba};
            let mut frame=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();
            let bits:Vec<u32>=frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original Lens Flare {index}");
        }
    }
 #[test]fn four_lenses_additive_transparency_identity_and_cancellation(){
  let source=GroupSurface{width:3,height:2,rgba:vec![0;24]};let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
  for lens_type in 0..4 {
   let mut frame=original.clone();LensFlare{lens_type,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();
   assert!(frame.pixels().iter().any(|p|p[0]>0.&&p[3]>0.));
   let mut unchanged=original.clone();LensFlare{lens_type,brightness:0.,..Default::default()}.apply_linear(&mut unchanged,&mut||Ok(())).unwrap();assert_eq!(unchanged.pixels(),original.pixels());
   LensFlare{lens_type,blend:100.,..Default::default()}.apply_linear(&mut unchanged,&mut||Ok(())).unwrap();assert_eq!(unchanged.pixels(),original.pixels());
  }
  let white=GroupSurface{width:1,height:1,rgba:vec![255;4]};let mut frame=LinearColorFrame::from_surface(&white,&mut||Ok(())).unwrap();
  LensFlare{brightness:300.,color:[1.,1.,1.,0.],ghosts:0.,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert!(frame.pixels()[0][0]>1.);assert_eq!(frame.pixels()[0][3],1.);
  assert!(LensFlare{lens_type:4,..Default::default()}.validate().is_err());assert!(LensFlare::default().apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum FlareChannel {CenterX,CenterY,Brightness,Blend,Size,Ghosts,ColorR,ColorG,ColorB,ColorA}
pub type FlareAnimation=std::collections::BTreeMap<FlareChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&FlareAnimation)->Result<()> {for(channel,curve)in animation {curve.validate()?;let(min,max)=match channel {FlareChannel::CenterX|FlareChannel::CenterY=>(-f64::MAX,f64::MAX),FlareChannel::Brightness=>(0.0,300.0),FlareChannel::Blend=>(0.0,100.0),FlareChannel::Size=>(10.0,400.0),FlareChannel::Ghosts=>(0.0,8.0),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"Lens Flare animation out of range");}Ok(())}
pub fn sample_animation(mut p:LensFlare,animation:&FlareAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<LensFlare> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();
p.validate()?;validate_animation(animation)?;for(channel,curve)in animation {let v=super::nested_composition::sample_value(curve,time)?;match channel {FlareChannel::CenterX=>p.center[0]=Some(v),FlareChannel::CenterY=>p.center[1]=Some(v),FlareChannel::Brightness=>p.brightness=v as f32,FlareChannel::Blend=>p.blend=v as f32,FlareChannel::Size=>p.size=v as f32,FlareChannel::Ghosts=>p.ghosts=v as f32,
FlareChannel::ColorR=>p.color[0]=v as f32,
FlareChannel::ColorG=>p.color[1]=v as f32,
FlareChannel::ColorB=>p.color[2]=v as f32,
FlareChannel::ColorA=>p.color[3]=v as f32,
}}p.validate()?;Ok(p)}
#[cfg(test)]mod animation_tests {
 use super::*;
 #[test]fn ten_channel_persistence_local_midpoint_precision_and_lens_retention(){
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"lens_flare","parameters":{"lensType":3},"animation":{"centerX":curve(0.0,5.0),"centerY":curve(1.0,3.0),"brightness":curve(0.0,300.0),"blend":curve(0.0,100.0),"size":curve(10.0,400.0),"ghosts":curve(0.0,7.0),"colorR":curve(0.0,1.0),"colorG":curve(1.0,0.0),"colorB":curve(-1.0,1.0),"colorA":curve(0.0,1.0)}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let super::super::composition::VideoEffect::LensFlare{parameters,animation:Some(curves)}=effect else {panic!("Lens Flare")};assert_eq!(curves.len(),10);
  let p=sample_animation(parameters,&curves,250000).unwrap();assert_eq!(p.center,[Some(2.5),Some(2.0)]);assert_eq!(p.color,[0.5,0.5,0.0,0.5]);assert_eq!(p.brightness,150.0);assert_eq!(p.blend,50.0);assert_eq!(p.size,205.0);assert_eq!(p.ghosts,3.5);assert_eq!(p.lens_type,3);
  let precise=1.23456789012345;let curve:FlareAnimation=serde_json::from_value(serde_json::json!({"centerX":{"mode":"constant","value":precise}})).unwrap();assert_eq!(sample_animation(parameters,&curve,0).unwrap().center,[Some(precise),None]);
  let bad:FlareAnimation=serde_json::from_value(serde_json::json!({"ghosts":{"mode":"constant","value":8.01}})).unwrap();assert!(validate_animation(&bad).is_err());
 }
}
