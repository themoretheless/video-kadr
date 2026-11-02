//! Independently authored fixed Concat display-space film, portrait and cinematic grades.
use super::{
    concat_chroma::{display, linear},
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
fn luma(c: [f32; 3]) -> f32 {
    c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
}
fn saturation(c: [f32; 3], amount: f32) -> [f32; 3] {
    let y = luma(c);
    c.map(|v| y * (1. - amount) + v * amount)
}
fn smooth(a: f32, b: f32, y: f32) -> f32 {
    let t = ((y - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn split(c: [f32; 3], shadow: [f32; 3], highlight: [f32; 3]) -> [f32; 3] {
    let y = luma(c);
    let low = 1. - smooth(0., 0.6, y);
    let high = smooth(0.4, 1., y);
    std::array::from_fn(|i| c[i] + (shadow[i] * low + highlight[i] * high))
}
pub(crate) fn kelvin(k: f32) -> [f32; 3] {
    let t = k / 100.;
    if t <= 66. {
        [
            1.,
            ((99.4708 * t.ln() - 161.1196) / 255.).clamp(0., 1.),
            ((138.5177 * (t - 10.).ln() - 305.0448) / 255.).clamp(0., 1.),
        ]
    } else {
        [
            (329.6987 * (t - 60.).powf(-0.1332) / 255.).clamp(0., 1.),
            (288.1222 * (t - 60.).powf(-0.0755) / 255.).clamp(0., 1.),
            1.,
        ]
    }
}
fn mono(c: [f32; 3], weights: [f32; 3]) -> [f32; 3] {
    let total = (weights[0] + weights[1] + weights[2]).max(0.001);
    let w = weights.map(|v| v / total);
    [c[0] * w[0] + c[1] * w[1] + c[2] * w[2]; 3]
}
fn tint_midtones(c: [f32; 3], tint: [f32; 3]) -> [f32; 3] {
    let weight = 1. - ((luma(c) - 0.5).abs() * 2.).min(1.);
    std::array::from_fn(|i| c[i] + tint[i] * weight)
}
#[derive(Clone, Copy)]
enum Kind {
    ClassicChrome,
    Cyberpunk,
    Delicious,
    Rosy,
    SciFi,
    HighContrastMono,
    LandscapePop,
    Natural,
    Retro70s,
    Silver,
    Tropical,
    Vaporwave,
    WinterCool,
}
struct Prepared {
    kind: Kind,
    gain: [f32; 3],
}
impl Prepared {
    fn new(kind: Kind) -> Self {
        let temperature = match kind {
            Kind::Delicious => Some(5200.),
            Kind::SciFi => Some(8200.),
            Kind::Natural => Some(5600.),
            Kind::WinterCool => Some(8800.),
            _ => None,
        };
        let gain = if let Some(k) = temperature {
            let daylight = kelvin(6500.);
            let target = kelvin(k);
            let tint = std::array::from_fn(|i| target[i] / daylight[i]);
            let y = luma(tint).max(0.001);
            tint.map(|v| v / y)
        } else {
            [1.; 3]
        };
        Self { kind, gain }
    }
    fn signal(&self, c: [f32; 3]) -> [f32; 3] {
        let c = c.map(display);
        let out = match self.kind {
            Kind::HighContrastMono => mono(c, [0.33; 3]).map(|v| (v - 0.5) * 1.6 + 0.5 - 0.03),
            Kind::LandscapePop => {
                let green = super::concat_scenery_looks::hsl(c, 120., 60., 0., 1.35, 1.05);
                let blue = super::concat_scenery_looks::hsl(green, 210., 50., 0., 1.25, 1.);
                blue.map(|v| (v - 0.5) * 1.12 + 0.5)
            }
            Kind::Natural => {
                let wb = std::array::from_fn(|i| c[i] * self.gain[i]);
                saturation(wb, 1.05).map(|v| (v - 0.5) * 1.04 + 0.5)
            }
            Kind::Retro70s => saturation(
                tint_midtones(c.map(|v| v * (1. - 0.1) + 0.1), [0.1, 0.06, -0.12]),
                0.85,
            ),
            Kind::Silver => tint_midtones(
                mono(c, [0.3, 0.33, 0.37]).map(|v| v * (1. - 0.05) + 0.05),
                [-0.02, 0., 0.03],
            ),
            Kind::Tropical => saturation(split(c, [0.; 3], [-0.06, 0.05, 0.08]), 1.35)
                .map(|v| (v - 0.5) * 1.1 + 0.5),
            Kind::Vaporwave => saturation(split(c, [0.1, 0., 0.15], [0., 0.08, 0.12]), 1.3),
            Kind::WinterCool => {
                let wb = std::array::from_fn(|i| c[i] * self.gain[i]);
                saturation(wb.map(|v| v * (1. - 0.06) + 0.06), 0.8)
            }
            Kind::ClassicChrome => {
                let f = c.map(|v| {
                    let b = v.clamp(0., 1.);
                    let toe = 0.1 * (1. - b);
                    let t = b * (1. - toe) + b * b * toe;
                    let shoulder = 0.35 * t;
                    t * (1. - shoulder) + (1. - (1. - t) * (1. - t)) * shoulder + (v - b)
                });
                saturation(f, 0.9)
            }
            Kind::Cyberpunk => saturation(split(c, [0.08, -0.02, 0.12], [-0.05, 0.08, 0.1]), 1.15)
                .map(|v| (v - 0.5) * 1.15 + 0.5),
            Kind::Delicious => {
                let wb = std::array::from_fn(|i| c[i] * self.gain[i]);
                saturation(wb, 1.3).map(|v| {
                    let b = v.clamp(0., 1.);
                    b * (1. - 0.25) + b * b * (3. - 2. * b) * 0.25 + (v - b)
                })
            }
            Kind::Rosy => saturation(split(c, [0.; 3], [0.08, 0.02, 0.03]), 1.08),
            Kind::SciFi => {
                let wb = std::array::from_fn(|i| c[i] * self.gain[i]);
                saturation(split(wb, [-0.02, 0.03, 0.06], [0., 0.02, 0.03]), 0.85)
            }
        };
        out.map(linear)
    }
}
macro_rules! fixed_look {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name {}
        impl $name {
            pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
                ensure!(
                    rgb.iter().all(|v| v.is_finite()),
                    "nonfinite Concat fixed-look input"
                );
                Ok(Prepared::new(Kind::$name).signal(rgb))
            }
            pub fn apply_linear(
                &self,
                frame: &mut LinearColorFrame,
                checkpoint: &mut dyn FnMut() -> Result<()>,
            ) -> Result<()> {
                let p = Prepared::new(Kind::$name);
                super::concat_working::apply_rgb(frame, |rgb| p.signal(rgb), checkpoint)
            }
        }
    };
}
fixed_look!(ClassicChrome);
fixed_look!(Cyberpunk);
fixed_look!(Delicious);
fixed_look!(Rosy);
fixed_look!(SciFi);

fixed_look!(HighContrastMono);
fixed_look!(LandscapePop);
fixed_look!(Natural);
fixed_look!(Retro70s);
fixed_look!(Silver);
fixed_look!(Tropical);
fixed_look!(Vaporwave);
fixed_look!(WinterCool);
