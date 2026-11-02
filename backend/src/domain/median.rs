//! Filmcraft Median, evaluated on premultiplied linear RGBA at source scale.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Median {
    pub radius: f32,
}
impl Median {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.radius.is_finite() && (0.0..=100.0).contains(&self.radius), "Median radius out of range");
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        checkpoint()?;
        // Original CPU dispatch rounds then caps the effective radius at four.
        let radius = self.radius.round().clamp(0.0, 4.0) as isize;
        if radius == 0 { return Ok(()); }
        let (width, height) = frame.dimensions();
        let source = frame.pixels().to_vec();
        let mut index = 0;
        let mut window = Vec::with_capacity(((radius * 2 + 1).pow(2)) as usize);
        frame.map_premultiplied(&mut |_| {
            let x = (index % width) as isize;
            let y = (index / width) as isize;
            index += 1;
            std::array::from_fn(|channel| {
                window.clear();
                for dy in -radius..=radius {
                    for dx in -radius..=radius {
                        let sx = (x + dx).clamp(0, width as isize - 1) as usize;
                        let sy = (y + dy).clamp(0, height as isize - 1) as usize;
                        window.push(source[sy * width + sx][channel]);
                    }
                }
                let middle = window.len() / 2;
                window.select_nth_unstable_by(middle, |a, b| a.total_cmp(b));
                window[middle]
            })
        }, checkpoint)
    }
}

pub fn validate_animation(animation: &super::composition::AnimatableValue) -> Result<()> {
    animation.validate()?;
    ensure!(super::composition::animatable_values_in(animation, 0.0, 100.0, true), "Median animation out of range");
    Ok(())
}
pub fn sample_animation(mut parameters: Median, animation: &super::composition::AnimatableValue, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<Median> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;
    validate_animation(animation)?;
    parameters.radius = super::nested_composition::sample_value(animation, time)? as f32;
    parameters.validate()?;
    Ok(parameters)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{group_surface::GroupSurface, linear_color_frame::LinearColorFrame};
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case { parameters: Median, width: usize, height: usize, rgba: Vec<u8>, bits: Vec<u32> }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-median.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(),32);
        for (index, case) in oracle.cases.into_iter().enumerate() {
            let source = GroupSurface {width:case.width,height:case.height,rgba:case.rgba};
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame, &mut || Ok(())).unwrap();
            let bits: Vec<u32> = frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original Median frame {index}");
        }
    }
    #[test]
    fn persisted_radius_curve_uses_local_clock_and_checks_range() {
        let effect: crate::domain::composition::VideoEffect = serde_json::from_value(serde_json::json!({
            "kind":"median", "parameters":{"radius":1}, "animation":{
                "mode":"keyframes", "track":{"timeBase":1000000,"interpolation":"linear",
                "keyframes":[{"tick":0,"value":0},{"tick":500000,"value":4}]}}
        })).unwrap();
        let restored = serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
        assert_eq!(effect, restored);
        let crate::domain::composition::VideoEffect::Median {parameters, animation:Some(curve)} = effect else {panic!("Median")};
        assert_eq!(sample_animation(parameters, &curve, 250000).unwrap().radius, 2.0);
        assert!(Median {radius:101.0}.validate().is_err());
    }
    #[test]
    fn thin_frame_repeats_edges_and_filters_alpha() {
        let source = GroupSurface { width: 1, height: 3,
            rgba: vec![255,0,0,255, 0,255,0,128, 0,0,255,0] };
        let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
        let original = frame.pixels().to_vec();
        Median { radius: 1.0 }.apply_linear(&mut frame, &mut || Ok(())).unwrap();
        assert_eq!(frame.pixels()[0], original[0]);
        assert_eq!(frame.pixels()[1], [0.0,0.0,0.0,128.0/255.0]);
        assert_eq!(frame.pixels()[2], original[2]);
    }
    #[test]
    fn impulse_removed_rounding_cap_and_cancellation() {
        let mut rgba = vec![0; 3 * 3 * 4];
        for p in rgba.chunks_exact_mut(4) { p[3] = 255; }
        rgba[16..20].copy_from_slice(&[255, 255, 255, 255]);
        let source = GroupSurface { width: 3, height: 3, rgba };
        let original = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
        let mut no_op = original.clone();
        Median { radius: 0.49 }.apply_linear(&mut no_op, &mut || Ok(())).unwrap();
        assert_eq!(original.pixels(), no_op.pixels());
        let mut filtered = original.clone();
        Median { radius: 0.5 }.apply_linear(&mut filtered, &mut || Ok(())).unwrap();
        assert!(filtered.pixels().iter().all(|p| *p == [0.0, 0.0, 0.0, 1.0]));
        let mut capped = original.clone();
        let mut maximum = original.clone();
        Median { radius: 4.0 }.apply_linear(&mut capped, &mut || Ok(())).unwrap();
        Median { radius: 100.0 }.apply_linear(&mut maximum, &mut || Ok(())).unwrap();
        assert_eq!(capped.pixels(), maximum.pixels());
        assert!(Median { radius: 1.0 }.apply_linear(&mut filtered, &mut || anyhow::bail!("cancelled")).is_err());
        assert!(Median { radius: f32::NAN }.validate().is_err());
    }
}
