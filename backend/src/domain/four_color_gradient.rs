//! Filmcraft four-color CPU gradient at source resolution.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all="camelCase", deny_unknown_fields)]
pub struct FourColorGradient {
    pub c1: [f32;4], pub c2: [f32;4], pub c3: [f32;4], pub c4: [f32;4],
    pub blend: f32, pub opacity: f32,
}
impl Default for FourColorGradient {
    fn default()->Self {Self {c1:[1.,1.,0.,1.], c2:[0.,1.,0.,1.], c3:[1.,0.,1.,1.], c4:[0.,0.,1.,1.], blend:100., opacity:100.}}
}
impl FourColorGradient {
    pub fn validate(&self)->Result<()> {
        ensure!([self.c1,self.c2,self.c3,self.c4].iter().flatten().all(|v|v.is_finite()),"4-Color Gradient color out of range");
        ensure!(self.blend.is_finite()&&(1.0..=1000.0).contains(&self.blend),"4-Color Gradient blend out of range");
        ensure!(self.opacity.is_finite()&&(0.0..=100.0).contains(&self.opacity),"4-Color Gradient opacity out of range"); Ok(())
    }
    pub fn apply_linear(&self, frame:&mut super::linear_color_frame::LinearColorFrame, checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
        self.validate()?; checkpoint()?;
        let(w,h)=frame.dimensions(); let(wf,hf)=(w as f32,h as f32);
        let points=[(0.25*wf,0.25*hf),(0.75*wf,0.25*hf),(0.25*wf,0.75*hf),(0.75*wf,0.75*hf)];
        let colors=[self.c1,self.c2,self.c3,self.c4]; let exponent=1.0+self.blend/100.0; let op=self.opacity/100.0; let mut index=0;
        frame.map_premultiplied(&mut |mut p| {
            let(x,y)=((index%w)as f32+0.5,(index/w)as f32+0.5);index+=1;
            let mut acc=[0.0f32;3];let mut sum=0.0;
            for(i,point)in points.iter().enumerate(){
                let distance=((x-point.0).powi(2)+(y-point.1).powi(2)).sqrt().max(1.0);
                let weight=1.0/distance.powf(exponent);
                for c in 0..3 {acc[c]+=colors[i][c]*weight;}sum+=weight;
            }
            for c in 0..3 {
                let v=(acc[c]/sum).clamp(0.0,1.0);
                let linear=if v<=0.04045 {v/12.92}else{((v+0.055)/1.055).powf(2.4)};
                p[c]=linear*op+p[c]*(1.0-op);
            }
            // Source CPU ignores individual color alpha; fill opacity controls coverage.
            p[3]=op+p[3]*(1.0-op);p
        },checkpoint)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case {parameters:FourColorGradient,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(Deserialize)]
        struct Oracle {commit:String,cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-four-color-gradient.json")).unwrap();
        assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),96);
        for(index,case)in oracle.cases.into_iter().enumerate(){
            let source=GroupSurface{width:case.width,height:case.height,rgba:case.rgba};
            let mut frame=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();
            let bits:Vec<u32>=frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original four-color gradient {index}");
        }
    }
    #[test]
    fn opacity_alpha_cancellation_and_bounds() {
        let source=GroupSurface{width:2,height:1,rgba:vec![64,128,192,128,0,0,0,0]};
        let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut frame=original.clone();
        FourColorGradient{opacity:0.,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),original.pixels());
        let uniform=FourColorGradient{c1:[1.,0.,0.,0.],c2:[1.,0.,0.,-1.],c3:[1.,0.,0.,2.],c4:[1.,0.,0.,0.],..Default::default()};
        uniform.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert!(frame.pixels().iter().all(|p|*p==[1.,0.,0.,1.]));
        assert!(uniform.apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());
        assert!(FourColorGradient{blend:0.,..Default::default()}.validate().is_err());
        assert!(FourColorGradient{opacity:100.1,..Default::default()}.validate().is_err());
    }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum GradientChannel {C1R,C1G,C1B,C1A,C2R,C2G,C2B,C2A,C3R,C3G,C3B,C3A,C4R,C4G,C4B,C4A,Blend,Opacity}
pub type GradientAnimation=std::collections::BTreeMap<GradientChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&GradientAnimation)->Result<()> {
 for(channel,curve)in animation {curve.validate()?;let(min,max)=match channel {GradientChannel::Blend=>(1.0,1000.0),GradientChannel::Opacity=>(0.0,100.0),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"4-Color Gradient animation out of range");}Ok(())
}
pub fn sample_animation(mut p:FourColorGradient,animation:&GradientAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<FourColorGradient> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 p.validate()?;validate_animation(animation)?;for(channel,curve)in animation {let v=super::nested_composition::sample_value(curve,time)? as f32;match channel {
GradientChannel::C1R=>p.c1[0]=v,
GradientChannel::C1G=>p.c1[1]=v,
GradientChannel::C1B=>p.c1[2]=v,
GradientChannel::C1A=>p.c1[3]=v,
GradientChannel::C2R=>p.c2[0]=v,
GradientChannel::C2G=>p.c2[1]=v,
GradientChannel::C2B=>p.c2[2]=v,
GradientChannel::C2A=>p.c2[3]=v,
GradientChannel::C3R=>p.c3[0]=v,
GradientChannel::C3G=>p.c3[1]=v,
GradientChannel::C3B=>p.c3[2]=v,
GradientChannel::C3A=>p.c3[3]=v,
GradientChannel::C4R=>p.c4[0]=v,
GradientChannel::C4G=>p.c4[1]=v,
GradientChannel::C4B=>p.c4[2]=v,
GradientChannel::C4A=>p.c4[3]=v,
GradientChannel::Blend=>p.blend=v,GradientChannel::Opacity=>p.opacity=v}}p.validate()?;Ok(p)
}
#[cfg(test)] mod animation_tests {
 use super::*;
 #[test] fn eighteen_channel_persistence_and_local_midpoint() {
  let mut animation=serde_json::Map::new();
  for i in 1..=4 {for c in ["R","G","B","A"] {animation.insert(format!("c{i}{c}"),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":500000,"value":1.0}]}}));}}
  for (name,a,b)in [("blend",1.0,1000.0),("opacity",0.0,100.0)] {animation.insert(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}}));}
  let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"four_color_gradient","parameters":{},"animation":animation})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let super::super::composition::VideoEffect::FourColorGradient{parameters,animation:Some(curves)}=effect else {panic!("gradient")};assert_eq!(curves.len(),18);parameters.validate().unwrap();validate_animation(&curves).unwrap();
  let p=sample_animation(parameters,&curves,250000).unwrap();assert_eq!(p.c1,[0.5;4]);assert_eq!(p.c2,[0.5;4]);assert_eq!(p.c3,[0.5;4]);assert_eq!(p.c4,[0.5;4]);assert_eq!(p.blend,500.5);assert_eq!(p.opacity,50.0);
 }
}
