//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Concat display-space BT.709 chroma key and directional spill removal.
//! Source contract: pinned concat.chroma-key/effect.wgsl (AGPL-3.0-or-later).
//! Native original-key runtime; expanded original RGB precision qualification remains open.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChromaKey {
    pub color: u32,
    pub similarity: f32,
    pub softness: f32,
    pub spill: f32,
}
impl Default for ChromaKey {
    fn default() -> Self {
        Self {
            color: 0x00b140ff,
            similarity: 30.,
            softness: 10.,
            spill: 50.,
        }
    }
}
fn chroma(c: [f32; 3]) -> [f32; 2] {
    [
        -0.1146 * c[0] - 0.3854 * c[1] + 0.5 * c[2],
        0.5 * c[0] - 0.4542 * c[1] - 0.0458 * c[2],
    ]
}
impl ChromaKey {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.similarity.is_finite() && (1. ..=60.).contains(&self.similarity),
            "Concat chroma similarity out of range"
        );
        ensure!(
            self.softness.is_finite() && (0. ..=50.).contains(&self.softness),
            "Concat chroma softness out of range"
        );
        ensure!(
            self.spill.is_finite() && (0. ..=100.).contains(&self.spill),
            "Concat chroma spill out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        ensure!(
            [self.similarity, self.softness, self.spill]
                .iter()
                .all(|v| v.is_finite()),
            "nonfinite Chroma Key runtime parameters"
        );
        frame.map_concat_straight_pixels(
            &mut |input, _, _| {
                let mut sample = input.map(|v| half::f16::from_f32(v).to_f32());
                for value in &mut sample[..3] {
                    *value = super::concat_chroma::display(*value);
                }
                let mut output = self.display_signal(sample);
                for value in &mut output[..3] {
                    *value = super::concat_working::texture_half(
                        super::concat_chroma::linear(*value).clamp(-65504., 65504.),
                    );
                }
                output[3] = super::concat_working::texture_half(output[3]);
                output
            },
            checkpoint,
        )
    }
    /// Straight display RGB and alpha; hidden RGB remains available at zero alpha.
    pub fn display_signal(&self, input: [f32; 4]) -> [f32; 4] {
        let key = chroma([
            (self.color >> 24) as f32 / 255.,
            ((self.color >> 16) & 255) as f32 / 255.,
            ((self.color >> 8) & 255) as f32 / 255.,
        ]);
        let here = chroma([input[0], input[1], input[2]]);
        let distance = ((here[0] - key[0]).powi(2) + (here[1] - key[1]).powi(2)).sqrt() / 0.7071;
        let similarity = self.similarity / 100.;
        let edge = similarity + (self.softness / 100.).max(0.001);
        let t = ((distance - similarity) / (edge - similarity)).clamp(0., 1.);
        let keep = t * t * (3. - 2. * t);
        let reach = (key[0] * key[0] + key[1] * key[1]).sqrt();
        let mut clean = here;
        if reach > 1e-4 {
            let direction = [key[0] / reach, key[1] / reach];
            let amount = (here[0] * direction[0] + here[1] * direction[1]).max(0.);
            clean = [
                here[0] - (direction[0] * amount) * (self.spill / 100.),
                here[1] - (direction[1] * amount) * (self.spill / 100.),
            ];
        }
        let y = input[0] * 0.2126 + input[1] * 0.7152 + input[2] * 0.0722;
        [
            y + 1.5748 * clean[1],
            y - 0.1873 * clean[0] - 0.4681 * clean[1],
            y + 1.8556 * clean[0],
            input[3] * keep,
        ]
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Channel {
    Color,
    Similarity,
    Softness,
    Spill,
}
impl Channel {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "color" => Some(Self::Color),
            "similarity" => Some(Self::Similarity),
            "softness" => Some(Self::Softness),
            "spill" => Some(Self::Spill),
            _ => None,
        }
    }
    fn range(self) -> (f64, f64) {
        match self {
            Self::Color => (0., u32::MAX as f64),
            Self::Similarity => (1., 60.),
            Self::Softness => (0., 50.),
            Self::Spill => (0., 100.),
        }
    }
}
pub type OriginalKeys =
    std::collections::BTreeMap<Channel, Vec<super::concat_effect_upgrade::ParamKey>>;
pub fn validate_keys(keys: &OriginalKeys) -> Result<()> {
    ensure!(
        keys.values().map(Vec::len).sum::<usize>() <= 4096,
        "too many original Chroma Key keys"
    );
    for (channel, run) in keys {
        let (low, high) = channel.range();
        ensure!(
            run.iter().all(|k| k.at.is_finite()
                && k.value.is_finite()
                && (low..=high).contains(&k.value)
                && k.ease.iter().all(|v| v.is_finite())),
            "invalid original Chroma Key key"
        );
    }
    Ok(())
}
pub fn sample_keys(mut p: ChromaKey, keys: &OriginalKeys, progress: f64) -> Result<ChromaKey> {
    p.validate()?;
    validate_keys(keys)?;
    for (channel, run) in keys {
        if run.is_empty() {
            continue;
        }
        let rest = match channel {
            Channel::Color => p.color as f64,
            Channel::Similarity => p.similarity as f64,
            Channel::Softness => p.softness as f64,
            Channel::Spill => p.spill as f64,
        };
        let value = super::concat_animation::PreparedTrack::new(run)?.value_at(progress, rest)?;
        match channel {
            Channel::Color => p.color = value.max(0.) as u32,
            Channel::Similarity => p.similarity = value as f32,
            Channel::Softness => p.softness = value as f32,
            Channel::Spill => p.spill = value as f32,
        }
    }
    ensure!(
        [p.similarity, p.softness, p.spill]
            .iter()
            .all(|v| v.is_finite()),
        "nonfinite Chroma Key runtime key"
    );
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_alpha_spill_and_neutral_direction() {
        let key = ChromaKey::default();
        key.validate().unwrap();
        let green = [0., 177. / 255., 64. / 255., 0.5];
        assert_eq!(key.display_signal(green)[3], 0.);
        let skin = [0.8, 0.6, 0.5, 0.37];
        assert_eq!(key.display_signal(skin)[3], 0.37);
        let no_spill = ChromaKey { spill: 0., ..key };
        let input = [0.4, 0.6, 0.4, 1.];
        let cleaned = key.display_signal(input);
        let retained = no_spill.display_signal(input);
        assert!(cleaned[1] < retained[1]);
        assert_eq!(cleaned[3], retained[3]);
        for color in [0, 0xffffffff] {
            let neutral = ChromaKey { color, ..key };
            let out = neutral.display_signal([0.4, 0.4, 0.4, 1.]);
            assert_eq!(out[3], 0.);
            assert!(out.iter().all(|v| v.is_finite()));
        }
        assert!(ChromaKey {
            similarity: f32::NAN,
            ..key
        }
        .validate()
        .is_err());
        assert!(ChromaKey {
            softness: 51.,
            ..key
        }
        .validate()
        .is_err());
        assert!(ChromaKey { spill: -1., ..key }.validate().is_err());
    }
    #[test]
    #[ignore = "requires independently generated CHROMA_KEY_GPU_ORACLE"]
    fn frame_alpha_and_hidden_rgb_match_original_metal() {
        use super::super::linear_color_frame::LinearColorFrame;
        let path = std::env::var("CHROMA_KEY_GPU_ORACLE").expect("set CHROMA_KEY_GPU_ORACLE");
        let oracle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            oracle["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        let mut values = 0;
        let mut mismatches = [0usize; 4];
        for (index, case) in oracle["cases"].as_array().unwrap().iter().enumerate() {
            let p: ChromaKey = serde_json::from_value(case["settings"].clone()).unwrap();
            let input: [f32; 4] =
                std::array::from_fn(|i| case["input"][i].as_f64().unwrap() as f32);
            let mut frame = LinearColorFrame::from_working(
                1,
                1,
                &[input[0], input[1], input[2], 1.],
                &mut || Ok(()),
            )
            .unwrap();
            frame
                .map_concat_straight_pixels(
                    &mut |mut p, _, _| {
                        p[3] = input[3];
                        p
                    },
                    &mut || Ok(()),
                )
                .unwrap();
            let effect: super::super::composition::VideoEffect = serde_json::from_value(
                serde_json::json!({"kind":"concat_chroma_key","parameters":p}),
            )
            .unwrap();
            effect.validate().unwrap();
            super::super::flat_surface::apply_effect_stack_working(
                &mut frame,
                &[effect],
                0,
                &[],
                None,
                &mut || Ok(()),
            )
            .unwrap();
            let mut actual = [0.; 4];
            frame
                .map_concat_straight_pixels(
                    &mut |p, _, _| {
                        actual = p;
                        p
                    },
                    &mut || Ok(()),
                )
                .unwrap();
            for channel in 0..4 {
                let expected = case["gpu"][channel].as_f64().unwrap() as f32;
                if actual[channel].to_bits() != expected.to_bits() {
                    mismatches[channel] += 1;
                    eprintln!(
                        "Chroma Key mismatch case{index} channel{channel}: {} != {expected}",
                        actual[channel]
                    );
                }
                values += 1;
            }
        }
        eprintln!("Chroma Key actual frame: {values} channels compared, mismatches {mismatches:?}");
        assert_eq!(
            mismatches, [0; 4],
            "original bitwise frame gate remains strict"
        );
    }
    #[test]
    fn alpha_map_cancellation_and_invalid_output_preserve_frame() {
        use super::super::linear_color_frame::LinearColorFrame;
        fn snapshot(frame: &mut LinearColorFrame) -> Vec<[f32; 4]> {
            let mut result = Vec::new();
            frame
                .map_concat_straight_pixels(
                    &mut |p, _, _| {
                        result.push(p);
                        p
                    },
                    &mut || Ok(()),
                )
                .unwrap();
            result
        }
        let mut frame =
            LinearColorFrame::from_working(2, 2, &[0.2, 0.4, 0.1, 1.].repeat(4), &mut || Ok(()))
                .unwrap();
        frame
            .map_concat_straight_pixels(
                &mut |mut p, x, y| {
                    if x == 0 && y == 0 {
                        p[3] = 0.;
                    }
                    p
                },
                &mut || Ok(()),
            )
            .unwrap();
        let before = snapshot(&mut frame);
        let mut checks = 0;
        assert!(frame
            .map_concat_straight_pixels(
                &mut |mut p, _, _| {
                    p[0] += 0.1;
                    p[3] = 0.;
                    p
                },
                &mut || {
                    checks += 1;
                    ensure!(checks < 2, "cancelled between rows");
                    Ok(())
                }
            )
            .is_err());
        assert_eq!(snapshot(&mut frame), before);
        assert!(frame
            .map_concat_straight_pixels(&mut |_, _, _| [f32::NAN, 0., 0., 0.], &mut || Ok(()))
            .is_err());
        assert_eq!(snapshot(&mut frame), before);
        frame
            .map_concat_straight_pixels(
                &mut |mut p, _, _| {
                    p[3] = 0.;
                    p
                },
                &mut || Ok(()),
            )
            .unwrap();
        let hidden = snapshot(&mut frame);
        assert!(hidden.iter().all(|p| p[3] == 0. && p[1] > 0.));
    }
    #[test]
    fn alpha_scratch_admission_is_bounded_without_large_allocations() {
        use super::super::linear_color_frame::LinearColorFrame;
        assert!(LinearColorFrame::concat_alpha_scratch_bytes(3840 * 2160).is_ok());
        let max = (1024 * 1024 * 1024 - 256) / 80;
        assert!(LinearColorFrame::concat_alpha_scratch_bytes(max).is_ok());
        assert!(LinearColorFrame::concat_alpha_scratch_bytes(max + 1).is_err());
        assert!(LinearColorFrame::concat_alpha_scratch_bytes(usize::MAX).is_err());
    }

    #[test]
    #[ignore = "requires independent CONCAT_EFFECT_UPGRADE_ORACLE chromaParameterCases"]
    fn original_key_uniform_bytes_match() {
        let path = std::env::var("CONCAT_EFFECT_UPGRADE_ORACLE")
            .expect("set CONCAT_EFFECT_UPGRADE_ORACLE");
        let oracle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            oracle["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        let cases = oracle["chromaParameterCases"].as_array().unwrap();
        for (index, case) in cases.iter().enumerate() {
            let original = super::super::concat_effect_upgrade::upgrade(
                serde_json::from_value(case["input"].clone()).unwrap(),
            )
            .unwrap();
            let Some(super::super::composition::VideoEffect::ConcatChromaKey {
                parameters,
                concat_keys,
            }) = super::super::concat_effect_upgrade::native_effect(&original.link).unwrap()
            else {
                panic!("Chroma native admission")
            };
            let p = sample_keys(
                parameters,
                &concat_keys.unwrap_or_default(),
                case["progress"].as_f64().unwrap(),
            )
            .unwrap();
            let values = [
                (p.color >> 24) as f32 / 255.,
                ((p.color >> 16) & 255) as f32 / 255.,
                ((p.color >> 8) & 255) as f32 / 255.,
                (p.color & 255) as f32 / 255.,
                p.similarity,
                p.softness,
                p.spill,
            ];
            let bytes: Vec<u8> = values.into_iter().flat_map(f32::to_le_bytes).collect();
            let expected: Vec<u8> = serde_json::from_value(case["uniformPrefix"].clone()).unwrap();
            assert_eq!(bytes, expected, "case{index}");
        }
        eprintln!("Original Chroma Key uniform packing: {} cases", cases.len());
    }
}
