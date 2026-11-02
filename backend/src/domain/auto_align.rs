//! Filmcraft Auto Align; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::{group_transform::GroupAffine, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoAlign {
    pub horizontal: u32,
    pub vertical: u32,
    pub margin_x: f32,
    pub margin_y: f32,
}
impl Default for AutoAlign {
    fn default() -> Self {
        Self {
            horizontal: 2,
            vertical: 2,
            margin_x: 400.,
            margin_y: 400.,
        }
    }
}
impl AutoAlign {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.horizontal <= 3 && self.vertical <= 3,
            "Auto Align choice out of range"
        );
        ensure!(
            self.margin_x.is_finite()
                && (0. ..=4000.).contains(&self.margin_x)
                && self.margin_y.is_finite()
                && (0. ..=4000.).contains(&self.margin_y),
            "Auto Align margin out of range"
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
            "invalid Auto Align pixel scale"
        );
        checkpoint()?;
        if self.horizontal == 0 && self.vertical == 0 {
            return Ok(());
        }
        let (w, h) = frame.dimensions();
        let (mut x0, mut x1, mut y0, mut y1) = (usize::MAX, 0, usize::MAX, 0);
        for (y, row) in frame.pixels().chunks_exact(w).enumerate() {
            checkpoint()?;
            for (x, p) in row.iter().enumerate() {
                if p[3] > 0.01 {
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                    y0 = y0.min(y);
                    y1 = y1.max(y);
                }
            }
        }
        if x0 == usize::MAX {
            return Ok(());
        }
        let (x0, x1, y0, y1) = (x0 as f64, x1 as f64 + 1., y0 as f64, y1 as f64 + 1.);
        let (mx, my) = (
            (self.margin_x * px_scale) as f64,
            (self.margin_y * px_scale) as f64,
        );
        let dx = match self.horizontal {
            1 => mx - x0,
            2 => (w as f64 - (x1 - x0)) / 2. - x0,
            3 => w as f64 - mx - x1,
            _ => 0.,
        };
        let dy = match self.vertical {
            1 => my - y0,
            2 => (h as f64 - (y1 - y0)) / 2. - y0,
            3 => h as f64 - my - y1,
            _ => 0.,
        };
        *frame = frame.transformed(
            w,
            h,
            GroupAffine([1., 0., 0., 1., dx.round(), dy.round()]),
            checkpoint,
        )?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn alpha_bounds_center_edges_empty_and_cancel() {
        let mut rgba = vec![0; 5 * 3 * 4];
        rgba[(1 * 5 + 1) * 4..(1 * 5 + 1) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
        rgba[3 * 4..3 * 4 + 4].copy_from_slice(&[255, 0, 0, 1]);
        let frame = LinearColorFrame::from_surface(
            &GroupSurface {
                width: 5,
                height: 3,
                rgba,
            },
            &mut || Ok(()),
        )
        .unwrap();
        let mut center = frame.clone();
        AutoAlign::default()
            .apply_linear(&mut center, 1., &mut || Ok(()))
            .unwrap();
        assert_eq!(center.pixels()[7][3], 1.);
        let mut edge = frame.clone();
        AutoAlign {
            horizontal: 3,
            vertical: 1,
            margin_x: 0.,
            margin_y: 0.,
        }
        .apply_linear(&mut edge, 1., &mut || Ok(()))
        .unwrap();
        assert_eq!(edge.pixels()[4][3], 1.);
        let mut identity = frame.clone();
        AutoAlign {
            horizontal: 0,
            vertical: 0,
            ..Default::default()
        }
        .apply_linear(&mut identity, 1., &mut || Ok(()))
        .unwrap();
        assert_eq!(identity.pixels(), frame.pixels());
        assert!(AutoAlign::default()
            .apply_linear(&mut identity, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
        let mut empty = LinearColorFrame::from_surface(
            &GroupSurface {
                width: 1,
                height: 1,
                rgba: vec![0; 4],
            },
            &mut || Ok(()),
        )
        .unwrap();
        AutoAlign::default()
            .apply_linear(&mut empty, 1., &mut || Ok(()))
            .unwrap();
        assert_eq!(empty.pixels(), &[[0.; 4]]);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoAlignChannel {
    MarginX,
    MarginY,
}
pub type AutoAlignAnimation =
    std::collections::BTreeMap<AutoAlignChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &AutoAlignAnimation) -> Result<()> {
    for v in a.values() {
        v.validate()?;
        ensure!(
            super::composition::animatable_values_in(v, 0., 4000., true),
            "Auto Align curve out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: AutoAlign, a: &AutoAlignAnimation, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<AutoAlign> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let value = super::nested_composition::sample_value(v, time)? as f32;
        match c {
            AutoAlignChannel::MarginX => p.margin_x = value,
            AutoAlignChannel::MarginY => p.margin_y = value,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn auto_align_persisted_curves_defaults_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"auto_align","parameters":{},"animation":{"marginX":curve(0.,3.),"marginY":curve(1.,4.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::AutoAlign {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Auto Align")
        };
        assert_eq!(parameters, AutoAlign::default());
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            AutoAlign {
                margin_x: 1.5,
                margin_y: 2.5,
                ..Default::default()
            }
        );
        let mut invalid = a;
        invalid.insert(
            AutoAlignChannel::MarginX,
            super::super::composition::AnimatableValue::Constant { value: 4001. },
        );
        assert!(validate_animation(&invalid).is_err());
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn original_working_frame_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: AutoAlign,
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
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-auto-align.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 2048);
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
                "Auto Align original working bits case{index}"
            );
        }
    }
}
