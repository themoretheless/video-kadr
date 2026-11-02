//! Filmcraft Color Balance display-domain shadows/midtones/highlights (MIT pinned renderer).
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,Default,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct ColorBalance {
    pub shadow_r:f32,
    pub shadow_g:f32,
    pub shadow_b:f32,
    pub mid_r:f32,
    pub mid_g:f32,
    pub mid_b:f32,
    pub hi_r:f32,
    pub hi_g:f32,
    pub hi_b:f32,
    pub preserve:bool,
}
impl ColorBalance {
    pub fn validate(&self)->Result<()> {
        ensure!([self.shadow_r,self.shadow_g,self.shadow_b,self.mid_r,self.mid_g,self.mid_b,self.hi_r,self.hi_g,self.hi_b].iter().all(|v|v.is_finite()&&(-100.0..=100.0).contains(v)),"Color Balance channel out of range");Ok(())
    }
    pub fn signal(&self,input:[f32;3])->[f32;3] {
        let encoded=input.map(|v|super::basic_grade::display(v.max(0.0)));
        let l=super::black_white::gray(encoded);
        let ws=(1.0-l).powi(2);let wh=l.powi(2);let wm=(1.0-ws-wh).max(0.0);
        let shadow=[self.shadow_r,self.shadow_g,self.shadow_b].map(|v|v/100.0*0.25);
        let mid=[self.mid_r,self.mid_g,self.mid_b].map(|v|v/100.0*0.25);
        let high=[self.hi_r,self.hi_g,self.hi_b].map(|v|v/100.0*0.25);
        let mut out=std::array::from_fn(|k|encoded[k]+shadow[k]*ws+mid[k]*wm+high[k]*wh);
        if self.preserve {let difference=l-super::black_white::gray(out);out=out.map(|v|v+difference);}
        out
    }
    pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
        self.validate()?;frame.map_linear_to_signal(&mut|input,_,_|Ok(self.signal(input)),checkpoint)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_animation_preserves_other_channels_and_authored_flag() {
        let curve:super::super::composition::AnimatableValue=serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":-100},{"tick":1000000,"value":100}]}})).unwrap();
        let animation=std::collections::BTreeMap::from([(BalanceChannel::ShadowR,curve),(BalanceChannel::HiB,super::super::composition::AnimatableValue::Constant{value:75.0})]);
        let parameters=ColorBalance{mid_g:-50.0,preserve:true,..Default::default()};
        let sampled=sample_animation(parameters,&animation,500000).unwrap();assert_eq!(sampled.shadow_r,0.0);assert_eq!(sampled.hi_b,75.0);assert_eq!(sampled.mid_g,-50.0);assert!(sampled.preserve);
        let restored:BalanceAnimation=serde_json::from_value(serde_json::to_value(&animation).unwrap()).unwrap();assert_eq!(restored,animation);
        let invalid=std::collections::BTreeMap::from([(BalanceChannel::MidG,super::super::composition::AnimatableValue::Constant{value:101.0})]);assert!(validate_animation(&invalid).is_err());
    }
    #[test]
    fn original_cpu_rgba_bits() {
        #[derive(serde::Deserialize)]
        struct Case {parameters:ColorBalance,input:[f32;4],bits:[u32;4]}
        #[derive(serde::Deserialize)]
        struct Oracle {commit:String,cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-color-balance.json")).unwrap();
        assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),60);
        for (index,case) in oracle.cases.iter().enumerate() {
            let mut actual=case.input;let alpha=actual[3];
            if alpha>1e-6 {
                let signal=case.parameters.signal([actual[0]/alpha,actual[1]/alpha,actual[2]/alpha]);
                for channel in 0..3 {actual[channel]=super::super::basic_grade::linear(signal[channel].clamp(0.0,1.0))*alpha;}
            }
            assert_eq!(actual.map(f32::to_bits),case.bits,"original Color Balance case {index}");
        }
    }
    #[test]
    fn tonal_weights_preserve_encoded_luminance_and_bounds() {
        let shadow=ColorBalance{shadow_r:100.0,..Default::default()};assert_eq!(shadow.signal([0.0;3]),[0.25,0.0,0.0]);
        let highlight=ColorBalance{hi_b:-100.0,..Default::default()};let out=highlight.signal([1.0;3]);assert!((out[2]-0.75).abs()<1e-6);
        let preserved=ColorBalance{preserve:true,..shadow};assert!((super::super::black_white::gray(preserved.signal([0.1,0.2,0.3]))-super::super::black_white::gray([0.1,0.2,0.3].map(super::super::basic_grade::display))).abs()<1e-6);
        assert!(ColorBalance{mid_g:101.0,..Default::default()}.validate().is_err());
    }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum BalanceChannel {ShadowR,ShadowG,ShadowB,MidR,MidG,MidB,HiR,HiG,HiB}
pub type BalanceAnimation=std::collections::BTreeMap<BalanceChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&BalanceAnimation)->Result<()> {
    for value in animation.values() {value.validate()?;ensure!(super::composition::animatable_values_in(value,-100.0,100.0,true),"Color Balance animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:ColorBalance,animation:&BalanceAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ColorBalance> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;validate_animation(animation)?;
    for (channel,curve) in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;match channel {BalanceChannel::ShadowR=>parameters.shadow_r=value,BalanceChannel::ShadowG=>parameters.shadow_g=value,BalanceChannel::ShadowB=>parameters.shadow_b=value,BalanceChannel::MidR=>parameters.mid_r=value,BalanceChannel::MidG=>parameters.mid_g=value,BalanceChannel::MidB=>parameters.mid_b=value,BalanceChannel::HiR=>parameters.hi_r=value,BalanceChannel::HiG=>parameters.hi_g=value,BalanceChannel::HiB=>parameters.hi_b=value}}
    parameters.validate()?;Ok(parameters)
}
