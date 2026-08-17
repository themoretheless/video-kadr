use std::fmt;

use serde::{Deserialize, Serialize};

/// Output-timeline keyframes arrive in seconds. The domain stores millisecond
/// ticks so track equality and plan fingerprints stay exact.
pub const OUTPUT_TIME_BASE: u32 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    Hold,
    Linear,
    EaseInOutCubic,
}

impl Interpolation {
    /// Map the wire token. `smooth` is the client-facing name of the cubic ease.
    pub fn from_wire(token: &str) -> Option<Self> {
        match token {
            "hold" => Some(Self::Hold),
            "linear" => Some(Self::Linear),
            "smooth" => Some(Self::EaseInOutCubic),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Keyframe<T> {
    pub tick: u64,
    pub value: T,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyframeTrack<T> {
    pub time_base: u32,
    pub interpolation: Interpolation,
    pub keyframes: Vec<Keyframe<T>>,
}

pub trait KeyframeValue {
    fn is_valid(&self) -> bool;
}

impl KeyframeValue for f64 {
    fn is_valid(&self) -> bool {
        self.is_finite()
    }
}

impl KeyframeValue for f32 {
    fn is_valid(&self) -> bool {
        self.is_finite()
    }
}

impl<const N: usize> KeyframeValue for [f64; N] {
    fn is_valid(&self) -> bool {
        self.iter().all(|value| value.is_finite())
    }
}

impl KeyframeValue for bool {
    fn is_valid(&self) -> bool {
        true
    }
}

impl<T: KeyframeValue> KeyframeTrack<T> {
    pub fn new(
        time_base: u32,
        interpolation: Interpolation,
        mut keyframes: Vec<Keyframe<T>>,
    ) -> Result<Self, KeyframeError> {
        if time_base == 0 {
            return Err(KeyframeError::InvalidTimeBase);
        }
        if keyframes.is_empty() {
            return Err(KeyframeError::EmptyTrack);
        }
        if !keyframes.iter().all(|keyframe| keyframe.value.is_valid()) {
            return Err(KeyframeError::NonFinite);
        }
        keyframes.sort_by_key(|keyframe| keyframe.tick);
        if keyframes
            .windows(2)
            .any(|window| window[0].tick == window[1].tick)
        {
            return Err(KeyframeError::DuplicateTick);
        }
        Ok(Self {
            time_base,
            interpolation,
            keyframes,
        })
    }
}

impl KeyframeTrack<f64> {
    pub fn sample_tick(&self, tick: u64) -> f64 {
        let first = &self.keyframes[0];
        if tick <= first.tick {
            return first.value;
        }
        let last = self.keyframes.last().expect("non-empty track");
        if tick >= last.tick {
            return last.value;
        }
        let next_index = self
            .keyframes
            .partition_point(|keyframe| keyframe.tick <= tick);
        let left = &self.keyframes[next_index - 1];
        let right = &self.keyframes[next_index];
        let progress = (tick - left.tick) as f64 / (right.tick - left.tick) as f64;
        let eased = match self.interpolation {
            Interpolation::Hold => 0.0,
            Interpolation::Linear => progress,
            Interpolation::EaseInOutCubic if progress < 0.5 => 4.0 * progress.powi(3),
            Interpolation::EaseInOutCubic => 1.0 - (-2.0 * progress + 2.0).powi(3) / 2.0,
        };
        left.value + (right.value - left.value) * eased
    }

    pub fn sample_seconds(&self, seconds: f64) -> Result<f64, KeyframeError> {
        if !seconds.is_finite() {
            return Err(KeyframeError::NonFinite);
        }
        let tick = (seconds.max(0.0) * self.time_base as f64).round() as u64;
        Ok(self.sample_tick(tick))
    }
}

pub struct PreviewKeyframeAdapter<'a> {
    track: &'a KeyframeTrack<f64>,
}

impl<'a> PreviewKeyframeAdapter<'a> {
    pub fn new(track: &'a KeyframeTrack<f64>) -> Self {
        Self { track }
    }

    pub fn sample(&self, seconds: f64) -> Result<f64, KeyframeError> {
        self.track.sample_seconds(seconds)
    }
}

pub struct FfmpegKeyframeAdapter<'a> {
    track: &'a KeyframeTrack<f64>,
}

impl<'a> FfmpegKeyframeAdapter<'a> {
    pub fn new(track: &'a KeyframeTrack<f64>) -> Self {
        Self { track }
    }

    /// Produce a deterministic expression for ffmpeg filters that accept an
    /// expression over `t`. Cubic easing remains a domain concern and is
    /// expanded into arithmetic instead of leaking preview implementation.
    pub fn expression(&self, time_variable: &str) -> Result<String, KeyframeError> {
        if time_variable.is_empty()
            || !time_variable
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(KeyframeError::InvalidVariable);
        }
        let mut expression = format_number(self.track.keyframes.last().unwrap().value);
        for window in self.track.keyframes.windows(2).rev() {
            let left = &window[0];
            let right = &window[1];
            let start = left.tick as f64 / self.track.time_base as f64;
            let end = right.tick as f64 / self.track.time_base as f64;
            let progress = format!(
                "(({time_variable}-{})/{})",
                format_number(start),
                format_number(end - start)
            );
            let eased = match self.track.interpolation {
                Interpolation::Hold => "0".to_owned(),
                Interpolation::Linear => progress,
                Interpolation::EaseInOutCubic => {
                    format!("if(lt({progress},0.5),4*pow({progress},3),1-pow(-2*{progress}+2,3)/2)")
                }
            };
            let segment = format!(
                "{}+({}-{})*{}",
                format_number(left.value),
                format_number(right.value),
                format_number(left.value),
                eased
            );
            expression = format!(
                "if(lt({time_variable},{}),{segment},{expression})",
                format_number(end)
            );
        }
        let first = &self.track.keyframes[0];
        let first_time = first.tick as f64 / self.track.time_base as f64;
        Ok(format!(
            "if(lt({time_variable},{}),{},{expression})",
            format_number(first_time),
            format_number(first.value)
        ))
    }
}

fn format_number(value: f64) -> String {
    let value = if value == -0.0 { 0.0 } else { value };
    let mut formatted = format!("{value:.6}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyframeError {
    InvalidTimeBase,
    EmptyTrack,
    DuplicateTick,
    NonFinite,
    InvalidVariable,
}

impl fmt::Display for KeyframeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid keyframe track: {self:?}")
    }
}

impl std::error::Error for KeyframeError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(interpolation: Interpolation) -> KeyframeTrack<f64> {
        KeyframeTrack::new(
            1_000,
            interpolation,
            vec![
                Keyframe {
                    tick: 0,
                    value: 0.0,
                },
                Keyframe {
                    tick: 1_000,
                    value: 10.0,
                },
                Keyframe {
                    tick: 2_000,
                    value: 20.0,
                },
            ],
        )
        .unwrap()
    }

    #[test]
    fn sampling_is_deterministic_at_boundaries_and_between_frames() {
        let linear_track = track(Interpolation::Linear);
        let preview = PreviewKeyframeAdapter::new(&linear_track);
        assert_eq!(preview.sample(0.5).unwrap(), 5.0);
        assert_eq!(preview.sample(1.0).unwrap(), 10.0);
        assert_eq!(preview.sample(9.0).unwrap(), 20.0);
        assert_eq!(track(Interpolation::Hold).sample_tick(999), 0.0);
    }

    #[test]
    fn ffmpeg_adapter_has_stable_golden_expression() {
        let expression = FfmpegKeyframeAdapter::new(&track(Interpolation::Linear))
            .expression("t")
            .unwrap();
        assert_eq!(
            expression,
            "if(lt(t,0),0,if(lt(t,1),0+(10-0)*((t-0)/1),if(lt(t,2),10+(20-10)*((t-1)/1),20)))"
        );
    }

    #[test]
    fn rejects_non_finite_values_at_the_domain_boundary() {
        assert_eq!(
            KeyframeTrack::new(
                1_000,
                Interpolation::Linear,
                vec![Keyframe {
                    tick: 0,
                    value: f64::NAN,
                }],
            ),
            Err(KeyframeError::NonFinite)
        );
    }
}
