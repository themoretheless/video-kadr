//! Filmcraft Brush Strokes active CPU primitive at source resolution.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};use serde::{Serialize,Deserialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct BrushStrokes {pub angle:f32,pub size:f32,pub length:f32,pub density:f32,pub randomness:f32,pub surface:u32,pub blend:f32}
impl Default for BrushStrokes{fn default()->Self{Self{angle:135.,size:2.,length:10.,density:1.,randomness:1.,surface:0,blend:0.}}}
impl BrushStrokes{
 pub fn validate(&self)->Result<()>{ensure!(self.angle.is_finite()&&self.surface<4,"Brush Strokes angle/surface out of range");for(v,hi)in[(self.size,5.),(self.length,40.),(self.density,2.),(self.randomness,2.),(self.blend,100.)]{ensure!(v.is_finite()&&(0.0..=hi).contains(&v),"Brush Strokes parameter out of range");}Ok(())}
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
 self.validate()?;checkpoint()?;let base_ang=self.angle.to_radians();let size=self.size.max(0.);let len=self.length;if len<0.5&&size<0.5{return Ok(());}let density=self.density.max(0.05);let rnd=self.randomness;let surface=self.surface;let blend=self.blend/100.;let cell=(len.max(2.)/density).max(2.);let steps=(len.ceil()as usize).clamp(2,24);let(w,h)=frame.dimensions();let src=frame.pixels().to_vec();let mut index=0;
 frame.map_premultiplied(&mut|mut p|{let(x,y)=(index%w,index/w);index+=1;
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let (cxi, cyi) = ((px / cell).floor() as i64, (py / cell).floor() as i64);
            let jit = (hash01(cxi, cyi, 1, 77) - 0.5) * rnd * std::f32::consts::FRAC_PI_2;
            let a = base_ang + jit;
            let (dx, dy) = (a.cos(), -a.sin());
            let (nx, ny) = (-dy, dx);
            let mut acc = [0.0f32; 4];
            let mut n = 0.0;
            for i in 0..steps {
                let t = (i as f32 / (steps - 1) as f32 - 0.5) * len;
                for s in [-0.5f32, 0.0, 0.5] {
                    let q = super::mosaic::sample(&src,w,h,px + dx * t + nx * s * size, py + dy * t + ny * s * size);
                    for k in 0..4 {
                        acc[k] += q[k];
                    }
                    n += 1.0;
                }
            }
            // bristle texture across the stroke
            let perp = px * nx + py * ny;
            let bristle = 1.0 + 0.1 * noise3(perp / size.max(1.0), (px * dx + py * dy) / len.max(1.0), cxi as f32 * 0.37, 5);
            let mut o = acc.map(|v| v / n);
            for v in &mut o[..3] {
                *v *= bristle;
            }
            let cov = (0.75 + 0.25 * hash01(cxi, cyi, 3, 77)).min(1.0);
            let o = match surface {
                1 => o.map(|v| v * cov),
                2 | 3 => {
                    let bg = if surface == 2 { 1.0 } else { 0.0 };
                    let mut b = [bg, bg, bg, 1.0];
                    over(&mut b, o.map(|v| v * cov));
                    b
                }
                _ => o,
            };
            let orig = p;
            for k in 0..4 {
                p[k] = o[k] + (orig[k] - o[k]) * blend;
            }
p},checkpoint)
 }
}
fn over(dst:&mut [f32;4],src:[f32;4]){for c in 0..4{dst[c]=src[c]+dst[c]*(1.-src[3]);}}
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
fn hash01(x:i64,y:i64,z:i64,seed:u64)->f32{(hash_u(x,y,z,seed)>>40)as f32/(1u64<<24)as f32}
#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}
#[inline]
fn grad3(h: u64, x: f32, y: f32, z: f32) -> f32 {
    // 12 edge directions of a cube (Perlin 2002)
    match h % 12 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x + z,
        5 => -x + z,
        6 => x - z,
        7 => -x - z,
        8 => y + z,
        9 => -y + z,
        10 => y - z,
        _ => -y - z,
    }
}

/// 3-D gradient noise in about −1..1 (our own hash; Perlin's interpolant and gradient set).
fn noise3(x: f32, y: f32, z: f32, seed: u64) -> f32 {
    let (xi, yi, zi) = (x.floor(), y.floor(), z.floor());
    let (xf, yf, zf) = (x - xi, y - yi, z - zi);
    let (xi, yi, zi) = (xi as i64, yi as i64, zi as i64);
    let (u, v, w) = (fade(xf), fade(yf), fade(zf));
    let g = |dx: i64, dy: i64, dz: i64| grad3(hash_u(xi + dx, yi + dy, zi + dz, seed), xf - dx as f32, yf - dy as f32, zf - dz as f32);
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = l(g(0, 0, 0), g(1, 0, 0), u);
    let x10 = l(g(0, 1, 0), g(1, 1, 0), u);
    let x01 = l(g(0, 0, 1), g(1, 0, 1), u);
    let x11 = l(g(0, 1, 1), g(1, 1, 1), u);
    l(l(x00, x10, v), l(x01, x11, v), w) * 0.9
}


#[cfg(test)]mod tests{use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
#[test]fn zero_brush_identity_all_surfaces_transparent_and_cancellation(){let source=GroupSurface{width:3,height:3,rgba:vec![0;36]};let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut f=original.clone();BrushStrokes{size:0.,length:0.,..Default::default()}.apply_linear(&mut f,&mut||Ok(())).unwrap();assert_eq!(f.pixels(),original.pixels());for surface in 0..4{let mut f=original.clone();BrushStrokes{surface,..Default::default()}.apply_linear(&mut f,&mut||Ok(())).unwrap();let expected=match surface{2=>[1.;4],3=>[0.,0.,0.,1.],_=>[0.;4]};assert!(f.pixels().iter().all(|p|*p==expected));}assert!(BrushStrokes::default().apply_linear(&mut f,&mut||anyhow::bail!("cancelled")).is_err());assert!(BrushStrokes{surface:4,..Default::default()}.validate().is_err());}
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum BrushStrokesChannel {Angle,Size,Length,Density,Randomness,Blend}
pub type BrushStrokesAnimation=std::collections::BTreeMap<BrushStrokesChannel,super::composition::AnimatableValue>;
pub fn validate_animation(a:&BrushStrokesAnimation)->Result<()>{for(c,v)in a{v.validate()?;let(lo,hi)=match c{BrushStrokesChannel::Size=>(0.,5.),BrushStrokesChannel::Length=>(0.,40.),BrushStrokesChannel::Density=>(0.,2.),BrushStrokesChannel::Randomness=>(0.,2.),BrushStrokesChannel::Blend=>(0.,100.),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(v,lo,hi,true),"Brush Strokes animation out of range");}Ok(())}
pub fn sample_animation(mut p:BrushStrokes,a:&BrushStrokesAnimation,t: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<BrushStrokes>{
    let t: super::animation_sample_time::AnimationSampleTime = t.into();
p.validate()?;validate_animation(a)?;for(c,v)in a{let v=super::nested_composition::sample_value(v,t)? as f32;match c{BrushStrokesChannel::Angle=>p.angle=v,BrushStrokesChannel::Size=>p.size=v,BrushStrokesChannel::Length=>p.length=v,BrushStrokesChannel::Density=>p.density=v,BrushStrokesChannel::Randomness=>p.randomness=v,BrushStrokesChannel::Blend=>p.blend=v,}}p.validate()?;Ok(p)}
#[cfg(test)]mod persistence_tests{use super::*;
#[test]fn six_curves_roundtrip_midpoint_and_surface(){let values=[("angle",135.),("size",2.),("length",10.),("density",1.),("randomness",1.),("blend",50.)];let animation:serde_json::Map<String,serde_json::Value>=values.into_iter().map(|(n,v)|(n.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":v-0.5},{"tick":500000,"value":v+0.5}]}}))).collect();let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"brush_strokes","parameters":{"surface":3},"animation":animation})).unwrap();assert_eq!(e,serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap());let super::super::composition::VideoEffect::BrushStrokes{parameters,animation:Some(a)}=e else{panic!("BrushStrokes")};assert_eq!(a.len(),6);let p=sample_animation(parameters,&a,250000).unwrap();assert_eq!((p.angle,p.size,p.length,p.density,p.randomness,p.blend,p.surface),(135.,2.,10.,1.,1.,50.,3));let invalid:BrushStrokesAnimation=serde_json::from_value(serde_json::json!({"size":{"mode":"constant","value":5.1}})).unwrap();assert!(validate_animation(&invalid).is_err());}
}

#[cfg(test)]mod original_tests {use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn original_working_frame_rgba_bits(){
 #[derive(Deserialize)]struct Case{parameters:BrushStrokes,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let f:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-brush-strokes.json")).unwrap();assert_eq!(f.cases.len(),192);
 for(i,c)in f.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();c.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original BrushStrokes {i}");}
 }
}
