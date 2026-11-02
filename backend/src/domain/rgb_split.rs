//! Filmcraft RGB Split; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct RgbSplit {
    pub mode: u8,
    pub amount: f32,
    pub angle: f32,
    pub center: Option<[Option<f64>; 2]>,
    pub blend: f32,
}
impl Default for RgbSplit {
    fn default() -> Self {
        Self {
            mode: 0,
            amount: 10.,
            angle: 0.,
            center: None,
            blend: 0.,
        }
    }
}
impl RgbSplit {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.mode <= 1, "invalid RGB Split mode");
        ensure!(
            self.amount.is_finite() && (0.0..=1000.).contains(&self.amount),
            "invalid RGB Split amount"
        );
        ensure!(self.angle.is_finite(), "nonfinite RGB Split angle");
        ensure!(
            self.blend.is_finite() && (0.0..=100.).contains(&self.blend),
            "invalid RGB Split blend"
        );
        if let Some(c) = self.center {
            ensure!(
                c.into_iter().flatten().all(f64::is_finite),
                "nonfinite RGB Split center"
            );
        }
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(scale.is_finite() && scale > 0., "invalid RGB Split scale");
        checkpoint()?;
        let amount = self.amount * scale;
        ensure!(amount.is_finite(), "RGB Split amount overflow");
        if amount.abs() < 1e-3 {
            return Ok(());
        }
        let (w, h) = frame.dimensions();
        let center = self.center.unwrap_or([None, None]);
        let cx = center[0].map(|v| v * scale as f64).unwrap_or(w as f64 / 2.) as f32;
        let cy = center[1].map(|v| v * scale as f64).unwrap_or(h as f64 / 2.) as f32;
        ensure!(
            cx.is_finite() && cy.is_finite(),
            "RGB Split center overflow"
        );
        let angle = self.angle.to_radians();
        let blend = self.blend / 100.;
        let maxd = ((w as f64).hypot(h as f64) / 2.) as f32;
        let source = frame.clone();
        let mut index = 0;
        frame.map_premultiplied(
            &mut |g| {
                let (px, py) = ((index % w) as f32 + 0.5, (index / w) as f32 + 0.5);
                index += 1;
                let (ox, oy) = if self.mode == 1 {
                    ((px - cx) / maxd * amount, (py - cy) / maxd * amount)
                } else {
                    (angle.cos() * amount, angle.sin() * amount)
                };
                let r = super::gradient_wipe::transparent_sample(&source, px - ox, py - oy);
                let b = super::gradient_wipe::transparent_sample(&source, px + ox, py + oy);
                let o = [r[0], g[1], b[2], r[3].max(g[3]).max(b[3])];
                std::array::from_fn(|k| o[k] + (g[k] - o[k]) * blend)
            },
            checkpoint,
        )
    }
}
#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn rgb_split_original_working_bits() {
        #[derive(Deserialize)]
        struct Case {
            parameters: RgbSplit,
            width: usize,
            height: usize,
            rgba: Vec<u8>,
            scale: f32,
            bits: Vec<u32>,
        }
        #[derive(Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let f: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-rgb-split.json"
        ))
        .unwrap();
        assert_eq!(f.cases.len(), 3840);
        for (i, c) in f.cases.into_iter().enumerate() {
            let mut frame = LinearColorFrame::from_surface(
                &super::super::group_surface::GroupSurface {
                    width: c.width,
                    height: c.height,
                    rgba: c.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
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
                "RGB Split original {i}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RgbSplitChannel {
    Amount,
    Angle,
    CenterX,
    CenterY,
    Blend,
}
pub type RgbSplitAnimation =
    std::collections::BTreeMap<RgbSplitChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &RgbSplitAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            RgbSplitChannel::Amount => (0., 1000.),
            RgbSplitChannel::Blend => (0., 100.),
            RgbSplitChannel::Angle => (-(f32::MAX as f64), f32::MAX as f64),
            _ => (-f64::MAX, f64::MAX),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "RGB Split animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: RgbSplit, a: &RgbSplitAnimation, t: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<RgbSplit> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)?;
        match c {
            RgbSplitChannel::Amount => p.amount = v as f32,
            RgbSplitChannel::Angle => p.angle = v as f32,
            RgbSplitChannel::Blend => p.blend = v as f32,
            RgbSplitChannel::CenterX | RgbSplitChannel::CenterY => {
                p.center.get_or_insert([None, None])
                    [if *c == RgbSplitChannel::CenterX { 0 } else { 1 }] = Some(v)
            }
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn rgb_split_five_curves_midpoint_hold_bounds_and_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let precise = 1.123456789012;
        let a:RgbSplitAnimation=serde_json::from_value(serde_json::json!({"amount":curve(10.,20.),"angle":curve(-37.,74.),"blend":curve(0.,75.),"centerX":curve(precise,precise),"centerY":curve(0.,2.)})).unwrap();
        let p = RgbSplit {
            mode: 1,
            ..RgbSplit::default()
        };
        let mid = sample_animation(p, &a, 250000).unwrap();
        assert_eq!(
            mid,
            RgbSplit {
                mode: 1,
                amount: 15.,
                angle: 18.5,
                blend: 37.5,
                center: Some([Some(precise), Some(1.)])
            }
        );
        assert_eq!(sample_animation(p, &a, 750000).unwrap().amount, 20.);
        let x = RgbSplitAnimation::from([(
            RgbSplitChannel::CenterX,
            a[&RgbSplitChannel::CenterX].clone(),
        )]);
        assert_eq!(
            sample_animation(p, &x, 250000).unwrap().center,
            Some([Some(precise), None])
        );
        let e: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"rgb_split","parameters":p,"animation":a}),
        )
        .unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        for (c, v) in [
            (RgbSplitChannel::Amount, 1001.),
            (RgbSplitChannel::Blend, -1.),
            (RgbSplitChannel::Angle, f64::MAX),
        ] {
            assert!(validate_animation(&RgbSplitAnimation::from([(
                c,
                super::super::composition::AnimatableValue::Constant { value: v }
            )]))
            .is_err());
        }
    }
}
