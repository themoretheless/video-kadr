//! Independently authored Concat lift/gamma/gain in signed display space.
use super::{composition::AnimatableValue, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ColorWheels {
    pub lift: [f32; 3],
    pub gamma: [f32; 3],
    pub gain: [f32; 3],
}
fn hue(w: [f32; 3]) -> [f32; 3] {
    let radius = w[0].hypot(w[1]).min(1.);
    if radius < 1e-5 {
        return [0.; 3];
    }
    let turn = (w[0].atan2(w[1]) / std::f32::consts::TAU + 1.).fract();
    let rgb = [1., 2. / 3., 1. / 3.]
        .map(|offset| (((turn + offset).fract() * 6. - 3.).abs() - 1.).clamp(0., 1.));
    let y = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
    let chroma = rgb.map(|v| v - y);
    let scale = chroma.iter().map(|v| v.abs()).fold(0f32, f32::max);
    chroma.map(|v| v / scale * radius)
}
impl ColorWheels {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.lift
                .into_iter()
                .chain(self.gamma)
                .chain(self.gain)
                .all(|v| v.is_finite() && (-1. ..=1.).contains(&v)),
            "Concat wheel coordinate out of range"
        );
        Ok(())
    }
    pub(crate) fn prepared(&self) -> Result<Prepared> {
        self.validate()?;
        self.prepared_sampled()
    }
    pub(crate) fn prepared_sampled(&self)->Result<Prepared>{
        ensure!(self.lift.into_iter().chain(self.gamma).chain(self.gain).all(|v|v.is_finite()),"nonfinite Concat wheel source value");
        let lift = hue(self.lift);
        let gamma = hue(self.gamma);
        let gain = hue(self.gain);
        let prepared=Prepared {
            neutral: *self == Self::default(),
            black: std::array::from_fn(|i| lift[i] * 0.25 + self.lift[2] * 0.5),
            exponent: std::array::from_fn(|i| (-(gamma[i] * 0.5 + self.gamma[2])).exp2()),
            gain: std::array::from_fn(|i| (gain[i] * 0.5 + self.gain[2]).exp2()),
        };
        ensure!(prepared.black.iter().chain(prepared.exponent.iter()).chain(prepared.gain.iter()).all(|v|v.is_finite()),"nonfinite Concat wheel transform");
        Ok(prepared)
    }
    pub fn signal(&self, input: [f32; 3]) -> Result<[f32; 3]> {
        Ok(self.prepared()?.signal(input))
    }
    pub fn apply_sampled_linear(&self,frame:&mut LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
        let prepared=self.prepared_sampled()?;
        ensure!(prepared.black.iter().chain(prepared.exponent.iter()).chain(prepared.gain.iter()).all(|v|v.is_finite()),"nonfinite Concat wheel transform");
        super::concat_working::apply_rgb(frame,|rgb|prepared.signal(rgb),checkpoint)
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        let prepared = self.prepared()?;
        super::concat_working::apply_rgb(frame, |rgb| prepared.signal(rgb), checkpoint)
    }
}
pub(crate) struct Prepared {
    neutral: bool,
    black: [f32; 3],
    exponent: [f32; 3],
    gain: [f32; 3],
}
impl Prepared {
    fn signal(&self, input: [f32; 3]) -> [f32; 3] {
        if self.neutral {
            return input;
        }
        self.display(input.map(super::concat_chroma::display))
            .map(super::concat_chroma::linear)
    }
    pub(crate) fn display(&self, input: [f32; 3]) -> [f32; 3] {
        if self.neutral {
            return input;
        }
        std::array::from_fn(|i| {
            let v = input[i];
            let below = v.min(1.);
            let moved = below + self.black[i] * (1. - below);
            let curved = if self.exponent[i] == 1. {
                moved
            } else {
                moved.signum() * moved.abs().powf(self.exponent[i])
            };
            (curved + (v - below)) * self.gain[i]
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Channel {
    #[serde(rename = "lift.x")]
    LiftX,
    #[serde(rename = "lift.y")]
    LiftY,
    #[serde(rename = "lift.m")]
    LiftMaster,
    #[serde(rename = "gamma.x")]
    GammaX,
    #[serde(rename = "gamma.y")]
    GammaY,
    #[serde(rename = "gamma.m")]
    GammaMaster,
    #[serde(rename = "gain.x")]
    GainX,
    #[serde(rename = "gain.y")]
    GainY,
    #[serde(rename = "gain.m")]
    GainMaster,
}
pub type Animation = std::collections::BTreeMap<Channel, AnimatableValue>;
pub fn validate_animation(values: &Animation) -> Result<()> {
    for value in values.values() {
        value.validate()?;
        ensure!(
            super::composition::animatable_values_in(value, -1., 1., true),
            "Concat wheel animation out of range"
        );
    }
    Ok(())
}
pub fn sample(
    mut p: ColorWheels,
    values: &Animation,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<ColorWheels> {
    p.validate()?;
    validate_animation(values)?;
    let time = time.into();
    for (channel, value) in values {
        let v = super::nested_composition::sample_value(value, time)? as f32;
        match channel {
            Channel::LiftX => p.lift[0] = v,
            Channel::LiftY => p.lift[1] = v,
            Channel::LiftMaster => p.lift[2] = v,
            Channel::GammaX => p.gamma[0] = v,
            Channel::GammaY => p.gamma[1] = v,
            Channel::GammaMaster => p.gamma[2] = v,
            Channel::GainX => p.gain[0] = v,
            Channel::GainY => p.gain[1] = v,
            Channel::GainMaster => p.gain[2] = v,
        }
    }
    p.validate()?;
    Ok(p)
}

pub type OriginalKeys=std::collections::BTreeMap<String,Vec<super::concat_effect_upgrade::ParamKey>>;
pub fn validate_keys(keys:&OriginalKeys)->Result<()>{
 let mut total=0usize;
 for(channel,run)in keys{let(root,axis)=channel.split_once('.').map_or((channel.as_str(),None),|(r,a)|(r,Some(a)));ensure!(["lift","gamma","gain"].contains(&root)&&axis.is_none_or(|a|matches!(a,"x"|"y"|"m")),"unknown wheel source channel");total+=run.len();ensure!(total<=4096,"too many wheel source keys");ensure!(run.iter().all(|k|k.at.is_finite()&&k.value.is_finite()&&(k.derived||(-1. ..=1.).contains(&k.value))&&k.ease.iter().all(|v|v.is_finite())),"invalid wheel source key");}Ok(())
}
pub fn sample_keys(mut p:ColorWheels,keys:&OriginalKeys,progress:f64)->Result<ColorWheels>{
 p.validate()?;validate_keys(keys)?;ensure!(progress.is_finite(),"nonfinite wheel source time");
 for(channel,run)in keys{let Some((root,axis))=channel.split_once('.')else{continue};let wheel=match root{"lift"=>&mut p.lift,"gamma"=>&mut p.gamma,"gain"=>&mut p.gain,_=>unreachable!()};let index=match axis{"x"=>0,"y"=>1,"m"=>2,_=>unreachable!()};wheel[index]=super::concat_animation::PreparedTrack::new(run)?.value_at(progress,wheel[index] as f64)? as f32;ensure!(wheel[index].is_finite(),"nonfinite wheel source result");}Ok(p)
}
