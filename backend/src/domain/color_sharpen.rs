//! FilmCraft Lumetri linear premultiplied unsharp mask; MIT notice docs/licenses/filmcraft-MIT.txt.
use super::{composition::AnimatableValue, group_surface::GroupSurface};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorSharpen {
    pub amount: f32,
}
impl ColorSharpen {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.amount.is_finite() && (-100.0..=100.0).contains(&self.amount),
            "invalid sharpen amount"
        );
        Ok(())
    }
    pub fn apply(
        &self,
        image: &mut GroupSurface,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.apply_scaled(image, 1.0, checkpoint)
    }
    pub fn apply_scaled(
        &self,
        image: &mut GroupSurface,
        scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        image.validate()?;
        ensure!(
            scale.is_finite() && scale > 0.0 && scale <= 1.0,
            "invalid sharpen scale"
        );
        checkpoint()?;
        let amount = self.amount / 100.0;
        if amount.abs() <= 1e-3 {
            return Ok(());
        }
        let mut frame =
            super::linear_color_frame::LinearColorFrame::from_surface(image, checkpoint)?;
        frame.sharpen(amount, scale, checkpoint)?;
        *image = frame.to_surface(checkpoint)?;
        Ok(())
    }
}
pub fn validate_animation(value: &AnimatableValue) -> Result<()> {
    value.validate()?;
    ensure!(
        super::composition::animatable_values_in(value, -100.0, 100.0, true),
        "invalid sharpen animation"
    );
    Ok(())
}
pub fn sample_animation(value: &AnimatableValue, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<ColorSharpen> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    validate_animation(value)?;
    let p = ColorSharpen {
        amount: super::nested_composition::sample_value(value, time)? as f32,
    };
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_actual_filmcraft_sdr_sharpen_renderer() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-color-sharpen.json"
        ))
        .unwrap();
        assert_eq!(f["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let input: Vec<u8> = serde_json::from_value(f["input"].clone()).unwrap();
        assert_eq!(f["cases"].as_array().unwrap().len(), 15);
        for (index, case) in f["cases"].as_array().unwrap().iter().enumerate() {
            let p: ColorSharpen = serde_json::from_value(case["parameters"].clone()).unwrap();
            let expected: Vec<u8> = serde_json::from_value(case["expected"].clone()).unwrap();
            let mut frame = GroupSurface {
                width: f["width"].as_u64().unwrap() as usize,
                height: f["height"].as_u64().unwrap() as usize,
                rgba: input.clone(),
            };
            p.apply_scaled(
                &mut frame,
                case["pixelScale"].as_f64().unwrap() as f32,
                &mut || Ok(()),
            )
            .unwrap();
            for (pixel, (actual, expected)) in frame
                .rgba
                .chunks_exact(4)
                .zip(expected.chunks_exact(4))
                .enumerate()
            {
                assert_eq!(actual[3], expected[3]);
                if actual[3] == 0 {
                    assert_eq!(actual, &input[pixel * 4..pixel * 4 + 4]);
                    continue;
                }
                for c in 0..3 {
                    assert!(
                        actual[c].abs_diff(expected[c]) <= 1,
                        "case {index} pixel {pixel} channel {c}: {} != {}",
                        actual[c],
                        expected[c]
                    );
                }
            }
        }
    }
    #[test]
    fn constant_alpha_and_hidden_rgb() {
        for amount in [-100.0, 0.0, 100.0] {
            let mut f = GroupSurface {
                width: 3,
                height: 2,
                rgba: [72, 131, 207, 128].repeat(6),
            };
            let original = f.rgba.clone();
            ColorSharpen { amount }
                .apply(&mut f, &mut || Ok(()))
                .unwrap();
            assert_eq!(f.rgba, original);
        }
        let mut f = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![12, 34, 56, 0],
        };
        ColorSharpen { amount: 100.0 }
            .apply(&mut f, &mut || Ok(()))
            .unwrap();
        assert_eq!(f.rgba, vec![12, 34, 56, 0]);
    }
    #[test]
    fn admission_and_cancel() {
        assert!(ColorSharpen { amount: f32::NAN }.validate().is_err());
        let mut f = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![1, 2, 3, 128],
        };
        assert!(ColorSharpen { amount: 100.0 }
            .apply(&mut f, &mut || anyhow::bail!("cancel"))
            .is_err());
        assert_eq!(f.rgba, vec![1, 2, 3, 128]);
    }
}
