//! Filmcraft Extract CPU formula (MIT; pinned gpufx.rs).
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct ColorExtract {pub black:f32,pub white:f32,pub softness:f32,pub invert:bool}
impl Default for ColorExtract {fn default()->Self {Self{black:0.0,white:255.0,softness:0.0,invert:false}}}
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum ExtractChannel {Black,White,Softness}
pub type ExtractAnimation=std::collections::BTreeMap<ExtractChannel,super::composition::AnimatableValue>;
impl ColorExtract {
    pub fn validate(&self)->Result<()> {
        for (value,max) in [(self.black,255.0),(self.white,255.0),(self.softness,100.0)] {ensure!(value.is_finite() && (0.0..=max).contains(&value),"Extract parameter out of range");}Ok(())
    }
    /// Result is linear RGB gray, despite thresholding display-encoded linear luminance.
    pub fn gray(&self,input:[f32;3])->f32 {
        let luminance=0.2126*input[0]+0.7152*input[1]+0.0722*input[2];
        let l=super::basic_grade::display(luminance.max(0.0));
        let lo=self.black/255.0;let hi=self.white/255.0;let soft=self.softness/100.0*0.2+1e-4;
        let inside=((l-lo)/soft).clamp(0.0,1.0).min(((hi-l)/soft).clamp(0.0,1.0));
        if self.invert {1.0-inside}else{inside}
    }
    pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
        self.validate()?;
        frame.map_premultiplied(&mut |mut pixel|{let a=pixel[3];if a>1e-6 {let v=self.gray([pixel[0]/a,pixel[1]/a,pixel[2]/a])*a;pixel[..3].fill(v);}pixel},checkpoint)
    }
}
pub fn validate_animation(animation:&ExtractAnimation)->Result<()> {
    for (channel,value) in animation {value.validate()?;let max=if *channel==ExtractChannel::Softness {100.0}else{255.0};ensure!(super::composition::animatable_values_in(value,0.0,max,true),"Extract animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:ColorExtract,animation:&ExtractAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ColorExtract> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;validate_animation(animation)?;
    for (channel,curve) in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;match channel {ExtractChannel::Black=>parameters.black=value,ExtractChannel::White=>parameters.white=value,ExtractChannel::Softness=>parameters.softness=value}}
    parameters.validate()?;Ok(parameters)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_cpu_rgba_bits() {
        #[derive(Deserialize)]
        struct Case {parameters:ColorExtract,input:[f32;4],bits:[u32;4]}
        #[derive(Deserialize)]
        struct Oracle {commit:String,cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-extract.json")).unwrap();
        assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),72);
        for (index,case) in oracle.cases.iter().enumerate() {
            case.parameters.validate().unwrap();let mut actual=case.input;let alpha=actual[3];
            if alpha>1e-6 {let gray=case.parameters.gray([actual[0]/alpha,actual[1]/alpha,actual[2]/alpha])*alpha;actual[..3].fill(gray);}
            assert_eq!(actual.map(f32::to_bits),case.bits,"original Extract case {index}");
        }
    }
    #[test]
    fn luminance_band_inversion_and_crossed_endpoints() {
        let default=ColorExtract::default();assert_eq!(default.gray([0.25;3]),1.0);assert_eq!(default.gray([0.0;3]),0.0);
        assert_eq!(ColorExtract{invert:true,..default}.gray([0.25;3]),0.0);
        let crossed=ColorExtract{black:200.0,white:50.0,..default};crossed.validate().unwrap();assert_eq!(crossed.gray([0.25;3]),0.0);
        assert!(ColorExtract{softness:101.0,..default}.validate().is_err());
    }
}
