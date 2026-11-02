//! Native premultiplied OkLab color tracks, using the pinned WeftCut evaluator.
use serde::{Deserialize, Serialize};
use weftcut_eval::{eval_precise, Extrapolate, Extrapolation, Interpolate, Kf, Rgba8, Segment};

const MAX_KEYS: usize = 4096;
const MAX_SAMPLES: usize = 4096;
const MAX_SAFE_TIME: i64 = 9_007_199_254_740_991;

/// Stable, collision-free ASCII filter instance names for arbitrary clip IDs.
pub fn color_command_target(clip_id: &str) -> String {
    use std::fmt::Write;
    let mut target = String::from("color_");
    for byte in clip_id.bytes() {
        write!(&mut target, "{byte:02x}").expect("String writes cannot fail");
    }
    target
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
impl From<Color> for Rgba8 {
    fn from(c: Color) -> Self {
        Self {
            r: c.r,
            g: c.g,
            b: c.b,
            a: c.a,
        }
    }
}
impl From<Rgba8> for Color {
    fn from(c: Rgba8) -> Self {
        Self {
            r: c.r,
            g: c.g,
            b: c.b,
            a: c.a,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorKey {
    pub t_us: f64,
    pub value: Color,
    pub in_: (f64, f64),
    pub out: (f64, f64),
    pub segment: Segment,
    #[serde(default)]
    pub legacy_curve: Option<LegacyColorCurve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tangent_modes: Option<ColorTangentModes>,
}

/// Retained authoring state; rendering consumes the explicit packed coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorTangentModes {
    pub incoming: super::tangents::TangentMode,
    pub outgoing: super::tangents::TangentMode,
    pub continuity: super::tangents::Continuity,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyColorCurve {
    EaseInOut,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorTrack {
    pub keys: Vec<ColorKey>,
    pub extrapolate: Extrapolation,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorSampleRequest {
    pub track: ColorTrack,
    pub default: Color,
    pub times_us: Vec<i64>,
}

pub fn sample_colors(request: &ColorSampleRequest) -> Result<Vec<Color>, String> {
    let keys = &request.track.keys;
    if keys.len() > MAX_KEYS || request.times_us.len() > MAX_SAMPLES {
        return Err("Color sampling accepts at most 4096 keys and times".into());
    }
    let valid_time = |time: i64| (-MAX_SAFE_TIME..=MAX_SAFE_TIME).contains(&time);
    if request.times_us.iter().any(|&time| !valid_time(time)) {
        return Err("Color sample time must be a safe integer".into());
    }
    for (index, key) in keys.iter().enumerate() {
        if !key.t_us.is_finite()
            || key.t_us.abs() > MAX_SAFE_TIME as f64
            || index > 0 && keys[index - 1].t_us >= key.t_us
        {
            return Err("Color keys must have strictly increasing finite safe times".into());
        }
        for (x, y) in [key.in_, key.out] {
            if !x.is_finite() || !(0.0..=1.0).contains(&x) || !y.is_finite() {
                return Err("Color tangent X must be in 0..1 and Y must be finite".into());
            }
        }
        if key.legacy_curve.is_some()
            && (!matches!(key.segment, Segment::Spline)
                || key.out != (1.0 / 3.0, 0.0)
                || keys
                    .get(index + 1)
                    .is_none_or(|next| next.in_ != (2.0 / 3.0, 1.0)))
        {
            return Err("Legacy color easing requires canonical cubic endpoint controls".into());
        }
        if let Segment::Elastic {
            amplitude, period, ..
        } = key.segment
        {
            if !amplitude.is_finite() || amplitude < 1.0 || !period.is_finite() || period <= 0.0 {
                return Err("Color Elastic needs finite amplitude >= 1 and period > 0".into());
            }
        }
    }
    let native: Vec<Kf<Rgba8>> = keys
        .iter()
        .map(|key| Kf {
            t_us: key.t_us,
            value: key.value.into(),
            in_: key.in_,
            out: key.out,
            segment: key.segment,
        })
        .collect();
    Ok(request
        .times_us
        .iter()
        .map(|&time| {
            eval_color(
                &native,
                keys,
                request.track.extrapolate,
                time as f64,
                request.default.into(),
            )
            .into()
        })
        .collect())
}

/// Validated resident track for repeated single-time preview without per-frame
/// JSON parsing, key admission or rebuilding the native key vector.
pub struct PreparedColorTrack {
    request: ColorSampleRequest,
    native: Vec<Kf<Rgba8>>,
    has_legacy_curve: bool,
}
impl PreparedColorTrack {
    pub fn new(mut request: ColorSampleRequest) -> Result<Self, String> {
        request.times_us.clear();
        // Reuse the HTTP admission path; no duplicate validation rules.
        sample_colors(&request)?;
        let native = request
            .track
            .keys
            .iter()
            .map(|key| Kf {
                t_us: key.t_us,
                value: key.value.into(),
                in_: key.in_,
                out: key.out,
                segment: key.segment,
            })
            .collect();
        let has_legacy_curve = request
            .track
            .keys
            .iter()
            .any(|key| key.legacy_curve.is_some());
        Ok(Self {
            request,
            native,
            has_legacy_curve,
        })
    }
    pub fn sample(&self, time: f64) -> Result<Color, String> {
        if !time.is_finite() || time.fract() != 0.0 || time.abs() > MAX_SAFE_TIME as f64 {
            return Err("Color sample time must be a safe integer".into());
        }
        let value = if self.has_legacy_curve {
            eval_color(
                &self.native,
                &self.request.track.keys,
                self.request.track.extrapolate,
                time,
                self.request.default.into(),
            )
        } else {
            eval_precise(
                &self.native,
                self.request.track.extrapolate,
                time,
                self.request.default.into(),
            )
        };
        Ok(value.into())
    }
}

// A saved Video Kadr cubic-in/out is piecewise, unlike an authored WeftCut
// Spline. Evaluate its scalar weight natively without quantizing a midpoint
// color. All normal curves and all RGBA interpolation stay in the shared core.
fn eval_color(
    native: &[Kf<Rgba8>],
    keys: &[ColorKey],
    ex: Extrapolation,
    time: f64,
    default: Rgba8,
) -> Rgba8 {
    if keys.len() < 2 || !keys.iter().any(|key| key.legacy_curve.is_some()) {
        return eval_precise(native, ex, time, default);
    }
    let first = &native[0];
    let last = &native[native.len() - 1];
    let inside = |t: f64| {
        if t <= first.t_us || t >= last.t_us {
            return eval_precise(native, Extrapolation::HOLD, t, default);
        }
        let index = native.partition_point(|key| key.t_us <= t) - 1;
        if keys[index].legacy_curve.is_none() {
            return eval_precise(native, Extrapolation::HOLD, t, default);
        }
        let a = &native[index];
        let b = &native[index + 1];
        let u = (t - a.t_us) / (b.t_us - a.t_us);
        let weight = if u < 0.5 {
            4.0 * u * u * u
        } else {
            1.0 - (-2.0 * u + 2.0).powi(3) / 2.0
        };
        Rgba8::lerp(a.value, b.value, weight)
    };
    if time >= first.t_us && time <= last.t_us {
        return inside(time);
    }
    let mode = if time < first.t_us {
        ex.before
    } else {
        ex.after
    };
    if matches!(mode, Extrapolate::Hold | Extrapolate::Continue) {
        return eval_precise(native, ex, time, default);
    }
    let period = last.t_us - first.t_us;
    let rel = time - first.t_us;
    let n = (rel / period).floor();
    let u = rel - n * period;
    match mode {
        Extrapolate::Loop => inside(first.t_us + u),
        Extrapolate::PingPong => inside(if n - 2.0 * (n / 2.0).floor() == 1.0 {
            last.t_us - u
        } else {
            first.t_us + u
        }),
        Extrapolate::Offset => Rgba8::offset(inside(first.t_us + u), first.value, last.value, n),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn legacy_color_easing_keeps_hue_without_midpoint_quantization() {
        let mut request: ColorSampleRequest = serde_json::from_value(json!({
            "track":{"keys":[
                {"t_us":0.5,"value":{"r":255,"g":0,"b":0,"a":255},"in_":[2.0/3.0,2.0/3.0],"out":[1.0/3.0,0],"segment":{"kind":"Spline"},"legacy_curve":"ease_in_out"},
                {"t_us":1000000.5,"value":{"r":0,"g":0,"b":0,"a":0},"in_":[2.0/3.0,1],"out":[1.0/3.0,1.0/3.0],"segment":{"kind":"Linear"}}
            ],"extrapolate":{"before":"Continue","after":"Continue"}},
            "default":{"r":0,"g":0,"b":0,"a":255},"times_us":[-500000,250000,750000,1500000]
        })).unwrap();
        let sampled = sample_colors(&request).unwrap();
        let prepared = PreparedColorTrack::new(request.clone()).unwrap();
        for (&time, &color) in request.times_us.iter().zip(&sampled) {
            assert_eq!(prepared.sample(time as f64).unwrap(), color);
        }

        assert_eq!(
            sampled[0],
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255
            }
        );
        assert_eq!(
            sampled[1],
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 239
            }
        );
        assert_eq!(
            sampled[2],
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 16
            }
        );
        assert_eq!(sampled[3].a, 0);
        request.track.extrapolate.after = Extrapolate::PingPong;
        request.times_us = vec![1250000, 1750000];
        let mirrored = sample_colors(&request).unwrap();
        assert_eq!(mirrored[0].a, 16);
        assert_eq!(mirrored[1].a, 239);
        request.track.keys[0].out.1 = 0.5;
        assert!(sample_colors(&request).is_err());
    }

    #[test]
    fn pinned_external_color_goldens_match_native_sampling() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/weftcut-animated-color.json"
        ))
        .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let track = &case["track"];
            let keys: Vec<Value> = if track["mode"] == "Static" {
                vec![
                    json!({"t_us":0,"value":track["value"],"in_":[2.0/3.0,2.0/3.0],"out":[1.0/3.0,1.0/3.0],"segment":{"kind":"Linear"}}),
                ]
            } else {
                track["value"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|key| {
                        json!({
                            "t_us":key["t_us"],"value":key["value"],
                            "in_":[key["in"]["x"],key["in"]["y"]],
                            "out":[key["out"]["x"],key["out"]["y"]],"segment":key["segment"]
                        })
                    })
                    .collect()
            };
            let samples = case["samples"].as_array().unwrap();
            let request: ColorSampleRequest = serde_json::from_value(json!({
                "track":{"keys":keys,"extrapolate":track.get("extrapolate").cloned().unwrap_or(json!({"before":"Hold","after":"Hold"}))},
                "default":fixture["default"],"times_us":samples.iter().map(|s| s["t_us"].clone()).collect::<Vec<_>>()
            })).unwrap();
            let actual = sample_colors(&request).unwrap();
            let prepared = PreparedColorTrack::new(request.clone()).unwrap();
            for (sample, color) in samples.iter().zip(actual) {
                assert_eq!(
                    prepared.sample(sample["t_us"].as_f64().unwrap()).unwrap(),
                    color
                );
                let expected: Color = serde_json::from_value(sample["expect"].clone()).unwrap();
                for (got, want) in [
                    (color.r, expected.r),
                    (color.g, expected.g),
                    (color.b, expected.b),
                    (color.a, expected.a),
                ] {
                    assert!(
                        got.abs_diff(want) <= 1,
                        "{} time={} got={color:?} expected={expected:?}",
                        case["name"],
                        sample["t_us"]
                    );
                }
            }
        }
    }

    #[test]
    fn rejects_bad_order_tangents_capacity_and_color_channels() {
        let mut request: ColorSampleRequest = serde_json::from_value(json!({
            "track":{"keys":[{"t_us":0,"value":{"r":255,"g":0,"b":0,"a":255},"in_":[0.5,0.5],"out":[0.5,0.5],"segment":{"kind":"Linear"}}],"extrapolate":{"before":"Hold","after":"Hold"}},
            "default":{"r":0,"g":0,"b":0,"a":255},"times_us":[0]
        })).unwrap();
        request.track.keys.push(request.track.keys[0].clone());
        assert!(sample_colors(&request).is_err());
        request.track.keys.pop();
        request.track.keys[0].out.0 = 1.1;
        assert!(sample_colors(&request).is_err());
        request.track.keys[0].out.0 = 0.5;
        request.times_us = vec![0; 4097];
        assert!(sample_colors(&request).is_err());
        assert!(serde_json::from_value::<Color>(json!({"r":256,"g":0,"b":0,"a":255})).is_err());
        assert!(serde_json::from_value::<Color>(json!({"r":1.5,"g":0,"b":0,"a":255})).is_err());
    }
}

/// Streaming export samples; frame times round exact rational microseconds.
/// No frame array or JavaScript color interpolation is required.
pub struct ColorExportFrames {
    track: PreparedColorTrack,
    numerator: u32,
    denominator: u32,
    frame: u64,
    count: u64,
}
impl ColorExportFrames {
    pub fn new(
        request: ColorSampleRequest,
        numerator: u32,
        denominator: u32,
        duration_us: u64,
    ) -> Result<Self, String> {
        if numerator == 0 || denominator == 0 || duration_us > MAX_SAFE_TIME as u64 {
            return Err("Invalid color export frame rate or duration".into());
        }
        let divisor = 1_000_000u128 * u128::from(denominator);
        let count = (u128::from(duration_us) * u128::from(numerator)).div_ceil(divisor);
        let count = u64::try_from(count).map_err(|_| "Color export frame count overflow")?;
        Ok(Self {
            track: PreparedColorTrack::new(request)?,
            numerator,
            denominator,
            frame: 0,
            count,
        })
    }
    pub fn frame_count(&self) -> u64 {
        self.count
    }
    /// A solid layer uses opaque white RGBA followed by sendcmd and a named
    /// mixer. Commands precede the mixer, so all four gains apply this frame.
    pub fn write_solid_commands(
        mut self,
        writer: &mut impl std::io::Write,
        target: &str,
    ) -> Result<(), String> {
        if target.is_empty()
            || !target
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err("Invalid solid color command target".into());
        }
        while self.frame < self.count {
            let time = u128::from(self.frame) * 1_000_000 * u128::from(self.denominator)
                / u128::from(self.numerator);
            let (_, color) = self.next().ok_or("Missing solid color frame")??;
            writeln!(writer, "{}.{:06} colorchannelmixer@{} rr {:.9}, colorchannelmixer@{} gg {:.9}, colorchannelmixer@{} bb {:.9}, colorchannelmixer@{} aa {:.9};",
                time / 1_000_000, time % 1_000_000,
                target, f64::from(color.r) / 255.0, target, f64::from(color.g) / 255.0,
                target, f64::from(color.b) / 255.0, target, f64::from(color.a) / 255.0)
                .map_err(|error| format!("Cannot write solid color commands: {error}"))?;
        }
        Ok(())
    }
    /// Schedule commands at the floor of the frame timestamp: rounding up
    /// would apply a command one frame late at fractional frame rates.
    /// The graph must render an isolated opaque foreground with drawtext@target,
    /// then colorchannelmixer@target. Applying alpha inside drawtext squares it.
    pub fn write_drawtext_commands(
        mut self,
        writer: &mut impl std::io::Write,
        target: &str,
    ) -> Result<(), String> {
        if target.is_empty()
            || !target
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err("Invalid drawtext command target".into());
        }
        while self.frame < self.count {
            let command_time = u128::from(self.frame) * 1_000_000 * u128::from(self.denominator)
                / u128::from(self.numerator);
            let (_, color) = self.next().ok_or("Missing color export frame")??;
            writeln!(
                writer,
                "{}.{:06} drawtext@{} fontcolor 0x{:02X}{:02X}{:02X}@1.000000000, colorchannelmixer@{} aa {:.9};",
                command_time / 1_000_000,
                command_time % 1_000_000,
                target,
                color.r,
                color.g,
                color.b,
                target,
                f64::from(color.a) / 255.0
            )
            .map_err(|error| format!("Cannot write color commands: {error}"))?;
        }
        Ok(())
    }
}
impl Iterator for ColorExportFrames {
    type Item = Result<(u64, Color), String>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.frame >= self.count {
            return None;
        }
        let numerator = u128::from(self.frame) * 1_000_000 * u128::from(self.denominator);
        let time = (numerator + u128::from(self.numerator) / 2) / u128::from(self.numerator);
        self.frame += 1;
        Some(
            u64::try_from(time)
                .map_err(|_| "Color export time overflow".into())
                .and_then(|time| self.track.sample(time as f64).map(|color| (time, color))),
        )
    }
}

#[cfg(test)]
mod export_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn export_frames_match_batch_with_fractional_fps_and_signed_keys() {
        let request: ColorSampleRequest = serde_json::from_value(json!({
            "track":{"keys":[
                {"t_us":-0.5,"value":{"r":255,"g":0,"b":0,"a":128},"in_":[0.6666666666666666,0.6666666666666666],"out":[0.3333333333333333,0.3333333333333333],"segment":{"kind":"Linear"}},
                {"t_us":1000000.5,"value":{"r":0,"g":0,"b":255,"a":255},"in_":[0.6666666666666666,0.6666666666666666],"out":[0.3333333333333333,0.3333333333333333],"segment":{"kind":"Linear"}}
            ],"extrapolate":{"before":"Hold","after":"Hold"}},
            "default":{"r":0,"g":0,"b":0,"a":255},"times_us":[]
        })).unwrap();
        for (num, den, count, last) in [(20, 1, 20, 950000), (30000, 1001, 30, 967633)] {
            let stream = ColorExportFrames::new(request.clone(), num, den, 1000000).unwrap();
            assert_eq!(stream.frame_count(), count);
            let frames = stream.collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(frames.first().unwrap().0, 0);
            assert_eq!(frames.last().unwrap().0, last);
            let mut batch = request.clone();
            batch.times_us = frames.iter().map(|frame| frame.0 as i64).collect();
            assert_eq!(
                sample_colors(&batch).unwrap(),
                frames.iter().map(|frame| frame.1).collect::<Vec<_>>()
            );
        }
        assert!(ColorExportFrames::new(request.clone(), 0, 1, 1000000).is_err());
        let mut commands = Vec::new();
        ColorExportFrames::new(request.clone(), 30000, 1001, 1000000)
            .unwrap()
            .write_drawtext_commands(&mut commands, "text_1")
            .unwrap();
        let commands = String::from_utf8(commands).unwrap();
        assert!(commands
            .lines()
            .nth(1)
            .unwrap()
            .starts_with("0.033366 drawtext@text_1 fontcolor "));
        assert_eq!(commands.lines().count(), 30);
        let mut solid = Vec::new();
        ColorExportFrames::new(request.clone(), 30000, 1001, 1000000)
            .unwrap()
            .write_solid_commands(&mut solid, "solid_1")
            .unwrap();
        let solid = String::from_utf8(solid).unwrap();
        assert_eq!(solid.lines().count(), 30);
        assert!(solid
            .lines()
            .nth(1)
            .unwrap()
            .starts_with("0.033366 colorchannelmixer@solid_1 rr "));
        assert!(solid
            .lines()
            .all(|line| line.matches("colorchannelmixer@solid_1").count() == 4));
        assert!(ColorExportFrames::new(request.clone(), 30, 1, 1000000)
            .unwrap()
            .write_solid_commands(&mut Vec::new(), "bad;target")
            .is_err());
        assert!(commands
            .lines()
            .all(|line| line.contains("@1.000000000, colorchannelmixer@text_1 aa ")));
        assert!(ColorExportFrames::new(request.clone(), 30, 1, 1000000)
            .unwrap()
            .write_drawtext_commands(&mut Vec::new(), "bad;target")
            .is_err());
        assert_eq!(
            ColorExportFrames::new(request, 30, 1, 0).unwrap().count(),
            0
        );
    }
}
