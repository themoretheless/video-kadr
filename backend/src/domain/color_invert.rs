//! Filmcraft Invert signal and premultiplied-alpha formulas (MIT; pinned gpufx.rs).
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all="camelCase", deny_unknown_fields)]
pub struct ColorInvert {
    pub channel: u8,
    pub blend: f32,
}
pub fn validate_animation(value:&super::composition::AnimatableValue)->Result<()> {
    value.validate()?;
    ensure!(super::composition::animatable_values_in(value,0.0,100.0,true), "Invert blend animation out of range");
    Ok(())
}
pub fn sample_animation(mut parameters:ColorInvert,animation:&super::composition::AnimatableValue,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ColorInvert> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;validate_animation(animation)?;
    parameters.blend=super::nested_composition::sample_value(animation,time)? as f32;
    parameters.validate()?;Ok(parameters)
}
impl ColorInvert {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.channel<=4 && self.blend.is_finite() && (0.0..=100.0).contains(&self.blend), "Invert parameter out of range");
        Ok(())
    }
    pub fn signal(&self, input:[f32;3]) -> [f32;3] {
        let blend=self.blend/100.0;
        std::array::from_fn(|index| {
            let original=input[index];
            let inverted=if self.channel==0 || usize::from(self.channel)==index+1 {1.0-original}else{original};
            inverted+(original-inverted)*blend
        })
    }
    pub fn alpha_pixel(&self, mut pixel:[f32;4]) -> [f32;4] {
        let alpha=pixel[3];let inverted=1.0-alpha;
        let scale=if alpha>1e-6 {inverted/alpha}else{0.0};
        for value in &mut pixel[..3] {*value*=scale;}
        let blend=self.blend/100.0;
        pixel[3]=inverted*(1.0-blend)+alpha*blend;
        pixel
    }
    pub fn apply_linear(&self, frame:&mut super::linear_color_frame::LinearColorFrame, checkpoint:&mut dyn FnMut()->Result<()>) -> Result<()> {
        self.validate()?;
        if self.channel==4 {frame.map_premultiplied(&mut |pixel|self.alpha_pixel(pixel),checkpoint)}
        else {frame.map_signal(&mut |input,_,_|Ok(self.signal(input)),checkpoint)}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_cpu_renderer_rgba_bits() {
        #[derive(Deserialize)]
        struct Case {parameters:ColorInvert,input:[f32;4],bits:[u32;4]}
        #[derive(Deserialize)]
        struct Oracle {commit:String,cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-invert.json")).unwrap();
        assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(),120);
        for (index,case) in oracle.cases.iter().enumerate() {
            case.parameters.validate().unwrap();
            let mut actual=case.input;
            if case.parameters.channel==4 {actual=case.parameters.alpha_pixel(actual);}
            else if actual[3]>1e-6 {
                let alpha=actual[3];
                let signal=[actual[0],actual[1],actual[2]].map(|value|super::super::basic_grade::display((value/alpha).max(0.0)));
                let output=case.parameters.signal(signal);
                for channel in 0..3 {actual[channel]=super::super::basic_grade::linear(output[channel].clamp(0.0,1.0))*alpha;}
            }
            assert_eq!(actual.map(f32::to_bits),case.bits,"original renderer case {index}");
        }
    }
    #[test]
    fn alpha_effect_reaches_output_surface_including_zero_alpha() {
        use super::super::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
        let source=GroupSurface{width:3,height:1,rgba:vec![64,128,192,64,64,128,192,255,0,0,0,0]};
        let mut frame=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
        ColorInvert{channel:4,blend:0.0}.apply_linear(&mut frame,&mut||Ok(())).unwrap();
        let result=frame.to_surface(&mut||Ok(())).unwrap();
        assert_eq!(&result.rgba[..4],&[64,128,192,191]);
        assert_eq!(result.rgba[7],0);
        assert_eq!(&result.rgba[8..12],&[0,0,0,255]);
    }
    #[test]
    fn channels_blending_and_source_alpha_contract() {
        assert_eq!(ColorInvert::default().signal([0.25,0.5,0.75]),[0.75,0.5,0.25]);
        assert_eq!(ColorInvert{channel:1,blend:50.0}.signal([0.25,0.5,0.75]),[0.5,0.5,0.75]);
        // Upstream rescales RGB to inverted alpha even when blending alpha with original.
        assert_eq!(ColorInvert{channel:4,blend:100.0}.alpha_pixel([0.125,0.25,0.5,0.25]),[0.375,0.75,1.5,0.25]);
        assert_eq!(ColorInvert{channel:4,blend:0.0}.alpha_pixel([0.0;4]),[0.0,0.0,0.0,1.0]);
        assert!(ColorInvert{channel:5,blend:0.0}.validate().is_err());
    }
}
