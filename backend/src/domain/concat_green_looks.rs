//! Independently authored Concat Matcha/Sage display-space grades.
use super::{
    concat_chroma::{display, linear, skin},
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
fn luma(rgb: [f32; 3]) -> f32 {
    rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722
}
fn saturation(rgb: [f32; 3], factor: f32) -> [f32; 3] {
    let y = luma(rgb);
    rgb.map(|v| y * (1. - factor) + v * factor)
}
fn matte(rgb: [f32; 3], black: f32, white: f32) -> [f32; 3] {
    rgb.map(|v| v * (white - black) + black)
}
fn tint(rgb: [f32; 3], color: [f32; 3], amount: f32) -> [f32; 3] {
    let mid = 1. - ((luma(rgb) - 0.5).abs() * 2.).min(1.);
    std::array::from_fn(|i| rgb[i] + color[i] * mid * amount)
}
fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn matcha(rgb: [f32; 3], a: f32) -> [f32; 3] {
    let rgb = rgb.map(display);
    let keep = 1. - skin(rgb) * 0.5;
    let out = saturation(matte(rgb, a * 0.06, 1. - a * 0.03), 1. - a * 0.25);
    let out = tint(out, [-0.03, 0.08, 0.], a * keep);
    let high = smooth(0.4, 1., luma(out));
    let color = [0., 0.06, -0.03];
    std::array::from_fn(|i| linear(out[i] + color[i] * high * a))
}
fn sage(rgb: [f32; 3], a: f32) -> [f32; 3] {
    let rgb = rgb.map(display);
    let keep = 1. - skin(rgb) * 0.8;
    let out = tint(saturation(rgb, 1. - a * 0.3), [-0.04, 0.06, 0.04], a * keep);
    let out = out.map(|v| {
        let c = v.clamp(0., 1.);
        let toe = 0.3 * a * (1. - c);
        let t = c * (1. - toe) + c * c * toe;
        let shoulder = 0.3 * a * t;
        t * (1. - shoulder) + (1. - (1. - t) * (1. - t)) * shoulder + (v - c)
    });
    matte(out, 0.02 * a, 1. - 0.02 * a).map(linear)
}
macro_rules! grade {
    ($name:ident,$signal:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct $name {
            pub amount: f32,
        }
        impl Default for $name {
            fn default() -> Self {
                Self { amount: 60. }
            }
        }
        impl $name {
            pub fn validate(&self) -> Result<()> {
                ensure!(
                    self.amount.is_finite() && (0.0..=100.0).contains(&self.amount),
                    concat!("Concat ", stringify!($name), " amount out of range")
                );
                Ok(())
            }
            pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
                self.validate()?;
                ensure!(
                    rgb.iter().all(|v| v.is_finite()),
                    "nonfinite Concat grade input"
                );
                Ok($signal(rgb, self.amount / 100.))
            }
            pub fn apply_linear(
                &self,
                frame: &mut LinearColorFrame,
                checkpoint: &mut dyn FnMut() -> Result<()>,
            ) -> Result<()> {
                self.validate()?;
                super::concat_working::apply_rgb(
                    frame,
                    |rgb| $signal(rgb, self.amount / 100.),
                    checkpoint,
                )
            }
        }
    };
}
grade!(Matcha, matcha);
grade!(Sage, sage);
