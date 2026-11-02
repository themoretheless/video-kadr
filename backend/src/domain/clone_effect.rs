//! Filmcraft Clone tiles; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CloneEffect {
    pub columns: f32,
    pub rows: f32,
    pub gap: f32,
    pub mirror: bool,
}
impl Default for CloneEffect {
    fn default() -> Self {
        Self {
            columns: 2.,
            rows: 1.,
            gap: 0.,
            mirror: false,
        }
    }
}
impl CloneEffect {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.columns.is_finite()
                && (1. ..=16.).contains(&self.columns)
                && self.rows.is_finite()
                && (1. ..=16.).contains(&self.rows),
            "Clone tile count out of range"
        );
        ensure!(
            self.gap.is_finite() && (0. ..=1000.).contains(&self.gap),
            "Clone gap out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            px_scale.is_finite() && px_scale > 0.,
            "invalid Clone pixel scale"
        );
        checkpoint()?;
        let cols = self.columns.clamp(1., 16.) as usize;
        let rows = self.rows.clamp(1., 16.) as usize;
        if cols == 1 && rows == 1 {
            return Ok(());
        }
        let gap = (self.gap * px_scale) as f64;
        let (wi, hi) = frame.dimensions();
        let (w, h) = (wi as f64, hi as f64);
        let tw = ((w - gap * (cols - 1) as f64) / cols as f64).max(1.);
        let th = ((h - gap * (rows - 1) as f64) / rows as f64).max(1.);
        let s = (tw / w).min(th / h);
        let src = frame.pixels().to_vec();
        let mut index = 0;
        frame.map_premultiplied(
            &mut |_| {
                let (px, py) = ((index % wi) as f64 + 0.5, (index / wi) as f64 + 0.5);
                index += 1;
                let (ci, ri) = (
                    ((px / (tw + gap)).floor() as usize).min(cols - 1),
                    ((py / (th + gap)).floor() as usize).min(rows - 1),
                );
                let (ox, oy) = (
                    ci as f64 * (tw + gap) + (tw - w * s) / 2.,
                    ri as f64 * (th + gap) + (th - h * s) / 2.,
                );
                let mut u = (px - ox) / s;
                let v = (py - oy) / s;
                if self.mirror && (ci + ri) % 2 == 1 {
                    u = w - u;
                }
                if u < 0. || v < 0. || u > w || v > h {
                    [0.; 4]
                } else {
                    super::twirl::sample(&src, wi, hi, u as f32, v as f32)
                }
            },
            checkpoint,
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloneChannel {
    Columns,
    Rows,
    Gap,
}
pub type CloneAnimation =
    std::collections::BTreeMap<CloneChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &CloneAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            CloneChannel::Gap => (0., 1000.),
            _ => (1., 16.),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "Clone curve out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: CloneEffect, a: &CloneAnimation, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<CloneEffect> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, time)? as f32;
        match c {
            CloneChannel::Columns => p.columns = v,
            CloneChannel::Rows => p.rows = v,
            CloneChannel::Gap => p.gap = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn single_tile_identity_mirror_gap_and_cancellation() {
        let source = super::super::group_surface::GroupSurface {
            width: 6,
            height: 4,
            rgba: (0..24)
                .flat_map(|i| [(i * 10) as u8, 64, 192, 128])
                .collect(),
        };
        let frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
        let mut a = frame.clone();
        CloneEffect {
            columns: 1.,
            rows: 1.,
            gap: 1000.,
            mirror: true,
        }
        .apply_linear(&mut a, 1., &mut || Ok(()))
        .unwrap();
        assert_eq!(a.pixels(), frame.pixels());
        let p = CloneEffect {
            columns: 2.75,
            rows: 1.,
            gap: 0.75,
            mirror: false,
        };
        p.apply_linear(&mut a, 1., &mut || Ok(())).unwrap();
        let mut b = frame.clone();
        CloneEffect { mirror: true, ..p }
            .apply_linear(&mut b, 1., &mut || Ok(()))
            .unwrap();
        assert_ne!(a.pixels(), b.pixels());
        assert!(p
            .apply_linear(&mut b, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
    #[test]
    fn persisted_three_curves_defaults_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"clone","parameters":{},"animation":{"columns":curve(1.,5.),"rows":curve(1.,3.),"gap":curve(0.,1.5)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::Clone {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Clone")
        };
        assert_eq!(parameters, CloneEffect::default());
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            CloneEffect {
                columns: 3.,
                rows: 2.,
                gap: 0.75,
                mirror: false
            }
        );
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn original_working_frame_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: CloneEffect,
            width: usize,
            height: usize,
            rgba: Vec<u8>,
            scale: f32,
            bits: Vec<u32>,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let fixture: Fixture =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-clone.json"))
                .unwrap();
        assert_eq!(fixture.cases.len(), 1152);
        for (index, c) in fixture.cases.into_iter().enumerate() {
            let source = super::super::group_surface::GroupSurface {
                width: c.width,
                height: c.height,
                rgba: c.rgba,
            };
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            c.parameters
                .apply_linear(&mut frame, c.scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "Clone original working bits case{index}"
            );
        }
    }
}
