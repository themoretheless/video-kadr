//! Filmcraft active CPU Stroke at source resolution.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};use serde::{Serialize,Deserialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Stroke {pub color:[f32;4],pub width:f32,pub position:u32,pub opacity:f32,pub only:bool}
impl Default for Stroke {fn default()->Self{Self{color:[1.;4],width:4.,position:0,opacity:100.,only:false}}}
impl Stroke {
 pub fn validate(&self)->Result<()> {ensure!(self.color.iter().all(|v|v.is_finite())&&self.width.is_finite()&&(0.0..=500.).contains(&self.width)&&self.opacity.is_finite()&&(0.0..=100.).contains(&self.opacity)&&self.position<3,"Stroke parameter out of range");Ok(())}
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
 self.validate()?;checkpoint()?;if self.width<=0.{return Ok(());}let(iw,ih)=frame.dimensions();let(w,h)=(iw+2,ih+2);let mut inside=vec![false;w*h];for y in 0..ih{checkpoint()?;for x in 0..iw{inside[(y+1)*w+x+1]=frame.pixels()[y*iw+x][3]>0.5;}}
 let outside:Vec<bool>=inside.iter().map(|b|!b).collect();let d_out=distance_to(&inside,w,h,checkpoint)?;let d_in=distance_to(&outside,w,h,checkpoint)?;let col:[f32;3]=std::array::from_fn(|c|super::basic_grade::linear(self.color[c].clamp(0.,1.)));let op=self.opacity/100.;let mut index=0;
 frame.map_premultiplied(&mut|p|{let(x,y)=(index%iw,index/iw);index+=1;let i=(y+1)*w+x+1;let s=if inside[i]{-(d_in[i]-0.5)}else{d_out[i]-0.5};let(lo,hi)=match self.position{0=>(0.,self.width),1=>(-self.width/2.,self.width/2.),_=>(-self.width,0.)};let k=(s-lo+0.5).clamp(0.,1.).min((hi-s+0.5).clamp(0.,1.))*op;let src=[col[0]*k,col[1]*k,col[2]*k,k];if self.only{src}else if k>0.{std::array::from_fn(|c|src[c]+p[c]*(1.-k))}else{p}},checkpoint)
 }
}
fn edt_1d(f: &[f64], out: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut v = vec![0usize; n];
    let mut z = vec![0f64; n + 1];
    let mut k = 0usize;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    let isect = |q: usize, p: usize| ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * q as f64 - 2.0 * p as f64);
    for q in 1..n {
        let mut s = isect(q, v[k]);
        while s <= z[k] {
            k -= 1;
            s = isect(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        *o = (q as f64 - p as f64).powi(2) + f[p];
    }
}

fn distance_to(inside:&[bool],w:usize,h:usize,checkpoint:&mut dyn FnMut()->Result<()>)->Result<Vec<f32>> {
 let mut g:Vec<f64>=inside.iter().map(|b|if *b{0.}else{1e12}).collect();for row in g.chunks_mut(w){checkpoint()?;let f=row.to_vec();edt_1d(&f,row);}
 let mut out=vec![0.;w*h];for x in 0..w{checkpoint()?;let f:Vec<f64>=(0..h).map(|y|g[y*w+x]).collect();let mut c=vec![0.;h];edt_1d(&f,&mut c);for y in 0..h{out[y*w+x]=c[y].sqrt()as f32;}}Ok(out)
}

#[cfg(test)]mod tests {
 use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn zero_width_only_is_identity_and_frame_border_is_an_edge(){
 let original=LinearColorFrame::from_surface(&GroupSurface{width:3,height:3,rgba:vec![255;36]},&mut||Ok(())).unwrap();let mut frame=original.clone();Stroke{width:0.,only:true,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),original.pixels());
 Stroke{width:1.,position:2,only:true,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels()[4],[0.;4]);assert_eq!(frame.pixels()[0],[1.;4]);
 assert!(Stroke::default().apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());assert!(Stroke{position:3,..Default::default()}.validate().is_err());
 }
 #[test]fn alpha_threshold_is_strict_and_color_alpha_is_ignored(){
 let make=|alpha|LinearColorFrame::from_surface(&GroupSurface{width:1,height:1,rgba:vec![255,255,255,alpha]},&mut||Ok(())).unwrap();let mut low=make(127);let mut high=make(128);let effect=Stroke{position:2,width:1.,only:true,color:[1.,1.,1.,0.],..Default::default()};effect.apply_linear(&mut low,&mut||Ok(())).unwrap();effect.apply_linear(&mut high,&mut||Ok(())).unwrap();assert_eq!(low.pixels()[0],[0.;4]);assert_eq!(high.pixels()[0],[1.;4]);
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum StrokeChannel {Width,Opacity,ColorR,ColorG,ColorB,ColorA}
pub type StrokeAnimation=std::collections::BTreeMap<StrokeChannel,super::composition::AnimatableValue>;
pub fn validate_animation(a:&StrokeAnimation)->Result<()>{for(c,v)in a {v.validate()?;let(lo,hi)=match c{StrokeChannel::Width=>(0.,500.),StrokeChannel::Opacity=>(0.,100.),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(v,lo,hi,true),"Stroke animation out of range");}Ok(())}
pub fn sample_animation(mut p:Stroke,a:&StrokeAnimation,t: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Stroke>{
    let t: super::animation_sample_time::AnimationSampleTime = t.into();
p.validate()?;validate_animation(a)?;for(c,v)in a {let v=super::nested_composition::sample_value(v,t)?as f32;match c{StrokeChannel::Width=>p.width=v,StrokeChannel::Opacity=>p.opacity=v,StrokeChannel::ColorR=>p.color[0]=v,StrokeChannel::ColorG=>p.color[1]=v,StrokeChannel::ColorB=>p.color[2]=v,StrokeChannel::ColorA=>p.color[3]=v}}p.validate()?;Ok(p)}
#[cfg(test)]mod animation_tests {
 use super::*;
 #[test]fn six_channels_roundtrip_local_midpoint_choices_and_ranges(){
 let names=["width","opacity","colorR","colorG","colorB","colorA"];let animation:serde_json::Map<String,serde_json::Value>=names.iter().enumerate().map(|(i,c)|((*c).into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":i as f64},{"tick":500000,"value":i as f64+2.}]}}))).collect();let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"stroke","parameters":{"position":2,"only":true},"animation":animation})).unwrap();assert_eq!(e,serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap());let super::super::composition::VideoEffect::Stroke{parameters,animation:Some(a)}=e else{panic!("Stroke")};assert_eq!(a.len(),6);let p=sample_animation(parameters,&a,250000).unwrap();assert_eq!((p.width,p.opacity,p.position,p.only),(1.,2.,2,true));assert_eq!(p.color,[3.,4.,5.,6.]);let a:StrokeAnimation=serde_json::from_value(serde_json::json!({"width":{"mode":"constant","value":500.1}})).unwrap();assert!(validate_animation(&a).is_err());
 }
}

#[cfg(test)]mod original_tests {use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn original_working_frame_rgba_bits(){
 #[derive(Deserialize)]struct Case{parameters:Stroke,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let f:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-stroke.json")).unwrap();assert_eq!(f.cases.len(),192);
 for(i,c)in f.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();c.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original Stroke {i}");}
 }
}
