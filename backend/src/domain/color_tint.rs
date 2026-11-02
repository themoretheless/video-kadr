//! Filmcraft Tint: display-encoded linear luminance ramp (MIT, pinned gpufx.rs).
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct ColorTint {pub black:[f32;4],pub white:[f32;4],pub amount:f32}
impl Default for ColorTint {fn default()->Self {Self{black:[0.0,0.0,0.0,1.0],white:[1.0;4],amount:100.0}}}
impl ColorTint {
    pub fn validate(&self)->Result<()> {
        ensure!(self.black.iter().chain(self.white.iter()).all(|value|value.is_finite()),"nonfinite Tint color");
        ensure!(self.amount.is_finite() && (0.0..=100.0).contains(&self.amount),"Tint amount out of range");Ok(())
    }
    pub fn signal(&self,input:[f32;3])->[f32;3] {
        let l=super::basic_grade::display(super::black_white::gray(input).max(0.0));
        let encoded=input.map(|value|super::basic_grade::display(value.max(0.0)));
        let amount=self.amount/100.0;
        std::array::from_fn(|channel| {
            let tint=self.black[channel]+(self.white[channel]-self.black[channel])*l;
            encoded[channel]+(tint-encoded[channel])*amount
        })
    }
    pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
        self.validate()?;frame.map_linear_to_signal(&mut |input,_,_|Ok(self.signal(input)),checkpoint)
    }
}
pub fn validate_animation(value:&super::composition::AnimatableValue)->Result<()> {
    value.validate()?;ensure!(super::composition::animatable_values_in(value,0.0,100.0,true),"Tint amount animation out of range");Ok(())
}
pub fn sample_animation(mut parameters:ColorTint,animation:&super::composition::AnimatableValue,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ColorTint> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;validate_animation(animation)?;parameters.amount=super::nested_composition::sample_value(animation,time)? as f32;parameters.validate()?;Ok(parameters)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persisted_independent_endpoint_animation_local_sampling() {
        let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({
            "kind":"color_tint","parameters":{"amount":25},"colorAnimation":{
                "blackR":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":-0.5},{"tick":1000000,"value":1.5}]}},
                "whiteA":{"mode":"constant","value":0.25}
            }
        })).unwrap();
        let serialized=serde_json::to_value(&effect).unwrap();assert!(serialized["colorAnimation"]["blackR"].is_object());
        let super::super::composition::VideoEffect::ColorTint{parameters,color_animation:Some(animation),..}=effect else {panic!("Tint")};
        let sampled=sample_colors(parameters,&animation,500000).unwrap();
        assert_eq!(sampled.black,[0.5,0.0,0.0,1.0]);assert_eq!(sampled.white,[1.0,1.0,1.0,0.25]);assert_eq!(sampled.amount,25.0);
        let invalid=std::collections::BTreeMap::from([(TintColorChannel::WhiteR,super::super::composition::AnimatableValue::Constant{value:f64::MAX})]);
        assert!(validate_color_animation(&invalid).is_err());
    }
    #[test]
    fn original_cpu_rgba_bits() {
        #[derive(serde::Deserialize)]
        struct Case {parameters:ColorTint,input:[f32;4],bits:[u32;4]}
        #[derive(serde::Deserialize)]
        struct Oracle {commit:String,cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-tint.json")).unwrap();
        assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),72);
        for (index,case) in oracle.cases.iter().enumerate() {
            let mut actual=case.input;let alpha=actual[3];
            if alpha>1e-6 {
                let signal=case.parameters.signal([actual[0]/alpha,actual[1]/alpha,actual[2]/alpha]);
                for channel in 0..3 {actual[channel]=super::super::basic_grade::linear(signal[channel].clamp(0.0,1.0))*alpha;}
            }
            assert_eq!(actual.map(f32::to_bits),case.bits,"original Tint case {index}");
        }
    }
    #[test]
    fn endpoint_colors_amount_and_ignored_color_alpha() {
        let tint=ColorTint{black:[1.0,0.0,0.0,0.0],white:[0.0,0.0,1.0,0.0],amount:100.0};
        assert_eq!(tint.signal([0.0;3]),[1.0,0.0,0.0]);
        let out=tint.signal([1.0;3]);assert!(out[0].abs()<1e-6);assert_eq!(out[1],0.0);assert!((out[2]-1.0).abs()<1e-6);
        assert_eq!(ColorTint{amount:0.0,..tint}.signal([0.0;3]),[0.0;3]);
        assert!(ColorTint{amount:101.0,..tint}.validate().is_err());
    }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum TintColorChannel {BlackR,BlackG,BlackB,BlackA,WhiteR,WhiteG,WhiteB,WhiteA}
pub type TintColorAnimation=std::collections::BTreeMap<TintColorChannel,super::composition::AnimatableValue>;
pub fn validate_color_animation(animation:&TintColorAnimation)->Result<()> {
    for value in animation.values() {value.validate()?;ensure!(super::composition::animatable_values_in(value,-(f32::MAX as f64),f32::MAX as f64,true),"Tint color animation exceeds finite f32 range");}
    Ok(())
}
pub fn sample_colors(mut parameters:ColorTint,animation:&TintColorAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ColorTint> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    validate_color_animation(animation)?;
    for (channel,value) in animation {
        let value=super::nested_composition::sample_value(value,time)? as f32;
        let (color,index)=match channel {
            TintColorChannel::BlackR=>(&mut parameters.black,0),TintColorChannel::BlackG=>(&mut parameters.black,1),
            TintColorChannel::BlackB=>(&mut parameters.black,2),TintColorChannel::BlackA=>(&mut parameters.black,3),
            TintColorChannel::WhiteR=>(&mut parameters.white,0),TintColorChannel::WhiteG=>(&mut parameters.white,1),
            TintColorChannel::WhiteB=>(&mut parameters.white,2),TintColorChannel::WhiteA=>(&mut parameters.white,3),
        };color[index]=value;
    }
    parameters.validate()?;Ok(parameters)
}
