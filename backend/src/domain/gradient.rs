//! Filmcraft active CPU Gradient generator at source resolution.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};use serde::{Serialize,Deserialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Gradient {pub start:[Option<f64>;2],pub end:[Option<f64>;2],pub start_color:[f32;4],pub end_color:[f32;4],pub shape:u32,pub scatter:f32,pub midpoint:f32,pub blend:f32}
impl Default for Gradient {fn default()->Self{Self{start:[None,None],end:[None,None],start_color:[0.,0.,0.,1.],end_color:[1.;4],shape:0,scatter:0.,midpoint:50.,blend:0.}}}
impl Gradient {
 pub fn validate(&self)->Result<()> {ensure!(self.start.iter().chain(self.end.iter()).flatten().all(|v|v.is_finite())&&self.start_color.iter().chain(self.end_color.iter()).all(|v|v.is_finite()),"Gradient coordinate/color out of range");ensure!(self.shape<4,"Gradient shape out of range");for v in[self.scatter,self.midpoint,self.blend]{ensure!(v.is_finite()&&(0.0..=100.).contains(&v),"Gradient parameter out of range");}Ok(())}
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
 self.validate()?;checkpoint()?;let(w,h)=frame.dimensions();let s=[self.start[0].unwrap_or(w as f64/2.),self.start[1].unwrap_or(0.)];let en=[self.end[0].unwrap_or(w as f64/2.),self.end[1].unwrap_or(h as f64)];let(sx,sy)=(s[0]as f32,s[1]as f32);let(vx,vy)=((en[0]-s[0])as f32,(en[1]-s[1])as f32);let l2=(vx*vx+vy*vy).max(1e-6);let l=l2.sqrt();let sc=self.start_color;let ec=self.end_color;let shape=self.shape;let scatter=self.scatter/100.;let mid=(self.midpoint/100.).clamp(0.01,0.99);let gamma=(0.5f32).ln()/mid.ln();let blend=self.blend/100.;
    let mut index=0;frame.map_premultiplied(&mut |p| {let(x,y)=((index%w)as f32+0.5,(index/w)as f32+0.5);index+=1;
        let (dx, dy) = (x - sx, y - sy);
        let along = (dx * vx + dy * vy) / l2;
        let across = (-dx * vy + dy * vx) / l2;
        let mut t = match shape {
            1 => (dx * dx + dy * dy).sqrt() / l,
            2 => along.abs(),
            3 => along.abs() + across.abs(),
            _ => along,
        };
        if scatter > 0.0 {
            t += (hash01(x as i64, y as i64, 0, 13) - 0.5) * scatter * 0.1;
        }
        let t = t.clamp(0.0, 1.0).powf(gamma);
        let c = std::array::from_fn::<f32,3,_>(|k|super::basic_grade::linear((sc[k]+(ec[k]-sc[k])*t).clamp(0.,1.)));
        let a = sc[3] + (ec[3] - sc[3]) * t;
        let g = [c[0] * a, c[1] * a, c[2] * a, a];
        [0, 1, 2, 3].map(|k| g[k] + (p[k] - g[k]) * blend)
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

#[cfg(test)]mod tests{
 use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn automatic_top_bottom_and_color_alpha_are_used(){
 let original=LinearColorFrame::from_surface(&GroupSurface{width:1,height:2,rgba:vec![0;8]},&mut||Ok(())).unwrap();let mut frame=original.clone();Gradient{start_color:[1.,1.,1.,0.],end_color:[1.,1.,1.,1.],..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),&[[0.25;4],[0.75;4]]);
 let mut a=original.clone();let mut b=original.clone();Gradient{midpoint:0.,..Default::default()}.apply_linear(&mut a,&mut||Ok(())).unwrap();Gradient{midpoint:1.,..Default::default()}.apply_linear(&mut b,&mut||Ok(())).unwrap();assert_eq!(a.pixels(),b.pixels());assert!(Gradient::default().apply_linear(&mut b,&mut||anyhow::bail!("cancelled")).is_err());assert!(Gradient{shape:4,..Default::default()}.validate().is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum GradientChannel {StartX,StartY,EndX,EndY,StartColorR,StartColorG,StartColorB,StartColorA,EndColorR,EndColorG,EndColorB,EndColorA,Scatter,Midpoint,Blend}
pub type GradientAnimation=std::collections::BTreeMap<GradientChannel,super::composition::AnimatableValue>;
pub fn validate_animation(a:&GradientAnimation)->Result<()>{for(c,v)in a {v.validate()?;let(lo,hi)=match c{GradientChannel::StartX|GradientChannel::StartY|GradientChannel::EndX|GradientChannel::EndY=>(-f64::MAX,f64::MAX),GradientChannel::Scatter|GradientChannel::Midpoint|GradientChannel::Blend=>(0.,100.),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(v,lo,hi,true),"Gradient animation out of range");}Ok(())}
pub fn sample_animation(mut p:Gradient,a:&GradientAnimation,t: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Gradient>{
    let t: super::animation_sample_time::AnimationSampleTime = t.into();
p.validate()?;validate_animation(a)?;for(c,v)in a {let v=super::nested_composition::sample_value(v,t)?;match c {
GradientChannel::StartX=>p.start[0]=Some(v),
GradientChannel::StartY=>p.start[1]=Some(v),
GradientChannel::EndX=>p.end[0]=Some(v),
GradientChannel::EndY=>p.end[1]=Some(v),
GradientChannel::StartColorR=>p.start_color[0]=v as f32,
GradientChannel::StartColorG=>p.start_color[1]=v as f32,
GradientChannel::StartColorB=>p.start_color[2]=v as f32,
GradientChannel::StartColorA=>p.start_color[3]=v as f32,
GradientChannel::EndColorR=>p.end_color[0]=v as f32,
GradientChannel::EndColorG=>p.end_color[1]=v as f32,
GradientChannel::EndColorB=>p.end_color[2]=v as f32,
GradientChannel::EndColorA=>p.end_color[3]=v as f32,
GradientChannel::Scatter=>p.scatter=v as f32,
GradientChannel::Midpoint=>p.midpoint=v as f32,
GradientChannel::Blend=>p.blend=v as f32,
}}p.validate()?;Ok(p)}

#[cfg(test)]mod animation_tests {
 use super::*;
 #[test]fn fifteen_channels_roundtrip_midpoint_precision_auto_axis_shape(){
 let names=["startX","startY","endX","endY","startColorR","startColorG","startColorB","startColorA","endColorR","endColorG","endColorB","endColorA","scatter","midpoint","blend"];
 let animation:serde_json::Map<String,serde_json::Value>=names.iter().enumerate().map(|(i,c)|((*c).into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":i as f64},{"tick":500000,"value":i as f64+2.}]}}))).collect();
 let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"gradient","parameters":{"shape":3},"animation":animation})).unwrap();assert_eq!(e,serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap());let super::super::composition::VideoEffect::Gradient{parameters,animation:Some(a)}=e else{panic!("Gradient")};assert_eq!(a.len(),15);let p=sample_animation(parameters,&a,250000).unwrap();assert_eq!(p.start,[Some(1.),Some(2.)]);assert_eq!(p.end,[Some(3.),Some(4.)]);assert_eq!(p.start_color,[5.,6.,7.,8.]);assert_eq!(p.end_color,[9.,10.,11.,12.]);assert_eq!((p.scatter,p.midpoint,p.blend,p.shape),(13.,14.,15.,3));
 let precise=1.23456789012345;let a:GradientAnimation=serde_json::from_value(serde_json::json!({"startX":{"mode":"constant","value":precise}})).unwrap();assert_eq!(sample_animation(parameters,&a,0).unwrap().start,[Some(precise),None]);let a:GradientAnimation=serde_json::from_value(serde_json::json!({"midpoint":{"mode":"constant","value":100.1}})).unwrap();assert!(validate_animation(&a).is_err());
 }
}

#[cfg(test)]mod original_tests {use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn original_working_frame_rgba_bits(){
 #[derive(Deserialize)]struct Case{parameters:Gradient,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let f:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-gradient.json")).unwrap();assert_eq!(f.cases.len(),288);
 for(i,c)in f.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();c.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original Gradient {i}");}
 }
}
