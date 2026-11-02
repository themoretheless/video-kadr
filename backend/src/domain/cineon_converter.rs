//! Cineon Converter source math from pinned Filmcraft render/vfx/color.rs (MIT).
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CineonConverter {
    pub conversion: u32,
    pub black10: f32,
    pub white10: f32,
    pub black_internal: f32,
    pub white_internal: f32,
    pub gamma: f32,
    pub rolloff: f32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CineonChannel {
    Black10,
    White10,
    BlackInternal,
    WhiteInternal,
    Gamma,
    Rolloff,
}
pub type CineonAnimation =
    std::collections::BTreeMap<CineonChannel, super::composition::AnimatableValue>;
impl CineonChannel {
    fn bounds(self) -> (f64, f64) {
        match self {
            Self::Black10 | Self::White10 => (0.0, 1023.0),
            Self::BlackInternal | Self::WhiteInternal | Self::Rolloff => (0.0, 255.0),
            Self::Gamma => (0.1, 5.0),
        }
    }
}
pub fn validate_animation(animation: &CineonAnimation) -> Result<()> {
    for (channel, value) in animation {
        value.validate()?;
        let (min, max) = channel.bounds();
        ensure!(
            super::composition::animatable_values_in(value, min, max, true),
            "Cineon animation parameter out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut parameters: CineonConverter,
    animation: &CineonAnimation,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<CineonConverter> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;
    validate_animation(animation)?;
    for (channel, value) in animation {
        let sampled = super::nested_composition::sample_value(value, time)? as f32;
        *match channel {
            CineonChannel::Black10 => &mut parameters.black10,
            CineonChannel::White10 => &mut parameters.white10,
            CineonChannel::BlackInternal => &mut parameters.black_internal,
            CineonChannel::WhiteInternal => &mut parameters.white_internal,
            CineonChannel::Gamma => &mut parameters.gamma,
            CineonChannel::Rolloff => &mut parameters.rolloff,
        } = sampled;
    }
    parameters.validate()?;
    Ok(parameters)
}
impl Default for CineonConverter {
    fn default() -> Self {
        Self {
            conversion: 0,
            black10: 95.0,
            white10: 685.0,
            black_internal: 0.0,
            white_internal: 255.0,
            gamma: 1.7,
            rolloff: 20.0,
        }
    }
}
impl CineonConverter {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.conversion <= 2, "unknown Cineon conversion mode");
        for (value, min, max) in [
            (self.black10, 0.0, 1023.0),
            (self.white10, 0.0, 1023.0),
            (self.black_internal, 0.0, 255.0),
            (self.white_internal, 0.0, 255.0),
            (self.gamma, 0.1, 5.0),
            (self.rolloff, 0.0, 255.0),
        ] {
            ensure!(
                value.is_finite() && value >= min && value <= max,
                "Cineon parameter out of range"
            );
        }
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        frame.map_signal(
            &mut |input, _, _| Ok(input.map(|value| self.signal(value))),
            checkpoint,
        )
    }
    pub fn signal(&self, value: f32) -> f32 {
        cineon_value(
            value,
            self.conversion,
            self.black10,
            self.white10.max(self.black10 + 1.0),
            self.black_internal / 255.0,
            self.white_internal / 255.0,
            self.gamma.max(0.1),
            self.rolloff / 255.0,
        )
    }
}
#[allow(clippy::too_many_arguments)]
fn cineon_value(
    v: f32,
    mode: u32,
    black: f32,
    white: f32,
    ib: f32,
    iw: f32,
    gamma: f32,
    roll: f32,
) -> f32 {
    let k = 0.002 / 0.6;
    let offset = 10f32.powf((black - white) * k);
    match mode {
        0 => {
            let code = v * 1023.0;
            let linv = ((10f32.powf((code - white) * k) - offset) / (1.0 - offset)).max(0.0);
            let mut o = linv.powf(1.0 / gamma);
            o = ib + o * (iw - ib);
            let r0 = 1.0 - roll;
            if roll > 1e-4 && o > r0 {
                o = r0 + roll * (1.0 - (-(o - r0) / roll).exp());
            }
            o.clamp(0.0, 1.0)
        }
        1 => {
            let d = ((v - ib) / (iw - ib).max(1e-4)).clamp(0.0, 1.0);
            let linv = d.powf(gamma);
            let code = white + (linv * (1.0 - offset) + offset).max(1e-6).log10() / k;
            (code / 1023.0).clamp(0.0, 1.0)
        }
        _ => {
            let code = v * 1023.0;
            (((code - black) / (white - black)) * (685.0 - 95.0) + 95.0) / 1023.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_modes_match_independently_compiled_source_renderer_bits() {
        #[derive(Deserialize)]
        struct Case {
            parameters: CineonConverter,
            input: [f32; 3],
            bits: [u32; 3],
        }
        #[derive(Deserialize)]
        struct Oracle {
            commit: String,
            cases: Vec<Case>,
        }
        let oracle: Oracle =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-cineon.json"))
                .unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(), 396);
        for (index, case) in oracle.cases.iter().enumerate() {
            case.parameters.validate().unwrap();
            let actual = case.input.map(|value| {
                super::super::basic_grade::linear(
                    case.parameters
                        .signal(super::super::basic_grade::display(value.max(0.0)))
                        .clamp(0.0, 1.0),
                )
                .to_bits()
            });
            assert_eq!(
                actual, case.bits,
                "source case {index}: {:?}",
                case.parameters
            );
        }
    }
    #[test]
    fn linear_frame_preserves_alpha_and_rejects_source_nan() {
        use super::super::{group_surface::GroupSurface, linear_color_frame::LinearColorFrame};
        let image = GroupSurface {
            width: 2,
            height: 1,
            rgba: vec![64, 128, 192, 128, 11, 22, 33, 0],
        };
        let mut frame = LinearColorFrame::from_surface(&image, &mut || Ok(())).unwrap();
        let converter = CineonConverter::default();
        let before = frame.pixels().to_vec();
        converter.apply_linear(&mut frame, &mut || Ok(())).unwrap();
        assert_eq!(frame.pixels()[0][3].to_bits(), before[0][3].to_bits());
        assert_eq!(frame.pixels()[1], before[1]);
        for channel in 0..3 {
            let alpha = before[0][3];
            let expected = super::super::basic_grade::linear(
                converter
                    .signal(super::super::basic_grade::display(
                        before[0][channel] / alpha,
                    ))
                    .clamp(0.0, 1.0),
            ) * alpha;
            assert_eq!(frame.pixels()[0][channel].to_bits(), expected.to_bits());
        }
        let pathological = CineonConverter {
            black10: 0.0,
            white10: 0.0,
            black_internal: 0.0,
            white_internal: 0.0,
            gamma: 0.1,
            rolloff: 0.0,
            conversion: 0,
        };
        pathological.validate().unwrap();
        assert!(pathological.signal(1.0).is_nan());
        let mut white = LinearColorFrame::from_surface(
            &GroupSurface {
                width: 1,
                height: 1,
                rgba: vec![255; 4],
            },
            &mut || Ok(()),
        )
        .unwrap();
        assert!(
            pathological
                .apply_linear(&mut white, &mut || Ok(()))
                .unwrap_err()
                .to_string()
                .contains("nonfinite grading signal")
        );
        assert!(
            converter
                .apply_linear(&mut frame, &mut || anyhow::bail!("cancelled"))
                .is_err()
        );
    }
    #[test]
    fn animation_samples_all_source_scalar_channels_and_persists() {
        let mut fields = serde_json::Map::new();
        for (name, low, high) in [
            ("black10", 0.0, 1023.0),
            ("white10", 0.0, 1023.0),
            ("blackInternal", 0.0, 255.0),
            ("whiteInternal", 0.0, 255.0),
            ("gamma", 0.1, 5.0),
            ("rolloff", 0.0, 255.0),
        ] {
            fields.insert(
                name.into(),
                serde_json::json!({"mode":"keyframes", "track":{
                "timeBase":1000000, "interpolation":"linear", "keyframes":[
                    {"tick":0,"value":low}, {"tick":1000000,"value":high}]}}),
            );
        }
        let animation: CineonAnimation = serde_json::from_value(fields.clone().into()).unwrap();
        let base = CineonConverter {
            conversion: 2,
            ..Default::default()
        };
        let sampled = sample_animation(base, &animation, 500000).unwrap();
        assert_eq!(
            sampled,
            CineonConverter {
                conversion: 2,
                black10: 511.5,
                white10: 511.5,
                black_internal: 127.5,
                white_internal: 127.5,
                gamma: 2.55,
                rolloff: 127.5
            }
        );
        assert_eq!(sample_animation(base, &animation, 0).unwrap().gamma, 0.1);
        assert_eq!(
            sample_animation(base, &animation, 1000000).unwrap().gamma,
            5.0
        );
        let effect: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"cineon_converter","parameters":{"conversion":2},"animation":fields})
        ).unwrap();
        assert_eq!(
            serde_json::from_value::<super::super::composition::VideoEffect>(
                serde_json::to_value(&effect).unwrap()
            )
            .unwrap(),
            effect
        );
        assert!(
            serde_json::from_value::<CineonAnimation>(serde_json::json!({
            "conversion":{"mode":"constant","value":1.0}}))
            .is_err()
        );
        let invalid: CineonAnimation = serde_json::from_value(serde_json::json!({
            "gamma":{"mode":"constant","value":0.09}}))
        .unwrap();
        assert!(validate_animation(&invalid).is_err());
        let invalid: CineonAnimation = serde_json::from_value(serde_json::json!({
            "rolloff":{"mode":"constant","value":256.0}}))
        .unwrap();
        assert!(validate_animation(&invalid).is_err());
    }
    #[test]
    fn source_black_white_endpoints_and_modes() {
        let converter = CineonConverter {
            gamma: 1.0,
            rolloff: 0.0,
            ..Default::default()
        };
        assert_eq!(converter.signal(95.0 / 1023.0), 0.0);
        assert!((converter.signal(685.0 / 1023.0) - 1.0).abs() < 1e-6);
        let inverse = CineonConverter {
            conversion: 1,
            ..converter
        };
        assert!((inverse.signal(0.0) - 95.0 / 1023.0).abs() < 1e-6);
        assert!((inverse.signal(1.0) - 685.0 / 1023.0).abs() < 1e-6);
        for input in [0.0, 0.1, 0.5, 0.9, 1.0] {
            let log = inverse.signal(input);
            assert!((converter.signal(log) - input).abs() < 1e-5);
        }
        assert!(
            CineonConverter {
                gamma: f32::NAN,
                ..converter
            }
            .validate()
            .is_err()
        );
    }
}
