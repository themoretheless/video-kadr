//! Filmcraft VR Blur/Sharpen, MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VrBlur {
    pub frame_layout: u32,
    pub blurriness: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VrSharpen {
    pub frame_layout: u32,
    pub amount: f32,
}
fn validate(layout: u32, strength: f32, max: f32) -> Result<()> {
    ensure!(
        layout <= 2 && strength.is_finite() && (0. ..=max).contains(&strength),
        "VR filter parameter out of range"
    );
    Ok(())
}
impl VrBlur {
    pub fn validate(&self) -> Result<()> {
        validate(self.frame_layout, self.blurriness, 1000.)
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        apply(
            frame,
            self.frame_layout,
            self.blurriness,
            false,
            px_scale,
            checkpoint,
        )
    }
}
impl VrSharpen {
    pub fn validate(&self) -> Result<()> {
        validate(self.frame_layout, self.amount, 500.)
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        apply(
            frame,
            self.frame_layout,
            self.amount,
            true,
            px_scale,
            checkpoint,
        )
    }
}
pub(crate) fn blur(
    px: &mut [[f32; 4]],
    w: usize,
    h: usize,
    sigma: f32,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    if sigma <= 0.3 {
        return Ok(());
    }
    for (y, row) in px.chunks_exact_mut(w).enumerate() {
        checkpoint()?;
        let lat = (0.5 - (y as f64 + 0.5) / h as f64) * std::f64::consts::PI;
        let s = (sigma as f64 / lat.cos().max(1e-3)).min(w as f64 / 4.) as f32;
        if s <= 0.3 {
            continue;
        }
        let r = ((((12. * s * s / 3. + 1.).sqrt() - 1.) / 2.).round().max(1.) as isize)
            .min((w as isize - 1) / 2)
            .max(0);
        if r == 0 {
            continue;
        }
        let wi = w as isize;
        let mut tmp = row.to_vec();
        for _ in 0..3 {
            checkpoint()?;
            let inv = 1. / (2 * r + 1) as f32;
            let mut acc = [0f32; 4];
            for j in -r..=r {
                let i = j.rem_euclid(wi) as usize;
                for k in 0..4 {
                    acc[k] += row[i][k];
                }
            }
            for x in 0..wi {
                for k in 0..4 {
                    tmp[x as usize][k] = acc[k] * inv;
                }
                let (out_i, in_i) = (
                    (x - r).rem_euclid(wi) as usize,
                    (x + r + 1).rem_euclid(wi) as usize,
                );
                for k in 0..4 {
                    acc[k] += row[in_i][k] - row[out_i][k];
                }
            }
            row.copy_from_slice(&tmp);
        }
    }
    for c in 0..4 {
        checkpoint()?;
        let plane: Vec<f32> = px.iter().map(|p| p[c]).collect();
        let vertical = super::gaussian_blur::blur_plane(&plane, w, h, sigma, 2, true, checkpoint)?;
        for (p, value) in px.iter_mut().zip(vertical) {
            p[c] = value;
        }
    }
    Ok(())
}
fn apply(
    frame: &mut super::linear_color_frame::LinearColorFrame,
    layout: u32,
    strength: f32,
    sharpen: bool,
    px_scale: f32,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    ensure!(
        px_scale.is_finite() && px_scale > 0.,
        "invalid VR pixel scale"
    );
    checkpoint()?;
    if strength == 0. {
        return Ok(());
    }
    let sigma = if sharpen {
        px_scale.max(0.35) * 1.2
    } else {
        strength * px_scale * 0.5
    };
    let (w, h) = frame.dimensions();
    let input = frame.pixels();
    let mut output = input.to_vec();
    let views = match layout {
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
        blur(&mut pixels, vw, vh, sigma, checkpoint)?;
        for y in 0..vh {
            checkpoint()?;
            for x in 0..vw {
                let index = (y0 + y) * w + x0 + x;
                let q = pixels[y * vw + x];
                if sharpen {
                    for k in 0..3 {
                        output[index][k] = (input[index][k]
                            + (input[index][k] - q[k]) * (strength / 100.))
                            .max(0.);
                    }
                } else {
                    output[index] = q;
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
pub fn validate_animation(
    value: &super::composition::AnimatableValue,
    sharpen: bool,
) -> Result<()> {
    value.validate()?;
    ensure!(
        super::composition::animatable_values_in(
            value,
            0.,
            if sharpen { 500. } else { 1000. },
            true
        ),
        "VR filter animation out of range"
    );
    Ok(())
}
pub fn sample_blur(
    mut p: VrBlur,
    a: &super::composition::AnimatableValue,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<VrBlur> {
    let local: super::animation_sample_time::AnimationSampleTime = local.into();

    p.validate()?;
    validate_animation(a, false)?;
    p.blurriness = super::nested_composition::sample_value(a, local)? as f32;
    p.validate()?;
    Ok(p)
}
pub fn sample_sharpen(
    mut p: VrSharpen,
    a: &super::composition::AnimatableValue,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<VrSharpen> {
    let local: super::animation_sample_time::AnimationSampleTime = local.into();

    p.validate()?;
    validate_animation(a, true)?;
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
        effect: String,
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        layout: u32,
        strength: f32,
        px_scale: f32,
        bits: Vec<u32>,
    }
    #[test]
    fn original_vr_filter_working_bits() {
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-vr-filter.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 1080);
        for (i, c) in fixture.cases.into_iter().enumerate() {
            let mut image = super::super::linear_color_frame::LinearColorFrame::from_surface(
                &super::super::group_surface::GroupSurface {
                    width: c.width,
                    height: c.height,
                    rgba: c.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            if c.effect == "vr_blur" {
                VrBlur {
                    frame_layout: c.layout,
                    blurriness: c.strength,
                }
                .apply_linear(&mut image, c.px_scale, &mut || Ok(()))
                .unwrap();
            } else {
                VrSharpen {
                    frame_layout: c.layout,
                    amount: c.strength,
                }
                .apply_linear(&mut image, c.px_scale, &mut || Ok(()))
                .unwrap();
            }
            assert_eq!(
                image
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "{} case{}",
                c.effect,
                i
            );
        }
    }
    #[test]
    fn persisted_scalar_animation_layout_and_cancellation() {
        let curve:super::super::composition::AnimatableValue=serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":1000,"value":500}]}})).unwrap();
        assert_eq!(
            sample_blur(
                VrBlur {
                    frame_layout: 2,
                    blurriness: 17.
                },
                &curve,
                500000
            )
            .unwrap(),
            VrBlur {
                frame_layout: 2,
                blurriness: 250.
            }
        );
        assert_eq!(
            sample_sharpen(
                VrSharpen {
                    frame_layout: 1,
                    amount: 17.
                },
                &curve,
                500000
            )
            .unwrap(),
            VrSharpen {
                frame_layout: 1,
                amount: 250.
            }
        );
        for kind in ["vr_blur", "vr_sharpen"] {
            let effect: super::super::composition::VideoEffect = serde_json::from_value(
                serde_json::json!({"kind":kind,"parameters":{"frameLayout":2},"animation":curve}),
            )
            .unwrap();
            assert_eq!(
                effect,
                serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap()
            );
        }
        let surface = super::super::group_surface::GroupSurface {
            width: 6,
            height: 4,
            rgba: vec![128; 96],
        };
        let mut frame =
            super::super::linear_color_frame::LinearColorFrame::from_surface(&surface, &mut || {
                Ok(())
            })
            .unwrap();
        let before = frame.pixels().to_vec();
        let mut n = 0;
        assert!(VrBlur {
            frame_layout: 2,
            blurriness: 50.
        }
        .apply_linear(&mut frame, 1., &mut || {
            n += 1;
            if n == 5 {
                anyhow::bail!("cancelled")
            }
            Ok(())
        })
        .is_err());
        assert_eq!(frame.pixels(), before);
        assert!(VrBlur {
            frame_layout: 3,
            blurriness: 1.
        }
        .validate()
        .is_err());
        assert!(VrSharpen {
            frame_layout: 0,
            amount: 501.
        }
        .validate()
        .is_err());
    }
}
