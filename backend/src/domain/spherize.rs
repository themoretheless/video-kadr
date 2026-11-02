//! Active Filmcraft Spherize CPU warp. MIT attribution docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};use serde::{Serialize,Deserialize};use super::linear_color_frame::LinearColorFrame;
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Spherize{pub radius:f32,pub center:Option<[Option<f64>;2]>}
impl Default for Spherize{fn default()->Self{Self{radius:0.,center:None}}}
impl Spherize{
 pub fn validate(&self)->Result<()>{ensure!(self.radius.is_finite()&&(0.0..=2500.).contains(&self.radius),"Spherize radius out of range");if let Some(center)=self.center{ensure!(center.into_iter().flatten().all(f64::is_finite),"nonfinite Spherize center");}Ok(())}
 pub fn apply_linear(&self,frame:&mut LinearColorFrame,px_scale:f32,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{self.validate()?;ensure!(px_scale.is_finite()&&px_scale>0.,"invalid Spherize pixel scale");checkpoint()?;let r=(self.radius*px_scale)as f64;if r<0.5{return Ok(());}let(w,h)=frame.dimensions();let center=self.center.unwrap_or([None,None]);let(cx,cy)=(center[0].map(|v|v*px_scale as f64).unwrap_or(w as f64/2.),center[1].map(|v|v*px_scale as f64).unwrap_or(h as f64/2.));let src=frame.pixels().to_vec();let mut index=0;
 frame.map_premultiplied(&mut|_|{let(x,y)=(index%w,index/w);index+=1;let(x,y)=(x as f64+0.5,y as f64+0.5);let(dx,dy)=(x-cx,y-cy);let d=dx.hypot(dy);let(u,v)=if d>=r||d<1e-9{(x,y)}else{let n=d/r;let k=1.-0.5*(1.-n*n).sqrt();(cx+dx*k,cy+dy*k)};super::twirl::sample(&src,w,h,u as f32,v as f32)},checkpoint)
 }
}
#[cfg(test)]mod tests{use super::*;use crate::domain::group_surface::GroupSurface;
#[test]fn default_small_radius_center_and_cancellation(){let mut f=LinearColorFrame::from_surface(&GroupSurface{width:1,height:1,rgba:vec![64,128,192,128]},&mut||Ok(())).unwrap();let before=f.clone();for radius in[0.,0.49,100.]{Spherize{radius,..Default::default()}.apply_linear(&mut f,1.,&mut||Ok(())).unwrap();assert_eq!(f.pixels(),before.pixels());}assert!(Spherize::default().apply_linear(&mut f,1.,&mut||anyhow::bail!("cancelled")).is_err());assert!(Spherize{radius:2500.1,..Default::default()}.validate().is_err());}
}
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum SpherizeChannel{Radius,CenterX,CenterY}
pub type SpherizeAnimation=std::collections::BTreeMap<SpherizeChannel,super::composition::AnimatableValue>;
pub fn validate_animation(a:&SpherizeAnimation)->Result<()>{for(c,v)in a{v.validate()?;let(lo,hi)=match c{SpherizeChannel::Radius=>(0.,2500.),_=>(-f64::MAX,f64::MAX)};ensure!(super::composition::animatable_values_in(v,lo,hi,true),"Spherize animation out of range");}Ok(())}
pub fn sample_animation(mut p:Spherize,a:&SpherizeAnimation,t: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Spherize>{
    let t: super::animation_sample_time::AnimationSampleTime = t.into();
p.validate()?;validate_animation(a)?;for(c,v)in a{let v=super::nested_composition::sample_value(v,t)?;match c{SpherizeChannel::Radius=>p.radius=v as f32,SpherizeChannel::CenterX|SpherizeChannel::CenterY=>{let center=p.center.get_or_insert([None,None]);center[if *c==SpherizeChannel::CenterX{0}else{1}]=Some(v)}}}p.validate()?;Ok(p)}
#[cfg(test)]mod persistence_tests{use super::*;
#[test]fn three_curves_precise_center_auto_axis_and_roundtrip(){let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});let precise=3.123456789012;let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"spherize","parameters":{},"animation":{"radius":curve(0.,10.),"centerX":curve(precise,precise),"centerY":curve(0.,2.)}})).unwrap();assert_eq!(e,serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap());let super::super::composition::VideoEffect::Spherize{parameters,animation:Some(a)}=e else{panic!("Spherize")};let p=sample_animation(parameters,&a,250000).unwrap();assert_eq!(p,Spherize{radius:5.,center:Some([Some(precise),Some(1.)])});let x=SpherizeAnimation::from([(SpherizeChannel::CenterX,a[&SpherizeChannel::CenterX].clone())]);assert_eq!(sample_animation(parameters,&x,250000).unwrap().center,Some([Some(precise),None]));}
}
#[cfg(test)]mod original_tests{use super::*;use crate::domain::group_surface::GroupSurface;
#[test]fn original_working_frame_bits(){
 #[derive(Deserialize)]struct Case{parameters:Spherize,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-spherize.json")).unwrap();assert_eq!(fixture.cases.len(),96);
 for(i,c)in fixture.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();c.parameters.apply_linear(&mut frame,c.scale,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original Spherize {i}");}
}
}
