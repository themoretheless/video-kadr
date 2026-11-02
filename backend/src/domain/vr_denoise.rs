//! Filmcraft VR De-Noise. MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VrDenoise {
    pub frame_layout: u32,
    pub amount: f32,
    pub noise_type: u32,
    pub show_noise: bool,
}
impl VrDenoise {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.frame_layout <= 2
                && self.noise_type <= 1
                && self.amount.is_finite()
                && (0. ..=100.).contains(&self.amount),
            "VR De-Noise parameter out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            px_scale.is_finite() && px_scale > 0.,
            "invalid VR pixel scale"
        );
        checkpoint()?;
        let level = self.amount / 100.;
        if level <= 0. {
            return Ok(());
        }
        let sigma = (level * if self.noise_type == 1 { 3. } else { 1.8 }) * px_scale.max(0.35);
        let thr = 0.02 + level * 0.12;
        let (w, h) = frame.dimensions();
        let input = frame.pixels();
        let mut output = input.to_vec();
        let views = match self.frame_layout {
            1 if h >= 2 => vec![(0, 0, w, h / 2), (0, h / 2, w, h - h / 2)],
            2 if w >= 2 => vec![(0, 0, w / 2, h), (w / 2, 0, w - w / 2, h)],
            _ => vec![(0, 0, w, h)],
        };
        for (x0, y0, vw, vh) in views {
            checkpoint()?;
            let mut pixels = Vec::with_capacity(vw * vh);
            for y in 0..vh {
                pixels.extend_from_slice(&input[(y0 + y) * w + x0..(y0 + y) * w + x0 + vw]);
            }
            super::vr_filter::blur(&mut pixels, vw, vh, sigma, checkpoint)?;
            for y in 0..vh {
                checkpoint()?;
                for x in 0..vw {
                    let index = (y0 + y) * w + x0 + x;
                    let p = input[index];
                    let q = pixels[y * vw + x];
                    let d = (0..3).map(|c| (p[c] - q[c]).abs()).fold(0., f32::max);
                    let k = (-(d * d) / (thr * thr)).exp();
                    let o: [f32; 4] = std::array::from_fn(|c| p[c] + (q[c] - p[c]) * k);
                    if self.show_noise {
                        for c in 0..3 {
                            output[index][c] = ((p[c] - o[c]) * 4. + 0.18).max(0.) * p[3];
                        }
                    } else {
                        output[index] = o;
                    }
                }
            }
        }
        let mut i = 0;
        frame.map_premultiplied(
            &mut |_| {
                let p = output[i];
                i += 1;
                p
            },
            checkpoint,
        )
    }
}
pub fn validate_animation(value: &super::composition::AnimatableValue) -> Result<()> {
    value.validate()?;
    ensure!(
        super::composition::animatable_values_in(value, 0., 100., true),
        "VR De-Noise animation out of range"
    );
    Ok(())
}
pub fn sample(
    mut p: VrDenoise,
    a: &super::composition::AnimatableValue,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<VrDenoise> {
        let local: super::animation_sample_time::AnimationSampleTime = local.into();

    p.validate()?;
    validate_animation(a)?;
    p.amount = super::nested_composition::sample_value(a, local)? as f32;
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        parameters: VrDenoise,
        px_scale: f32,
        bits: Vec<u32>,
    }
    #[test]
    fn original_vr_denoise_working_bits() {
        let f: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-vr-denoise.json"
        ))
        .unwrap();
        assert_eq!(f.cases.len(), 1296);
        for (i, c) in f.cases.into_iter().enumerate() {
            let mut frame = super::super::linear_color_frame::LinearColorFrame::from_surface(
                &super::super::group_surface::GroupSurface {
                    width: c.width,
                    height: c.height,
                    rgba: c.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            c.parameters
                .apply_linear(&mut frame, c.px_scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "original VR De-Noise case{i}"
            );
        }
    }
    #[test]
    fn scalar_curve_preserves_modes_and_mid_blur_cancellation() {
        let a:super::super::composition::AnimatableValue=serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":1000,"value":100}]}})).unwrap();
        let p = VrDenoise {
            frame_layout: 2,
            amount: 17.,
            noise_type: 1,
            show_noise: true,
        };
        assert_eq!(
            sample(p, &a, 500000).unwrap(),
            VrDenoise { amount: 50., ..p }
        );
        let effect: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"vr_denoise","parameters":p,"animation":a}),
        )
        .unwrap();
        assert_eq!(
            effect,
            serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap()
        );
        let mut frame = super::super::linear_color_frame::LinearColorFrame::from_surface(
            &super::super::group_surface::GroupSurface {
                width: 6,
                height: 4,
                rgba: vec![128; 96],
            },
            &mut || Ok(()),
        )
        .unwrap();
        let before = frame.pixels().to_vec();
        let mut n = 0;
        assert!(VrDenoise { amount: 100., ..p }
            .apply_linear(&mut frame, 1., &mut || {
                n += 1;
                if n == 5 {
                    anyhow::bail!("cancelled")
                }
                Ok(())
            })
            .is_err());
        assert_eq!(frame.pixels(), before);
        assert!(VrDenoise { noise_type: 2, ..p }.validate().is_err());
        assert!(VrDenoise { amount: 101., ..p }.validate().is_err());
    }
}
