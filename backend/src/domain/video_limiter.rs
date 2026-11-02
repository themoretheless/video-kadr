//! Filmcraft Video Limiter math from pinned render/vfx/color.rs (MIT).
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VideoLimiter {
    pub clip_level: u32,
    pub compression: u32,
    pub axis: u32,
    pub gamut_warning: bool,
    pub warning_color: [f32; 4],
}
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum WarningChannel { Red, Green, Blue, Alpha }
pub type VideoLimiterAnimation=std::collections::BTreeMap<WarningChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&VideoLimiterAnimation)->Result<()> {
    for value in animation.values() {
        value.validate()?;
        ensure!(super::composition::animatable_values_in(value,0.0,1.0,true),"invalid Video Limiter color animation");
    }
    Ok(())
}
pub fn sample_animation(mut parameters:VideoLimiter,animation:&VideoLimiterAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<VideoLimiter> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    validate_animation(animation)?;
    for (channel,value) in animation {
        let index=match channel {WarningChannel::Red=>0,WarningChannel::Green=>1,WarningChannel::Blue=>2,WarningChannel::Alpha=>3};
        parameters.warning_color[index]=super::nested_composition::sample_value(value,time)? as f32;
    }
    parameters.validate()?;
    Ok(parameters)
}
impl Default for VideoLimiter {
    fn default() -> Self {
        Self {
            clip_level: 0,
            compression: 1,
            axis: 3,
            gamut_warning: false,
            warning_color: [1.0, 0.0, 0.0, 1.0],
        }
    }
}
impl VideoLimiter {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.clip_level <= 9 && self.compression <= 4 && self.axis <= 3,
            "invalid Video Limiter choice"
        );
        ensure!(
            self.warning_color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            "invalid Video Limiter warning color"
        );
        Ok(())
    }
    pub fn apply(&self, linear: [f32; 3]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            linear.iter().all(|v| v.is_finite()),
            "nonfinite Video Limiter input"
        );
        let input = linear.map(|v| {
            if v <= 1.0 {
                super::basic_grade::display(v.max(0.0))
            } else {
                1.0 + (v - 1.0) / 2.4
            }
        });
        let output = limit(
            input,
            1.0 + self.clip_level as f32 / 100.0,
            [0.0, 0.03, 0.05, 0.10, 0.20][self.compression as usize],
            self.axis,
        );
        if self.gamut_warning && input.iter().zip(output).any(|(a, b)| (*a - b).abs() > 1e-4) {
            return Ok([
                self.warning_color[0],
                self.warning_color[1],
                self.warning_color[2],
            ]
            .map(super::basic_grade::linear));
        }
        Ok(output.map(|v| {
            if v <= 1.0 {
                super::basic_grade::linear(v.max(0.0))
            } else {
                1.0 + (v - 1.0) * 2.4
            }
        }))
    }
}
fn knee(v: f32, max: f32, comp: f32) -> f32 {
    let k = max * (1.0 - comp);
    if comp <= 0.0 || v <= k {
        return v.min(max);
    }
    let r = max - k;
    k + r * (1.0 - (-(v - k) / r).exp())
}

fn limit(v: [f32; 3], max: f32, comp: f32, axis: u32) -> [f32; 3] {
    let y = 0.2126f32 * v[0] + (1.0 - 0.2126f32 - 0.0722f32) * v[1] + 0.0722f32 * v[2];
    let ycc = [
        y,
        (v[2] - y) / (2.0 * (1.0 - 0.0722f32)),
        (v[0] - y) / (2.0 * (1.0 - 0.2126f32)),
    ];
    let (kr, kb) = (0.2126f32, 0.0722f32);
    let to_rgb = |y: f32, cb: f32, cr: f32| {
        let r = y + 2.0 * (1.0 - kr) * cr;
        let b = y + 2.0 * (1.0 - kb) * cb;
        let g = (y - kr * r - kb * b) / (1.0 - kr - kb);
        [r, g, b]
    };
    let (mut y, mut cb, mut cr) = (ycc[0], ycc[1], ycc[2]);
    if axis != 1 {
        y = knee(y.max(0.0), max, comp);
    }
    if axis != 0 {
        // scale chroma so every channel fits 0…max (the gamut of legal R′G′B′)
        let rgb = to_rgb(y, cb, cr);
        let mut s = 1.0f32;
        for &ch in &rgb {
            let d = ch - y;
            if ch > max && d > 1e-6 {
                s = s.min(((max - y) / d).max(0.0));
            }
            if ch < 0.0 && d < -1e-6 {
                s = s.min((y / -d).max(0.0));
            }
        }
        if axis == 3 && s < 1.0 {
            // smart limit: soft chroma compression rather than a hard cut
            s = knee(s, 1.0, comp.max(0.03)).min(1.0);
        }
        cb *= s;
        cr *= s;
    }
    to_rgb(y, cb, cr).map(|q| q.clamp(0.0, max))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_modes_match_independently_compiled_source_renderer_bits() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-video-limiter.json")).unwrap();
        assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases=fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(),2400);
        for (index,case) in cases.iter().enumerate() {
            let parameters:VideoLimiter=serde_json::from_value(case["parameters"].clone()).unwrap();
            let input:[f32;3]=serde_json::from_value(case["input"].clone()).unwrap();
            let output=parameters.apply(input).unwrap();
            for channel in 0..3 {
                assert_eq!(output[channel].to_bits(),case["bits"][channel].as_u64().unwrap() as u32,"case{index} channel{channel} parameters{parameters:?} input{input:?}");
            }
        }
    }
    #[test]
    fn warning_color_animation_keeps_static_choices_and_source_bounds() {
        let animation:VideoLimiterAnimation=serde_json::from_value(serde_json::json!({"green":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":1000000,"value":1.0}]}},"red":{"mode":"constant","value":0.25}})).unwrap();
        let parameters=VideoLimiter{clip_level:9,gamut_warning:true,..Default::default()};
        let sampled=sample_animation(parameters,&animation,500000).unwrap();
        assert_eq!(sampled.warning_color,[0.25,0.5,0.0,1.0]);
        assert_eq!(sampled.clip_level,9);
        let invalid:VideoLimiterAnimation=serde_json::from_value(serde_json::json!({"green":{"mode":"constant","value":1.1}})).unwrap();
        assert!(validate_animation(&invalid).is_err());
        assert!(serde_json::from_value::<VideoLimiterAnimation>(serde_json::json!({"axis":{"mode":"constant","value":1.0}})).is_err());
    }
    #[test]
    fn persisted_effect_roundtrip_and_bounds() {
        use super::super::composition::VideoEffect;
        let effect:VideoEffect=serde_json::from_value(serde_json::json!({"kind":"video_limiter","parameters":{"clipLevel":9,"compression":4,"axis":3,"gamutWarning":true,"warningColor":[0.0,1.0,0.0,0.5]}})).unwrap();
        let VideoEffect::VideoLimiter {parameters,..}=&effect else {panic!("wrong effect")};
        parameters.validate().unwrap();
        assert_eq!(serde_json::from_value::<VideoEffect>(serde_json::to_value(&effect).unwrap()).unwrap(),effect);
        let invalid:VideoEffect=serde_json::from_value(serde_json::json!({"kind":"video_limiter","parameters":{"clipLevel":10}})).unwrap();
        let VideoEffect::VideoLimiter {parameters,..}=invalid else {panic!("wrong effect")};
        assert!(parameters.validate().is_err());
        assert!(serde_json::from_value::<VideoEffect>(serde_json::json!({"kind":"video_limiter","parameters":{"bogus":1}})).is_err());
    }
    #[test]
    fn superwhites_gamut_warning_and_domains() {
        let limiter = VideoLimiter {
            compression: 0,
            axis: 2,
            ..Default::default()
        };
        let out = limiter.apply([4.0, 4.0, 4.0]).unwrap();
        assert!(out.iter().all(|v| (*v - 1.0).abs() < 1e-6));
        let raised = VideoLimiter {
            clip_level: 9,
            ..limiter
        };
        assert!(
            raised
                .apply([4.0, 4.0, 4.0])
                .unwrap()
                .iter()
                .all(|v| *v > 1.0)
        );
        let warning = VideoLimiter {
            gamut_warning: true,
            ..limiter
        };
        assert_eq!(warning.apply([4.0, 4.0, 4.0]).unwrap(), [1.0, 0.0, 0.0]);
        assert!(limiter.apply([f32::NAN, 0.0, 0.0]).is_err());
        assert!(VideoLimiter { axis: 4, ..limiter }.validate().is_err());
    }
}
