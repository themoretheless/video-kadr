//! Active Filmcraft Magnify (Legacy). MIT notice docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};use serde::{Serialize,Deserialize};use super::linear_color_frame::LinearColorFrame;
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Magnify{pub shape:u32,pub center:Option<[Option<f64>;2]>,pub magnification:f32,pub size:f32,pub feather:f32,pub opacity:f32,pub mode:u32}
impl Default for Magnify{fn default()->Self{Self{shape:0,center:None,magnification:200.,size:100.,feather:0.,opacity:100.,mode:0}}}
impl Magnify{
 pub fn validate(&self)->Result<()>{ensure!(self.shape<2&&self.mode<5,"invalid Magnify choice");for(v,lo,hi)in[(self.magnification,100.,600.),(self.size,0.,4000.),(self.feather,0.,1000.),(self.opacity,0.,100.)]{ensure!(v.is_finite()&&(lo..=hi).contains(&v),"Magnify parameter out of range");}if let Some(center)=self.center{ensure!(center.into_iter().flatten().all(f64::is_finite),"nonfinite Magnify center");}Ok(())}
 pub fn apply_linear(&self,frame:&mut LinearColorFrame,px_scale:f32,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{self.validate()?;ensure!(px_scale.is_finite()&&px_scale>0.,"invalid Magnify pixel scale");checkpoint()?;let(w,h)=frame.dimensions();let center=self.center.unwrap_or([None,None]);let(cx,cy)=(center[0].map(|v|v*px_scale as f64).unwrap_or(w as f64/2.),center[1].map(|v|v*px_scale as f64).unwrap_or(h as f64/2.));let m=(self.magnification/100.).max(0.01) as f64;let r=self.size*px_scale;let fe=self.feather*px_scale;let op=self.opacity/100.;let src=frame.pixels().to_vec();let mut index=0;
 frame.map_premultiplied(&mut|p|{let(x,y)=(index%w,index/w);index+=1;let(dx,dy)=(x as f64+0.5-cx,y as f64+0.5-cy);let d=if self.shape==1{dx.abs().max(dy.abs())}else{dx.hypot(dy)}as f32;let k=if fe>0.{((r-d)/fe).clamp(0.,1.)}else{(r-d+0.5).clamp(0.,1.)}*op;if k<=0.{return p;}let s=super::twirl::sample(&src,w,h,(cx+dx/m)as f32,(cy+dy/m)as f32);let unpremul=|p:[f32;4]|if p[3]<=1e-6{[0.;3]}else{[p[0]/p[3],p[1]/p[3],p[2]/p[3]]};let(base,color)=(unpremul(p),unpremul(s));let sample_alpha=s[3];let mut out=[0.;4];for i in 0..3{let(b,s)=(base[i],color[i]);let mixed=match self.mode{1=>b+s,2=>1.-(1.-b.min(1.))*(1.-s.min(1.)),3=>b*s,4=>if b<0.5{2.*b*s}else{1.-2.*(1.-b.min(1.))*(1.-s.min(1.))},_=>s};out[i]=mixed*sample_alpha;}// alpha comes from the sampled source, not its straight channels
 out[3]=s[3];std::array::from_fn(|i|p[i]+(out[i]-p[i])*k)},checkpoint)
 }
}
#[cfg(test)]mod tests{use super::*;use crate::domain::group_surface::GroupSurface;
#[test]fn identity_zero_opacity_and_cancellation(){let mut f=LinearColorFrame::from_surface(&GroupSurface{width:2,height:2,rgba:vec![128;16]},&mut||Ok(())).unwrap();let before=f.clone();Magnify{magnification:100.,..Default::default()}.apply_linear(&mut f,1.,&mut||Ok(())).unwrap();assert_eq!(f.pixels(),before.pixels());Magnify{opacity:0.,mode:1,..Default::default()}.apply_linear(&mut f,1.,&mut||Ok(())).unwrap();assert_eq!(f.pixels(),before.pixels());assert!(Magnify::default().apply_linear(&mut f,1.,&mut||anyhow::bail!("cancelled")).is_err());assert!(Magnify{mode:5,..Default::default()}.validate().is_err());}
}
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum MagnifyChannel{CenterX,CenterY,Magnification,Size,Feather,Opacity}
pub type MagnifyAnimation=std::collections::BTreeMap<MagnifyChannel,super::composition::AnimatableValue>;
pub fn validate_animation(a:&MagnifyAnimation)->Result<()>{for(c,v)in a{v.validate()?;let(lo,hi)=match c{MagnifyChannel::CenterX|MagnifyChannel::CenterY=>(-f64::MAX,f64::MAX),MagnifyChannel::Magnification=>(100.,600.),MagnifyChannel::Size=>(0.,4000.),MagnifyChannel::Feather=>(0.,1000.),MagnifyChannel::Opacity=>(0.,100.)};ensure!(super::composition::animatable_values_in(v,lo,hi,true),"Magnify animation out of range");}Ok(())}
pub fn sample_animation(mut p:Magnify,a:&MagnifyAnimation,t: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Magnify>{
    let t: super::animation_sample_time::AnimationSampleTime = t.into();
p.validate()?;validate_animation(a)?;for(c,v)in a{let v=super::nested_composition::sample_value(v,t)?;match c{MagnifyChannel::CenterX|MagnifyChannel::CenterY=>{let center=p.center.get_or_insert([None,None]);center[if *c==MagnifyChannel::CenterX{0}else{1}]=Some(v)},MagnifyChannel::Magnification=>p.magnification=v as f32,MagnifyChannel::Size=>p.size=v as f32,MagnifyChannel::Feather=>p.feather=v as f32,MagnifyChannel::Opacity=>p.opacity=v as f32}}p.validate()?;Ok(p)}
#[cfg(test)]mod persistence_tests{use super::*;
#[test]fn six_curves_roundtrip_precise_center_and_auto_axis(){let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});let precise=3.123456789012;let animation=serde_json::json!({"centerX":curve(precise,precise),"centerY":curve(0.,2.),"magnification":curve(100.,200.),"size":curve(0.,2.),"feather":curve(0.,2.),"opacity":curve(0.,100.)});let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"magnify","parameters":{"shape":1,"mode":4},"animation":animation})).unwrap();assert_eq!(e,serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap());let super::super::composition::VideoEffect::Magnify{parameters,animation:Some(a)}=e else{panic!("Magnify")};let p=sample_animation(parameters,&a,250000).unwrap();assert_eq!(p.center,Some([Some(precise),Some(1.)]));assert_eq!((p.magnification,p.size,p.feather,p.opacity,p.shape,p.mode),(150.,1.,1.,50.,1,4));let only_x=MagnifyAnimation::from([(MagnifyChannel::CenterX,a[&MagnifyChannel::CenterX].clone())]);assert_eq!(sample_animation(Magnify::default(),&only_x,250000).unwrap().center,Some([Some(precise),None]));}
}
#[cfg(test)]mod original_tests{use super::*;use crate::domain::group_surface::GroupSurface;
#[test]fn original_working_frame_bits(){
 #[derive(Deserialize)]struct Case{parameters:Magnify,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-magnify.json")).unwrap();assert_eq!(fixture.cases.len(),640);
 for(i,c)in fixture.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();c.parameters.apply_linear(&mut frame,c.scale,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original Magnify {i}");}
}
}
