//! Independent single-pass Adjust signal. Registration/UI follow differential qualification.
use super::{
    concat_chroma::{display, linear},
    concat_color_wheels::ColorWheels,
    concat_curves::Curves,
    concat_white_balance::WhiteBalance,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Adjust {
    pub exposure: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub temperature: f32,
    pub tint: f32,
    pub shadows: f32,
    pub highlights: f32,
    pub whites: f32,
    pub blacks: f32,
    pub sharpen: f32,
    pub fade: f32,
    pub vignette: f32,
    pub lift: [f32; 3],
    pub gamma: [f32; 3],
    pub gain: [f32; 3],
    pub luma: Vec<[f64; 2]>,
    pub red: Vec<[f64; 2]>,
    pub green: Vec<[f64; 2]>,
    pub blue: Vec<[f64; 2]>,
}
impl Default for Adjust {
    fn default() -> Self {
        Self {
            exposure: 0.,
            brightness: 0.,
            contrast: 0.,
            saturation: 0.,
            temperature: 6500.,
            tint: 0.,
            shadows: 0.,
            highlights: 0.,
            whites: 0.,
            blacks: 0.,
            sharpen: 0.,
            fade: 0.,
            vignette: 0.,
            lift: [0.; 3],
            gamma: [0.; 3],
            gain: [0.; 3],
            luma: vec![],
            red: vec![],
            green: vec![],
            blue: vec![],
        }
    }
}
fn smooth(low: f32, high: f32, v: f32) -> f32 {
    let t = ((v - low) / (high - low)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn luma(v: [f32; 3]) -> f32 {
    v[0] * 0.2126 + v[1] * 0.7152 + v[2] * 0.0722
}
fn lifted(v: f32, by: f32) -> f32 {
    if v >= 0. {
        (v + by).max(0.)
    } else {
        v + by
    }
}
impl Adjust {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.exposure.is_finite() && (-3. ..=3.).contains(&self.exposure),
            "Concat Adjust exposure out of range"
        );
        for v in [
            self.brightness,
            self.contrast,
            self.saturation,
            self.shadows,
            self.highlights,
            self.whites,
            self.blacks,
            self.tint,
        ] {
            ensure!(
                v.is_finite() && (-100. ..=100.).contains(&v),
                "Concat Adjust signed amount out of range"
            );
        }
        for v in [self.sharpen, self.fade, self.vignette] {
            ensure!(
                v.is_finite() && (0. ..=100.).contains(&v),
                "Concat Adjust amount out of range"
            );
        }
        WhiteBalance {
            temperature: self.temperature,
            tint: self.tint,
        }
        .validate()?;
        self.wheels().validate()?;
        self.curves().validate()?;
        Ok(())
    }
    fn wheels(&self) -> ColorWheels {
        ColorWheels {
            lift: self.lift,
            gamma: self.gamma,
            gain: self.gain,
        }
    }
    fn curves(&self) -> Curves {
        Curves {
            luma: self.luma.clone(),
            red: self.red.clone(),
            green: self.green.clone(),
            blue: self.blue.clone(),
        }
    }
    /// Input and neighborhood are straight scene-linear RGB, sampled from the SAME original pass input.
    /// UV is pixel-center normalized. Half-float boundaries belong outside this single shader pass.
    pub fn signal(
        &self,
        input: [f32; 3],
        neighbors: [[f32; 3]; 9],
        uv: [f32; 2],
    ) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            input
                .into_iter()
                .chain(neighbors.into_iter().flatten())
                .chain(uv)
                .all(f32::is_finite),
            "Nonfinite Adjust input"
        );
        Ok(self.prepared()?.signal(input, neighbors, uv))
    }
    /// One native pass with one original half-float input and one half-float output boundary.
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.apply_prepared(self.prepared()?,frame,checkpoint)
    }
    pub fn apply_sampled_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
        let mut base=self.clone();base.lift=[0.;3];base.gamma=[0.;3];base.gain=[0.;3];
        let mut prepared=base.prepared()?;prepared.parameters=self.clone();prepared.wheels=self.wheels().prepared_sampled()?;
        self.apply_prepared(prepared,frame,checkpoint)
    }
    fn apply_prepared(&self,prepared:PreparedAdjust,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
        let (width, height) = frame.dimensions();
        let straight = |p: [f32; 4]| -> [f32; 3] {
            if p[3] > 0. {
                std::array::from_fn(|i| half::f16::from_f32(p[i] / p[3]).to_f32())
            } else {
                [0.; 3]
            }
        };
        let mut operation = |mut p: [f32; 4], neighbors: [[f32; 3]; 9], x: usize, y: usize| {
            if p[3] <= 0. {
                return p;
            }
            let input = straight(p);
            let out = prepared.signal(
                input,
                neighbors,
                [
                    (x as f32 + 0.5) / width as f32,
                    (y as f32 + 0.5) / height as f32,
                ],
            );
            let alpha = half::f16::from_f32(p[3]).to_f32();
            for i in 0..3 {
                p[i] = super::concat_working::texture_half(out[i].clamp(-65504., 65504.)) * alpha;
            }
            p[3] = alpha;
            p
        };
        if self.sharpen > 0. {
            frame.map_original_rgb_neighborhood(&mut |p| straight(p), &mut operation, checkpoint)
        } else {
            frame.map_concat_rgb_pixels(&mut |p, x, y| operation(p, [[0.; 3]; 9], x, y), checkpoint)
        }
    }
    pub fn prepared(&self) -> Result<PreparedAdjust> {
        self.validate()?;
        Ok(PreparedAdjust {
            parameters: self.clone(),
            white: WhiteBalance {
                temperature: self.temperature,
                tint: self.tint,
            }
            .prepared()?,
            wheels: self.wheels().prepared()?,
            curves: self.curves().prepared()?,
        })
    }
}
pub struct PreparedAdjust {
    parameters: Adjust,
    white: super::concat_white_balance::PreparedWhiteBalance,
    wheels: super::concat_color_wheels::Prepared,
    curves: super::concat_curves::Prepared,
}
impl PreparedAdjust {
    fn signal(&self, input: [f32; 3], neighbors: [[f32; 3]; 9], uv: [f32; 2]) -> [f32; 3] {
        let exposed = input.map(|v| v * self.parameters.exposure.exp2());
        let bright =
            exposed.map(|v| linear(lifted(display(v), self.parameters.brightness / 100. * 0.5)));
        let balanced = self.white.signal(bright);
        let contrast = super::concat_contrast::ConcatContrast {
            amount: self.parameters.contrast,
        };
        let mut d = balanced.map(|v| display(contrast.channel(v)));
        let s = self.parameters.saturation / 100.;
        let factor = if s > 0. {
            1. + 2. * s
        } else {
            (1. + s) * (1. + s)
        };
        let y = luma(d);
        d = d.map(|v| y * (1. - factor) + v * factor);
        let y = luma(d.map(|v| v.clamp(0., 1.)));
        let by = (1. - smooth(0., 0.5, y)) * self.parameters.shadows / 100. * 0.18
            + smooth(0.5, 1., y) * self.parameters.highlights / 100. * 0.18
            + (1. - smooth(0., 0.25, y)) * self.parameters.blacks / 100. * 0.1
            + smooth(0.75, 1., y) * self.parameters.whites / 100. * 0.1;
        d = d.map(|v| lifted(v, by));
        let matte = self.parameters.fade / 100. * 0.25;
        d = d.map(|v| {
            let below = v.min(1.);
            below * (1. - matte) + matte + (v - below)
        });
        d = self.wheels.display(d);
        d = self.curves.display(d);
        if self.parameters.sharpen > 0. {
            let mut blur = [0.; 3];
            for n in neighbors {
                for i in 0..3 {
                    blur[i] += display(n[i]);
                }
            }
            for i in 0..3 {
                d[i] += (display(input[i]) - blur[i] / 9.) * self.parameters.sharpen / 100. * 2.;
            }
        }
        let mut light = d.map(linear);
        if self.parameters.vignette > 0. {
            let v = self.parameters.vignette / 100.;
            let r = ((uv[0] - 0.5) * 2.).hypot((uv[1] - 0.5) * 2.);
            let fall = smooth(1.4 - v * 0.9, 1.6 - v * 0.3, r);
            let attenuation = (1. - fall * (0.4 + 0.6 * v)).powf(2.4);
            light = light.map(|c| c * attenuation);
        }
        light
    }
}

/// Source Adjust animates wheel coordinates only; scalar sliders and curve points remain authored constants.
pub fn sample_animation(
    parameters: &Adjust,
    animation: &super::concat_color_wheels::Animation,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<Adjust> {
    parameters.validate()?;
    let wheels = super::concat_color_wheels::sample(parameters.wheels(), animation, time)?;
    let mut sampled = parameters.clone();
    sampled.lift = wheels.lift;
    sampled.gamma = wheels.gamma;
    sampled.gain = wheels.gain;
    Ok(sampled)
}

pub fn sample_keys(parameters:&Adjust,keys:&super::concat_color_wheels::OriginalKeys,progress:f64)->Result<Adjust>{
    parameters.validate()?;let wheels=super::concat_color_wheels::sample_keys(parameters.wheels(),keys,progress)?;
    let mut p=parameters.clone();p.lift=wheels.lift;p.gamma=wheels.gamma;p.gain=wheels.gain;Ok(p)
}
