//! Filmcraft Levels source signal math (MIT, pinned gpufx.rs).
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct ColorLevels {pub in_black:f32,pub in_white:f32,pub out_black:f32,pub out_white:f32,pub gamma:f32}
impl Default for ColorLevels {
    fn default()->Self {Self{in_black:0.0,in_white:255.0,out_black:0.0,out_white:255.0,gamma:100.0}}
}
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum LevelsChannel {InBlack,InWhite,OutBlack,OutWhite,Gamma}
pub type LevelsAnimation=std::collections::BTreeMap<LevelsChannel,super::composition::AnimatableValue>;
impl ColorLevels {
    pub fn validate(&self)->Result<()> {
        for value in [self.in_black,self.in_white,self.out_black,self.out_white] {ensure!(value.is_finite() && (0.0..=255.0).contains(&value),"Levels endpoint out of range");}
        ensure!(self.gamma.is_finite() && (10.0..=1000.0).contains(&self.gamma),"Levels gamma out of range");Ok(())
    }
    pub fn signal(&self,value:f32)->f32 {
        let ib=self.in_black/255.0;let iw=(self.in_white/255.0).max(ib+1e-3);
        let ob=self.out_black/255.0;let ow=self.out_white/255.0;let gamma=100.0/self.gamma.max(1.0);
        ob+((value-ib)/(iw-ib)).clamp(0.0,1.0).powf(gamma)*(ow-ob)
    }
    pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
        self.validate()?;frame.map_signal(&mut |input,_,_|Ok(input.map(|value|self.signal(value))),checkpoint)
    }
}
pub fn validate_animation(animation:&LevelsAnimation)->Result<()> {
    for (channel,value) in animation {value.validate()?;let (min,max)=if *channel==LevelsChannel::Gamma {(10.0,1000.0)}else{(0.0,255.0)};ensure!(super::composition::animatable_values_in(value,min,max,true),"Levels animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:ColorLevels,animation:&LevelsAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ColorLevels> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;validate_animation(animation)?;
    for (channel,curve) in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;match channel {LevelsChannel::InBlack=>parameters.in_black=value,LevelsChannel::InWhite=>parameters.in_white=value,LevelsChannel::OutBlack=>parameters.out_black=value,LevelsChannel::OutWhite=>parameters.out_white=value,LevelsChannel::Gamma=>parameters.gamma=value}}
    parameters.validate()?;Ok(parameters)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independently_compiled_source_renderer_bits() {
        #[derive(Deserialize)]
        struct Case {
            parameters: ColorLevels,
            input: [f32; 3],
            bits: [u32; 3],
        }
        #[derive(Deserialize)]
        struct Oracle {
            commit: String,
            cases: Vec<Case>,
        }
        let oracle: Oracle =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-levels.json"))
                .unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(), 36);
        for (index, case) in oracle.cases.iter().enumerate() {
            case.parameters.validate().unwrap();
            let actual = case.input.map(|v| {
                super::super::basic_grade::linear(
                    case.parameters
                        .signal(super::super::basic_grade::display(v.max(0.0)))
                        .clamp(0.0, 1.0),
                )
                .to_bits()
            });
            assert_eq!(actual, case.bits, "source case{index}");
        }
    }
    #[test]
    fn independent_channel_animation_and_persistence() {
        let animation:LevelsAnimation=serde_json::from_value(serde_json::json!({"gamma":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":100.0},{"tick":1000000,"value":300.0}]}},"outBlack":{"mode":"constant","value":32.0}})).unwrap();
        let sampled=sample_animation(ColorLevels::default(),&animation,500000).unwrap();
        assert_eq!(sampled.gamma,200.0);assert_eq!(sampled.out_black,32.0);assert_eq!(sampled.in_white,255.0);
        let restored:LevelsAnimation=serde_json::from_value(serde_json::to_value(&animation).unwrap()).unwrap();assert_eq!(restored,animation);
        let invalid:LevelsAnimation=serde_json::from_value(serde_json::json!({"gamma":{"mode":"constant","value":9.0}})).unwrap();assert!(validate_animation(&invalid).is_err());
    }
    #[test]
    fn source_endpoint_floor_inverse_output_and_gamma() {
        let identity=ColorLevels::default();assert_eq!(identity.signal(0.5),0.5);
        assert_eq!(ColorLevels{gamma:200.0,..identity}.signal(0.25),0.5);
        assert_eq!(ColorLevels{out_black:255.0,out_white:0.0,..identity}.signal(0.25),0.75);
        let crossed=ColorLevels{in_black:128.0,in_white:64.0,..identity};crossed.validate().unwrap();
        assert_eq!(crossed.signal(0.5),0.0);assert_eq!(crossed.signal(1.0),1.0);
        assert!(ColorLevels{gamma:0.0,..identity}.validate().is_err());
    }
}
