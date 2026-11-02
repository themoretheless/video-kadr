fn bounce_out(mut t: f64) -> f64 {
    if t < 1.0 / 2.75 {
        return 7.5625 * t * t;
    }
    if t < 2.0 / 2.75 {
        t -= 1.5 / 2.75;
        return 7.5625 * t * t + 0.75;
    }
    if t < 2.5 / 2.75 {
        t -= 2.25 / 2.75;
        return 7.5625 * t * t + 0.9375;
    }
    t -= 2.625 / 2.75;
    7.5625 * t * t + 0.984375
}

fn bounce_expression(t: &str) -> String {
    format!(
        "if(lt({t},1/2.75),7.5625*{t}*{t},if(lt({t},2/2.75),7.5625*pow({t}-1.5/2.75,2)+0.75,if(lt({t},2.5/2.75),7.5625*pow({t}-2.25/2.75,2)+0.9375,7.5625*pow({t}-2.625/2.75,2)+0.984375)))"
    )
}

use super::bezier::TimingBezier;
use super::elastic::{Elastic, ElasticDirection};
use super::tangents::{Continuity, Tangent, TangentKey, TangentMode, solve_auto_tangents};
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    /// Keep the left value until the next keyframe tick.
    Hold,
    /// Linear progress `p` over the segment.
    Linear,
    /// Cubic ease-in, `p^3`.
    EaseIn,
    /// Cubic ease-out, `1 - (1 - p)^3`.
    EaseOut,
    /// Symmetric cubic ease-in/out.
    EaseInOut,
    /// Legacy wire token retained for saved-project compatibility. It is
    /// semantically identical to `EaseInOut`.
    EaseInOutCubic,
    BounceIn,
    BounceOut,
    BounceInOut,
    #[serde(untagged)]
    Procedural(ProceduralInterpolation),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProceduralInterpolation {
    Bezier {
        p1: [f64; 2],
        p2: [f64; 2],
    },
    Elastic {
        dir: ElasticDirection,
        amplitude: f64,
        period: f64,
    },
}

impl ProceduralInterpolation {
    fn valid(self) -> bool {
        match self {
            Self::Elastic {
                dir,
                amplitude,
                period,
            } => Elastic::new(dir, amplitude, period).is_some(),
            Self::Bezier { p1, p2 } => TimingBezier::new(p1, p2).is_some(),
        }
    }
    fn sample(self, progress: f64) -> Option<f64> {
        match self {
            Self::Elastic {
                dir,
                amplitude,
                period,
            } => Some(Elastic::new(dir, amplitude, period)?.sample(progress)),
            Self::Bezier { p1, p2 } => TimingBezier::new(p1, p2)?.sample(progress),
        }
    }
    fn bounds(self, start: f64, end: f64) -> Option<(f64, f64)> {
        match self {
            Self::Elastic {
                dir,
                amplitude,
                period,
            } => Elastic::new(dir, amplitude, period)?.bounds(start, end),
            Self::Bezier { p1, p2 } => TimingBezier::new(p1, p2)?.bounds_interval(start, end),
        }
    }
    fn expression(self, progress: &str) -> Option<String> {
        match self {
            Self::Elastic {
                dir,
                amplitude,
                period,
            } => Some(Elastic::new(dir, amplitude, period)?.ffmpeg_expression(progress)),
            Self::Bezier { p1, p2 } => Some(TimingBezier::new(p1, p2)?.ffmpeg_expression(progress)),
        }
    }
}

impl Interpolation {
    fn valid(self) -> bool {
        match self {
            Self::Procedural(value) => value.valid(),
            _ => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Keyframe<T> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tangents: Option<KeyframeTangents>,
    pub tick: i64,
    pub value: T,
    /// Outgoing segment override; omission retains the authored track default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpolation: Option<Interpolation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyframeTangents {
    pub incoming: Tangent,
    pub outgoing: Tangent,
    pub continuity: Continuity,
}

fn segment_interpolation<T>(
    left: &Keyframe<T>,
    right: &Keyframe<T>,
    default: Interpolation,
) -> Interpolation {
    match left.interpolation.unwrap_or(default) {
        Interpolation::Procedural(ProceduralInterpolation::Bezier { p1, p2 }) => {
            let p1 = left.tangents.map_or(p1, |t| [t.outgoing.x, t.outgoing.y]);
            let p2 = right.tangents.map_or(p2, |t| [t.incoming.x, t.incoming.y]);
            Interpolation::Procedural(ProceduralInterpolation::Bezier { p1, p2 })
        }
        mode => mode,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExtrapolationMode {
    #[default]
    Hold,
    Continue,
    Loop,
    PingPong,
    Offset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Extrapolation {
    pub before: ExtrapolationMode,
    pub after: ExtrapolationMode,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyframeTrack<T> {
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub sample_clock:Option<super::native_keyframe_clock::Clock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extrapolation: Option<Extrapolation>,
    pub time_base: u64,
    pub interpolation: Interpolation,
    pub keyframes: Vec<Keyframe<T>>,
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct KeyframeTrackWire<T>{
    #[serde(default)] sample_clock:Option<super::native_keyframe_clock::Clock>,
    #[serde(default)] extrapolation:Option<Extrapolation>,
    time_base:u64,interpolation:Interpolation,keyframes:Vec<Keyframe<T>>,
}
impl<'de,T:Deserialize<'de>> Deserialize<'de> for KeyframeTrack<T>{
    fn deserialize<D:serde::Deserializer<'de>>(deserializer:D)->Result<Self,D::Error>{
        let wire=KeyframeTrackWire::<T>::deserialize(deserializer)?;
        if let Some(clock)=wire.sample_clock{clock.validate_domain(wire.time_base,&wire.keyframes).map_err(serde::de::Error::custom)?;}
        Ok(Self{sample_clock:wire.sample_clock,extrapolation:wire.extrapolation,time_base:wire.time_base,interpolation:wire.interpolation,keyframes:wire.keyframes})
    }
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
        time_base: u64,
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
        if !interpolation.valid()
            || keyframes.iter().any(|key| {
                key.interpolation.is_some_and(|mode| !mode.valid())
                    || key
                        .tangents
                        .is_some_and(|t| !t.incoming.valid() || !t.outgoing.valid())
            })
        {
            return Err(KeyframeError::NonFinite);
        }
        keyframes.sort_by_key(|keyframe| keyframe.tick);
        if keyframes
            .windows(2)
            .any(|window| window[0].tick == window[1].tick)
        {
            return Err(KeyframeError::DuplicateTick);
        }
        if keyframes
            .windows(2)
            .any(|pair| !segment_interpolation(&pair[0], &pair[1], interpolation).valid())
        {
            return Err(KeyframeError::NonFinite);
        }
        Ok(Self {
            sample_clock:None,
            extrapolation: None,
            time_base,
            interpolation,
            keyframes,
        })
    }
}

impl KeyframeTrack<f64> {
    /// Authoring normalization only. Trim/split retain already resolved numbers.
    pub fn normalize_tangents(&mut self) -> Result<(), KeyframeError> {
        if self.keyframes.iter().any(|key| {
            key.tangents
                .is_some_and(|t| !t.incoming.valid() || !t.outgoing.valid())
        }) {
            return Err(KeyframeError::NonFinite);
        }
        let keys: Vec<_> = self
            .keyframes
            .iter()
            .map(|key| {
                let fallback = KeyframeTangents {
                    incoming: Tangent {
                        x: 2.0 / 3.0,
                        y: 2.0 / 3.0,
                        mode: TangentMode::Free,
                    },
                    outgoing: Tangent {
                        x: 1.0 / 3.0,
                        y: 1.0 / 3.0,
                        mode: TangentMode::Free,
                    },
                    continuity: Continuity::Broken,
                };
                let tangent = key.tangents.unwrap_or(fallback);
                TangentKey {
                    t_us: key.tick,
                    value: key.value,
                    in_: tangent.incoming,
                    out: tangent.outgoing,
                    continuity: tangent.continuity,
                    outgoing_spline: matches!(
                        key.interpolation.unwrap_or(self.interpolation),
                        Interpolation::Procedural(ProceduralInterpolation::Bezier { .. })
                    ),
                }
            })
            .collect();
        let solved = solve_auto_tangents(&keys, Some(&|v: &f64| *v));
        if solved
            .iter()
            .any(|key| !key.in_.valid() || !key.out.valid())
        {
            return Err(KeyframeError::NonFinite);
        }
        for (key, solved) in self.keyframes.iter_mut().zip(solved) {
            if let Some(tangent) = &mut key.tangents {
                tangent.incoming = solved.in_;
                tangent.outgoing = solved.out;
            }
        }
        Ok(())
    }

    pub fn sample_tick(&self, tick: i64) -> f64 {
        let first = &self.keyframes[0];
        if tick < first.tick {
            return self.extrapolated(tick, true);
        }
        let last = self.keyframes.last().expect("non-empty track");
        if tick > last.tick {
            return self.extrapolated(tick, false);
        }
        if tick == last.tick {
            return last.value;
        }
        let next_index = self
            .keyframes
            .partition_point(|keyframe| keyframe.tick <= tick);
        let left = &self.keyframes[next_index - 1];
        let right = &self.keyframes[next_index];
        if tick == left.tick {
            return left.value;
        }
        let progress = (i128::from(tick) - i128::from(left.tick)) as f64
            / (i128::from(right.tick) - i128::from(left.tick)) as f64;
        self.interpolate_segment(left, right, progress)
    }

    fn interpolate_segment(&self, left: &Keyframe<f64>, right: &Keyframe<f64>, progress: f64) -> f64 {
        let eased = match segment_interpolation(left, right, self.interpolation) {
            Interpolation::Procedural(value) => {
                value.sample(progress).expect("validated curve parameters")
            }
            Interpolation::BounceOut => bounce_out(progress),
            Interpolation::BounceIn => 1.0 - bounce_out(1.0 - progress),
            Interpolation::BounceInOut => {
                if progress < 0.5 {
                    0.5 * (1.0 - bounce_out(1.0 - 2.0 * progress))
                } else {
                    0.5 + 0.5 * bounce_out(2.0 * progress - 1.0)
                }
            }
            Interpolation::Hold => 0.0,
            Interpolation::Linear => progress,
            Interpolation::EaseIn => progress.powi(3),
            Interpolation::EaseOut => 1.0 - (1.0 - progress).powi(3),
            Interpolation::EaseInOut | Interpolation::EaseInOutCubic if progress < 0.5 => {
                4.0 * progress.powi(3)
            }
            Interpolation::EaseInOut | Interpolation::EaseInOutCubic => {
                1.0 - (-2.0 * progress + 2.0).powi(3) / 2.0
            }
        };
        left.value + (right.value - left.value) * eased
    }

    fn endpoint_slope(&self, before: bool) -> f64 {
        if self.keyframes.len() < 2 {
            return 0.0;
        }
        let index = if before { 0 } else { self.keyframes.len() - 2 };
        let left = &self.keyframes[index];
        let right = &self.keyframes[index + 1];
        let unit = match segment_interpolation(left, right, self.interpolation) {
            Interpolation::Procedural(ProceduralInterpolation::Bezier { p1, p2 }) => {
                TimingBezier::new(p1, p2)
                    .expect("validated Bézier parameters")
                    .endpoint_slope(before)
            }
            Interpolation::Linear => 1.0,
            Interpolation::EaseIn if !before => 3.0,
            Interpolation::EaseOut if before => 3.0,
            _ => 0.0,
        };
        unit * (right.value - left.value) / (i128::from(right.tick) - i128::from(left.tick)) as f64
    }

    fn extrapolated(&self, tick: i64, before: bool) -> f64 {
        let end = if before {
            &self.keyframes[0]
        } else {
            self.keyframes.last().unwrap()
        };
        let modes = self.extrapolation.unwrap_or_default();
        let mode = if before { modes.before } else { modes.after };
        if mode == ExtrapolationMode::Hold || self.keyframes.len() < 2 {
            return end.value;
        }
        if matches!(
            mode,
            ExtrapolationMode::Loop | ExtrapolationMode::PingPong | ExtrapolationMode::Offset
        ) {
            let first = &self.keyframes[0];
            let last = self.keyframes.last().unwrap();
            let period = i128::from(last.tick) - i128::from(first.tick);
            let relative = i128::from(tick) - i128::from(first.tick);
            let cycle = relative.div_euclid(period);
            let remainder = relative.rem_euclid(period);
            let folded = if mode == ExtrapolationMode::PingPong && cycle.rem_euclid(2) == 1 {
                i128::from(last.tick) - remainder
            } else {
                i128::from(first.tick) + remainder
            };
            let sampled = self.sample_tick(folded as i64);
            return sampled
                + if mode == ExtrapolationMode::Offset {
                    cycle as f64 * (last.value - first.value)
                } else {
                    0.0
                };
        }
        end.value + self.endpoint_slope(before) * (i128::from(tick) - i128::from(end.tick)) as f64
    }

    /// Bounds include authored values and all rendered offset-cycle extrema.
    /// Elastic includes interior extrema; Hold also contributes its left limit.
    pub fn value_bounds_seconds(&self, start: f64, end: f64) -> Result<(f64, f64), KeyframeError> {
        let a = self.sample_seconds(start)?;
        let b = self.sample_seconds(end)?;
        let mut bounds = self
            .keyframes
            .iter()
            .fold((a.min(b), a.max(b)), |(lo, hi), key| {
                (lo.min(key.value), hi.max(key.value))
            });
        for window in self.keyframes.windows(2) {
            if let Interpolation::Procedural(value) =
                segment_interpolation(&window[0], &window[1], self.interpolation)
            {
                let curve = value;
                let (lo, hi) = curve.bounds(0.0, 1.0).ok_or(KeyframeError::NonFinite)?;
                let delta = window[1].value - window[0].value;
                for unit in [lo, hi] {
                    let value = window[0].value + delta * unit;
                    bounds.0 = bounds.0.min(value);
                    bounds.1 = bounds.1.max(value);
                }
            }
        }
        if self.keyframes.len() < 2 {
            return Ok(bounds);
        }
        let first = &self.keyframes[0];
        let last = self.keyframes.last().unwrap();
        let period = (i128::from(last.tick) - i128::from(first.tick)) as f64;
        let delta = last.value - first.value;
        let start_tick = start * self.time_base as f64;
        let end_tick = end * self.time_base as f64;
        let modes = self.extrapolation.unwrap_or_default();
        for (mode, low, high) in [
            (modes.before, start_tick, end_tick.min(first.tick as f64)),
            (modes.after, start_tick.max(last.tick as f64), end_tick),
        ] {
            if mode != ExtrapolationMode::Offset || high <= low {
                continue;
            }
            for window in self.keyframes.windows(2) {
                if let Interpolation::Procedural(value) =
                    segment_interpolation(&window[0], &window[1], self.interpolation)
                {
                    let curve = value;
                    let seg_start = window[0].tick as f64;
                    let seg_end = window[1].tick as f64;
                    let first_cycle = ((low - seg_end) / period).ceil();
                    let last_cycle = ((high - seg_start) / period).floor();
                    for cycle in [first_cycle, first_cycle + 1.0, last_cycle - 1.0, last_cycle] {
                        if cycle < first_cycle || cycle > last_cycle {
                            continue;
                        }
                        let start = ((low - cycle * period - seg_start) / (seg_end - seg_start))
                            .clamp(0.0, 1.0);
                        let end = ((high - cycle * period - seg_start) / (seg_end - seg_start))
                            .clamp(0.0, 1.0);
                        if let Some((lo, hi)) = curve.bounds(start, end) {
                            for unit in [lo, hi] {
                                let candidate = window[0].value
                                    + (window[1].value - window[0].value) * unit
                                    + cycle * delta;
                                bounds.0 = bounds.0.min(candidate);
                                bounds.1 = bounds.1.max(candidate);
                            }
                        }
                    }
                }
            }
            for (index, key) in self.keyframes.iter().enumerate() {
                let first_cycle = ((low - key.tick as f64) / period).ceil();
                let last_cycle = ((high - key.tick as f64) / period).floor();
                if first_cycle > last_cycle {
                    continue;
                }
                for cycle in [first_cycle, last_cycle] {
                    let mut include = |value: f64| {
                        let candidate = value + cycle * delta;
                        bounds.0 = bounds.0.min(candidate);
                        bounds.1 = bounds.1.max(candidate);
                    };
                    include(key.value);
                    if index > 0 {
                        let previous = &self.keyframes[index - 1];
                        if previous.interpolation.unwrap_or(self.interpolation)
                            == Interpolation::Hold
                        {
                            include(previous.value);
                        }
                    }
                }
            }
        }
        if !bounds.0.is_finite() || !bounds.1.is_finite() {
            return Err(KeyframeError::NonFinite);
        }
        Ok(bounds)
    }

    pub fn sample_seconds(&self, seconds: f64) -> Result<f64, KeyframeError> {
        if let Some(clock)=self.sample_clock {return clock.prepare(self).and_then(|prepared|prepared.sample_seconds(seconds)).map_err(|_|KeyframeError::InvalidClock)}
        if !seconds.is_finite() {
            return Err(KeyframeError::NonFinite);
        }
        let tick = (seconds * self.time_base as f64).round() as i64;
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
        self.expression_shifted(time_variable, 0.0)
    }

    /// As [`Self::expression`], with keyframe zero occurring `offset_seconds`
    /// after the filter's time origin. This is used when a clip-local track is
    /// evaluated by a timeline-global FFmpeg filter such as `overlay`.
    pub fn expression_shifted(
        &self,
        time_variable: &str,
        offset_seconds: f64,
    ) -> Result<String, KeyframeError> {
        if time_variable.is_empty()
            || !time_variable
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            || !offset_seconds.is_finite()
        {
            return Err(KeyframeError::InvalidVariable);
        }
        let time_expression = if offset_seconds == 0.0 {
            time_variable.to_owned()
        } else {
            format!("({time_variable}-{})", format_number(offset_seconds))
        };
        let time_expression=if let Some(clock)=self.track.sample_clock {
            clock.prepare(self.track).map_err(|_|KeyframeError::InvalidClock)?;
            let(num,den,offset)=clock.parts();
            format!("((({time_expression})*{}+{})/{})",num,format_number(offset as f64/1e6),den)
        }else{time_expression};
        self.expression_time(&time_expression)
    }

    fn expression_time(&self, time_expression: &str) -> Result<String, KeyframeError> {
        let mut expression = format_number(self.track.keyframes.last().unwrap().value);
        for window in self.track.keyframes.windows(2).rev() {
            let left = &window[0];
            let right = &window[1];
            let start = left.tick as f64 / self.track.time_base as f64;
            let end = right.tick as f64 / self.track.time_base as f64;
            let progress = format!(
                "(({time_expression}-{})/{})",
                format_number(start),
                format_number(end - start)
            );
            let eased = match segment_interpolation(left, right, self.track.interpolation) {
                Interpolation::Procedural(value) => value
                    .expression(&progress)
                    .ok_or(KeyframeError::NonFinite)?,
                Interpolation::BounceOut => bounce_expression(&progress),
                Interpolation::BounceIn => {
                    format!("1-({})", bounce_expression(&format!("(1-{progress})")))
                }
                Interpolation::BounceInOut => format!(
                    "if(lt({progress},0.5),0.5*(1-({})),0.5+0.5*({}))",
                    bounce_expression(&format!("(1-2*{progress})")),
                    bounce_expression(&format!("(2*{progress}-1)"))
                ),
                Interpolation::Hold => "0".to_owned(),
                Interpolation::Linear => progress,
                Interpolation::EaseIn => format!("pow({progress},3)"),
                Interpolation::EaseOut => format!("1-pow(1-{progress},3)"),
                Interpolation::EaseInOut | Interpolation::EaseInOutCubic => {
                    format!("if(lt({progress},0.5),4*pow({progress},3),1-pow(-2*{progress}+2,3)/2)")
                }
            };
            let mut segment = format!(
                "{}+({}-{})*{}",
                format_number(left.value),
                format_number(right.value),
                format_number(left.value),
                eased
            );
            if matches!(
                segment_interpolation(left, right, self.track.interpolation),
                Interpolation::Procedural(_)
            ) {
                segment = format!(
                    "if(eq({time_expression},{}),{},{segment})",
                    format_number(start),
                    format_number(left.value)
                );
            }
            expression = format!(
                "if(lt({time_expression},{}),{segment},{expression})",
                format_number(end)
            );
        }
        let first = &self.track.keyframes[0];
        let first_time = first.tick as f64 / self.track.time_base as f64;
        let modes = self.track.extrapolation.unwrap_or_default();
        let boundary = |before: bool| {
            let end = if before {
                first
            } else {
                self.track.keyframes.last().unwrap()
            };
            let mode = if before { modes.before } else { modes.after };
            if mode == ExtrapolationMode::Hold || self.track.keyframes.len() < 2 {
                return format_number(end.value);
            }
            if matches!(
                mode,
                ExtrapolationMode::Loop | ExtrapolationMode::PingPong | ExtrapolationMode::Offset
            ) {
                let last = self.track.keyframes.last().unwrap();
                let period = format_number(
                    (i128::from(last.tick) - i128::from(first.tick)) as f64
                        / self.track.time_base as f64,
                );
                let cycle = format!(
                    "floor(({time_expression}-{})/{period})",
                    format_number(first_time)
                );
                let remainder = format!(
                    "({time_expression}-{}-({cycle})*{period})",
                    format_number(first_time)
                );
                let forward = format!("({}+{remainder})", format_number(first_time));
                let folded = if mode == ExtrapolationMode::PingPong {
                    format!(
                        "if(eq(({cycle})-2*floor(({cycle})/2),1),({}-{remainder}),{forward})",
                        format_number(last.tick as f64 / self.track.time_base as f64)
                    )
                } else {
                    forward
                };
                let mut held = self.track.clone();
                held.extrapolation = None;
                let sampled = FfmpegKeyframeAdapter::new(&held)
                    .expression_time(&folded)
                    .expect("valid folded expression");
                return if mode == ExtrapolationMode::Offset {
                    format!(
                        "({sampled})+({cycle})*({})",
                        format_number(last.value - first.value)
                    )
                } else {
                    sampled
                };
            }
            format!(
                "{}+{}*({time_expression}-{})",
                format_number(end.value),
                format_number(self.track.endpoint_slope(before) * self.track.time_base as f64),
                format_number(end.tick as f64 / self.track.time_base as f64)
            )
        };
        if modes.after != ExtrapolationMode::Hold {
            let last_time =
                self.track.keyframes.last().unwrap().tick as f64 / self.track.time_base as f64;
            expression = format!(
                "if(gt({time_expression},{}),{},{expression})",
                format_number(last_time),
                boundary(false)
            );
        }
        Ok(format!(
            "if(lt({time_expression},{}),{},{expression})",
            format_number(first_time),
            boundary(true)
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
    InvalidClock,
    InvalidTimeBase,
    EmptyTrack,
    DuplicateTick,
    TooManyKeyframes,
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

    #[test]
    fn bounce_directions_match_pinned_values_and_real_ffmpeg() {
        for (mode, midpoint, wire_name) in [
            (Interpolation::BounceOut, 0.765625, "bounce_out"),
            (Interpolation::BounceIn, 0.234375, "bounce_in"),
            (Interpolation::BounceInOut, 0.5, "bounce_in_out"),
        ] {
            let mut value = KeyframeTrack::new(
                1000,
                Interpolation::Linear,
                vec![
                    Keyframe {
                        tangents: None,
                        tick: 0,
                        value: 0.0,
                        interpolation: Some(mode),
                    },
                    Keyframe {
                        tangents: None,
                        tick: 1000,
                        value: 1.0,
                        interpolation: None,
                    },
                ],
            )
            .unwrap();
            assert_eq!(value.sample_tick(500), midpoint);
            value.extrapolation = Some(Extrapolation {
                before: ExtrapolationMode::Continue,
                after: ExtrapolationMode::Continue,
            });
            assert_eq!(value.sample_tick(-1000), 0.0);
            assert_eq!(value.sample_tick(2000), 1.0);
            let wire = serde_json::to_value(&value).unwrap();
            assert_eq!(wire["keyframes"][0]["interpolation"], wire_name);
            assert_eq!(
                serde_json::from_value::<KeyframeTrack<f64>>(wire).unwrap(),
                value
            );
            let expression = FfmpegKeyframeAdapter::new(&value).expression("t").unwrap();
            let output = std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("aevalsrc=exprs='{expression}':s=1000:d=1.001"),
                    "-c:a",
                    "pcm_f64le",
                    "-f",
                    "f64le",
                    "pipe:1",
                ])
                .output()
                .expect("FFmpeg required for Bounce export qualification");
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout.len(), 1001 * 8);
            for (tick, bytes) in output.stdout.chunks_exact(8).enumerate() {
                let actual = f64::from_le_bytes(bytes.try_into().unwrap());
                assert!(
                    (actual - value.sample_tick(tick as i64)).abs() < 1e-12,
                    "{mode:?} tick={tick} actual={actual}"
                );
            }
        }
        assert_eq!(bounce_out(0.25), 0.47265625);
        assert_eq!(bounce_out(0.75), 0.97265625);
    }

    #[test]
    fn parameterized_elastic_wire_boundaries_export_and_offset_bounds() {
        for dir in [
            ElasticDirection::In,
            ElasticDirection::Out,
            ElasticDirection::InOut,
        ] {
            let mode = Interpolation::Procedural(ProceduralInterpolation::Elastic {
                dir,
                amplitude: 2.0,
                period: 0.17,
            });
            let mut value = KeyframeTrack::new(
                1000,
                mode,
                vec![
                    Keyframe {
                        tangents: None,
                        tick: 0,
                        value: 0.2,
                        interpolation: None,
                    },
                    Keyframe {
                        tangents: None,
                        tick: 1000,
                        value: 0.4,
                        interpolation: None,
                    },
                ],
            )
            .unwrap();
            let wire = serde_json::to_value(&value).unwrap();
            assert_eq!(wire["interpolation"]["kind"], "elastic");
            assert_eq!(wire["interpolation"]["amplitude"], 2.0);
            assert_eq!(
                serde_json::from_value::<KeyframeTrack<f64>>(wire).unwrap(),
                value
            );
            assert_eq!(value.sample_tick(0), 0.2);
            assert_eq!(value.sample_tick(1000), 0.4);
            let expression = FfmpegKeyframeAdapter::new(&value).expression("t").unwrap();
            let output = std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("aevalsrc=exprs='{expression}':s=1000:d=1.001"),
                    "-c:a",
                    "pcm_f64le",
                    "-f",
                    "f64le",
                    "pipe:1",
                ])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout.len(), 1001 * 8);
            for (tick, bytes) in output.stdout.chunks_exact(8).enumerate() {
                let actual = f64::from_le_bytes(bytes.try_into().unwrap());
                assert!(
                    (actual - value.sample_tick(tick as i64)).abs() < 1e-12,
                    "{dir:?} tick={tick}"
                );
            }
            value.extrapolation = Some(Extrapolation {
                before: ExtrapolationMode::Offset,
                after: ExtrapolationMode::Offset,
            });
            for (start, end) in [(-4.7, -2.1), (-0.4, 0.7), (2.2, 5.6), (1000.2, 1001.7)] {
                let (lo, hi) = value.value_bounds_seconds(start, end).unwrap();
                for tick in (start * 1000.0) as i64..=(end * 1000.0) as i64 {
                    let sample = value.sample_tick(tick);
                    assert!(
                        sample >= lo - 1e-10 && sample <= hi + 1e-10,
                        "{dir:?} tick={tick} {sample} outside {lo}..{hi}"
                    );
                }
            }
        }
        let bad = Interpolation::Procedural(ProceduralInterpolation::Elastic {
            dir: ElasticDirection::Out,
            amplitude: 0.5,
            period: 0.3,
        });
        assert_eq!(
            KeyframeTrack::new(
                1000,
                bad,
                vec![Keyframe {
                    tangents: None,
                    tick: 0,
                    value: 0.0,
                    interpolation: None
                }]
            ),
            Err(KeyframeError::NonFinite)
        );
        assert_eq!(
            serde_json::to_value(Interpolation::Linear).unwrap(),
            "linear"
        );
    }

    #[test]
    fn bezier_keyframes_roundtrip_export_and_continued_offset_bounds() {
        let mode = Interpolation::Procedural(ProceduralInterpolation::Bezier {
            p1: [0.1, -2.0],
            p2: [0.9, 3.0],
        });
        let mut value = KeyframeTrack::new(
            1000,
            Interpolation::Linear,
            vec![
                Keyframe {
                    tangents: None,
                    tick: 0,
                    value: 0.2,
                    interpolation: Some(mode),
                },
                Keyframe {
                    tangents: None,
                    tick: 1000,
                    value: 0.4,
                    interpolation: None,
                },
            ],
        )
        .unwrap();
        let wire = serde_json::to_value(&value).unwrap();
        assert_eq!(wire["keyframes"][0]["interpolation"]["kind"], "bezier");
        assert_eq!(
            serde_json::from_value::<KeyframeTrack<f64>>(wire).unwrap(),
            value
        );
        let expression = FfmpegKeyframeAdapter::new(&value).expression("t").unwrap();
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &format!("aevalsrc=exprs='{expression}':s=1000:d=1.001"),
                "-c:a",
                "pcm_f64le",
                "-f",
                "f64le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout.len(), 1001 * 8);
        for (tick, bytes) in output.stdout.chunks_exact(8).enumerate() {
            assert!(
                (f64::from_le_bytes(bytes.try_into().unwrap()) - value.sample_tick(tick as i64))
                    .abs()
                    < 1e-12
            );
        }
        value.extrapolation = Some(Extrapolation {
            before: ExtrapolationMode::Continue,
            after: ExtrapolationMode::Continue,
        });
        assert!((value.sample_tick(-1000) - 4.2).abs() < 1e-12);
        assert!((value.sample_tick(2000) + 3.6).abs() < 1e-12);
        value.extrapolation = Some(Extrapolation {
            before: ExtrapolationMode::Offset,
            after: ExtrapolationMode::Offset,
        });
        for (start, end) in [(-4.7, -2.1), (-0.4, 0.7), (2.2, 5.6), (1000.2, 1001.7)] {
            let (lo, hi) = value.value_bounds_seconds(start, end).unwrap();
            for tick in (start * 1000.0) as i64..=(end * 1000.0) as i64 {
                let at = value.sample_tick(tick);
                assert!(
                    at >= lo - 1e-10 && at <= hi + 1e-10,
                    "{tick} {at} outside {lo}..{hi}"
                );
            }
        }
        let bad = Interpolation::Procedural(ProceduralInterpolation::Bezier {
            p1: [-0.1, 0.0],
            p2: [1.0, 1.0],
        });
        assert_eq!(
            KeyframeTrack::new(
                1000,
                bad,
                vec![Keyframe {
                    tangents: None,
                    tick: 0,
                    value: 0.0,
                    interpolation: None
                }]
            ),
            Err(KeyframeError::NonFinite)
        );
    }

    fn track(interpolation: Interpolation) -> KeyframeTrack<f64> {
        KeyframeTrack::new(
            1_000,
            interpolation,
            vec![
                Keyframe {
                    tangents: None,
                    interpolation: None,
                    tick: 0,
                    value: 0.0,
                },
                Keyframe {
                    tangents: None,
                    interpolation: None,
                    tick: 1_000,
                    value: 10.0,
                },
                Keyframe {
                    tangents: None,
                    interpolation: None,
                    tick: 2_000,
                    value: 20.0,
                },
            ],
        )
        .unwrap()
    }

    #[test]
    fn per_key_tangents_roundtrip_normalize_and_match_ffmpeg() {
        let bezier = Interpolation::Procedural(ProceduralInterpolation::Bezier {
            p1: [0.25, 0.1],
            p2: [0.25, 1.0],
        });
        let mut value = track(bezier);
        let legacy = serde_json::to_value(&value).unwrap();
        assert!(legacy["keyframes"][0].get("tangents").is_none());
        for key in &mut value.keyframes {
            key.tangents = Some(KeyframeTangents {
                incoming: Tangent {
                    x: 0.8,
                    y: 0.7,
                    mode: TangentMode::Auto,
                },
                outgoing: Tangent {
                    x: 0.2,
                    y: 0.3,
                    mode: TangentMode::Auto,
                },
                continuity: Continuity::Smooth,
            });
        }
        value.normalize_tangents().unwrap();
        assert_eq!(value.keyframes[0].tangents.unwrap().outgoing.y, 0.0);
        assert!((value.keyframes[1].tangents.unwrap().outgoing.y - 1.0 / 3.0).abs() < 1e-15);
        let wire = serde_json::to_value(&value).unwrap();
        assert_eq!(wire["keyframes"][1]["tangents"]["continuity"], "smooth");
        assert_eq!(
            serde_json::from_value::<KeyframeTrack<f64>>(wire).unwrap(),
            value
        );
        let resolved = value.clone();
        value.normalize_tangents().unwrap();
        assert_eq!(value, resolved);
        let expression = FfmpegKeyframeAdapter::new(&value).expression("t").unwrap();
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &format!("aevalsrc=exprs='{expression}':s=1000:d=2.001"),
                "-c:a",
                "pcm_f64le",
                "-f",
                "f64le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout.len(), 2001 * 8);
        for (tick, bytes) in output.stdout.chunks_exact(8).enumerate() {
            let actual = f64::from_le_bytes(bytes.try_into().unwrap());
            assert!(
                (actual - value.sample_tick(tick as i64)).abs() < 1e-12,
                "tick={tick}"
            );
        }
        let (low, high) = value.value_bounds_seconds(0.0, 2.0).unwrap();
        assert_eq!((low, high), (0.0, 20.0));
        value.keyframes[0].tangents.as_mut().unwrap().outgoing.x = -0.1;
        assert_eq!(
            KeyframeTrack::new(value.time_base, value.interpolation, value.keyframes),
            Err(KeyframeError::NonFinite)
        );
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
        let shifted = FfmpegKeyframeAdapter::new(&track(Interpolation::Hold))
            .expression_shifted("t", 2.5)
            .unwrap();
        assert!(shifted.contains("lt((t-2.5),1)"), "{shifted}");
    }

    #[test]
    fn cubic_easing_modes_match_preview_and_ffmpeg_contract() {
        assert_eq!(track(Interpolation::EaseIn).sample_tick(500), 1.25);
        assert_eq!(track(Interpolation::EaseOut).sample_tick(500), 8.75);
        assert_eq!(track(Interpolation::EaseInOut).sample_tick(500), 5.0);
        assert_eq!(
            track(Interpolation::EaseInOutCubic).sample_tick(500),
            track(Interpolation::EaseInOut).sample_tick(500)
        );
        for interpolation in [
            Interpolation::EaseIn,
            Interpolation::EaseOut,
            Interpolation::EaseInOut,
            Interpolation::EaseInOutCubic,
        ] {
            let expression = FfmpegKeyframeAdapter::new(&track(interpolation))
                .expression("T")
                .unwrap();
            assert!(
                expression.contains("pow("),
                "{interpolation:?}: {expression}"
            );
        }
    }

    #[test]
    fn rejects_non_finite_values_at_the_domain_boundary() {
        assert_eq!(
            KeyframeTrack::new(
                1_000,
                Interpolation::Linear,
                vec![Keyframe {
                    tangents: None,
                    interpolation: None,
                    tick: 0,
                    value: f64::NAN,
                }],
            ),
            Err(KeyframeError::NonFinite)
        );
    }
    #[test]
    fn per_key_interpolation_roundtrips_and_matches_real_ffmpeg_samples() {
        let mut keyed = track(Interpolation::Linear);
        keyed.keyframes[0].interpolation = Some(Interpolation::EaseIn);
        keyed.keyframes[1].interpolation = Some(Interpolation::Hold);
        assert_eq!(keyed.sample_tick(500), 1.25);
        assert_eq!(keyed.sample_tick(1500), 10.0);
        let wire = serde_json::to_value(&keyed).unwrap();
        assert_eq!(wire["keyframes"][0]["interpolation"], "ease_in");
        assert!(wire["keyframes"][2].get("interpolation").is_none());
        let restored: KeyframeTrack<f64> = serde_json::from_value(wire).unwrap();
        assert_eq!(restored, keyed);
        let expression = FfmpegKeyframeAdapter::new(&keyed).expression("T").unwrap();
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=16x16:r=2:d=2",
                "-vf",
                &format!("format=gray,geq=lum='({expression})*10'"),
                "-frames:v",
                "4",
                "-pix_fmt",
                "gray",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .expect("ffmpeg must be available for numeric expression qualification");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout.len(), 4 * 16 * 16);
        for (frame, tick) in [0, 500, 1000, 1500].into_iter().enumerate() {
            let expected = keyed.sample_tick(tick) * 10.0;
            assert!(
                output.stdout[frame * 256..(frame + 1) * 256]
                    .iter()
                    .all(|pixel| (f64::from(*pixel) - expected).abs() <= 1.0)
            );
        }
    }
    #[test]
    fn signed_keys_roundtrip_and_sample_across_clip_zero() {
        let track = KeyframeTrack::new(
            1000,
            Interpolation::Linear,
            vec![
                Keyframe {
                    tangents: None,
                    tick: -1000,
                    value: 0.0,
                    interpolation: None,
                },
                Keyframe {
                    tangents: None,
                    tick: 1000,
                    value: 10.0,
                    interpolation: None,
                },
            ],
        )
        .unwrap();
        assert_eq!(track.sample_tick(-500), 2.5);
        assert_eq!(track.sample_tick(0), 5.0);
        assert_eq!(track.sample_seconds(-0.5).unwrap(), 2.5);
        let wire = serde_json::to_string(&track).unwrap();
        let restored: KeyframeTrack<f64> = serde_json::from_str(&wire).unwrap();
        assert_eq!(restored, track);
        assert!(
            FfmpegKeyframeAdapter::new(&track)
                .expression("t")
                .unwrap()
                .contains("-1")
        );
        let expression = FfmpegKeyframeAdapter::new(&track).expression("T").unwrap();
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=16x16:r=2:d=1",
                "-vf",
                &format!("format=gray,geq=lum='({expression})*20'"),
                "-frames:v",
                "2",
                "-pix_fmt",
                "gray",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout.len(), 512);
        for (index, expected) in [100.0, 150.0].into_iter().enumerate() {
            assert!(
                output.stdout[index * 256..(index + 1) * 256]
                    .iter()
                    .all(|pixel| (f64::from(*pixel) - expected).abs() <= 1.0)
            );
        }
        let wide = KeyframeTrack::new(
            1,
            Interpolation::Linear,
            vec![
                Keyframe {
                    tangents: None,
                    tick: i64::MIN,
                    value: 0.0,
                    interpolation: None,
                },
                Keyframe {
                    tangents: None,
                    tick: i64::MAX,
                    value: 1.0,
                    interpolation: None,
                },
            ],
        )
        .unwrap();
        assert!((wide.sample_tick(0) - 0.5).abs() < 1e-12);
    }
    #[test]
    fn continuation_slopes_roundtrip_and_compile() {
        let mut keyed = KeyframeTrack::new(
            1000,
            Interpolation::EaseIn,
            vec![
                Keyframe {
                    tangents: None,
                    tick: 1000,
                    value: 1.0,
                    interpolation: None,
                },
                Keyframe {
                    tangents: None,
                    tick: 2000,
                    value: 3.0,
                    interpolation: None,
                },
            ],
        )
        .unwrap();
        keyed.extrapolation = Some(Extrapolation {
            before: ExtrapolationMode::Continue,
            after: ExtrapolationMode::Continue,
        });
        assert_eq!(keyed.sample_tick(0), 1.0);
        assert_eq!(keyed.sample_tick(3000), 9.0);
        let restored: KeyframeTrack<f64> =
            serde_json::from_str(&serde_json::to_string(&keyed).unwrap()).unwrap();
        assert_eq!(restored, keyed);
        let expression = FfmpegKeyframeAdapter::new(&keyed).expression("T").unwrap();
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=16x16:r=1:d=4",
                "-vf",
                &format!("format=gray,geq=lum='({expression})*10'"),
                "-frames:v",
                "4",
                "-pix_fmt",
                "gray",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout.len(), 1024);
        for (frame, expected) in [10.0, 10.0, 30.0, 90.0].into_iter().enumerate() {
            assert!(
                output.stdout[frame * 256..(frame + 1) * 256]
                    .iter()
                    .all(|pixel| (f64::from(*pixel) - expected).abs() <= 1.0)
            );
        }
    }
    #[test]
    fn cyclic_modes_match_native_and_ffmpeg_at_negative_cycles_and_boundaries() {
        for (mode, expected) in [
            (ExtrapolationMode::Loop, [15., 10., 15., 10., 20., 15., 10.]),
            (
                ExtrapolationMode::PingPong,
                [15., 20., 15., 10., 20., 15., 10.],
            ),
            (ExtrapolationMode::Offset, [-5., 0., 5., 10., 20., 25., 30.]),
        ] {
            let mut track = KeyframeTrack::new(
                1000,
                Interpolation::Linear,
                vec![
                    Keyframe {
                        tangents: None,
                        tick: 1000,
                        value: 10.,
                        interpolation: None,
                    },
                    Keyframe {
                        tangents: None,
                        tick: 2000,
                        value: 20.,
                        interpolation: None,
                    },
                ],
            )
            .unwrap();
            track.extrapolation = Some(Extrapolation {
                before: mode,
                after: mode,
            });
            for (tick, value) in [-500, 0, 500, 1000, 2000, 2500, 3000]
                .into_iter()
                .zip(expected)
            {
                assert_eq!(track.sample_tick(tick), value);
            }
            let expression = FfmpegKeyframeAdapter::new(&track)
                .expression_shifted("T", 0.5)
                .unwrap();
            let output = std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=black:s=16x16:r=2:d=4",
                    "-vf",
                    &format!("format=gray,geq=lum='({expression})+50'"),
                    "-frames:v",
                    "8",
                    "-pix_fmt",
                    "gray",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout.len(), 8 * 256);
            for frame in 0..8 {
                let expected = track.sample_tick(frame as i64 * 500 - 500) + 50.;
                assert!(
                    output.stdout[frame * 256..(frame + 1) * 256]
                        .iter()
                        .all(|pixel| (f64::from(*pixel) - expected).abs() <= 1.)
                );
            }
        }
    }
    #[test]
    fn offset_bounds_include_interior_peaks_in_far_and_negative_cycles() {
        let mut track = KeyframeTrack::new(
            1000,
            Interpolation::Linear,
            vec![
                Keyframe {
                    tangents: None,
                    tick: 1000,
                    value: 1.,
                    interpolation: None,
                },
                Keyframe {
                    tangents: None,
                    tick: 1500,
                    value: 10.,
                    interpolation: None,
                },
                Keyframe {
                    tangents: None,
                    tick: 2000,
                    value: 2.,
                    interpolation: None,
                },
            ],
        )
        .unwrap();
        track.extrapolation = Some(Extrapolation {
            before: ExtrapolationMode::Offset,
            after: ExtrapolationMode::Offset,
        });
        assert_eq!(track.value_bounds_seconds(0., 4.).unwrap(), (0., 12.));
        assert_eq!(
            track.value_bounds_seconds(-100., 100.).unwrap(),
            (-100., 108.)
        );
        let (lo, hi) = track.value_bounds_seconds(-100., 100.).unwrap();
        for tick in -100_000..=100_000 {
            let value = track.sample_tick(tick);
            assert!(value >= lo && value <= hi);
        }
        assert!(track.value_bounds_seconds(0., f64::INFINITY).is_err());
    }
}

include!("keyframe_source_time.rs");
