//! Read-only raw INSV IMU extraction and independent complementary orientation fusion.
//! X6 raw words are offset binary, not signed i16. Camera axes are deliberately
//! recovered from a qualified embedded calibration or supplied explicitly; the
//! camera name alone does not establish an IMU mounting matrix.
use super::{
    insta360_metadata::{self, Metadata},
    spherical_video::Orientation,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    f64::consts::PI,
    io::{Read, Seek, SeekFrom},
};
const MAX_OUTPUT: usize = 20_000;
const MAX_INPUT: usize = 1_000_000;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct GyroRequest {
    pub start_seconds: f64,
    pub duration_seconds: f64,
    pub sample_rate_hz: f64,
    /// Unit quaternion mapping sensor coordinates to renderer camera coordinates.
    pub camera_from_sensor: Option<[f64; 4]>,
    /// Explicit capture-clock correction; positive values sample later IMU data.
    pub time_offset_seconds: f64,
    pub accelerometer_correction: bool,
    pub drift_correction: super::gyro_drift::DriftCorrection,
    pub visual_drift_correction: bool,
    pub orientation_tolerance_degrees: Option<f64>,
    pub motion_padding_seconds: f64,
    /// Observed IMU history used only for a qualified retained trim map.
    pub gravity_preroll_seconds: f64,
}
impl Default for GyroRequest {
    fn default() -> Self {
        Self {
            start_seconds: 0.,
            duration_seconds: 10.,
            sample_rate_hz: 60.,
            camera_from_sensor: None,
            time_offset_seconds: 0.,
            accelerometer_correction: true,
            drift_correction: Default::default(),
            visual_drift_correction: false,
            orientation_tolerance_degrees: None,
            motion_padding_seconds: 0.,
            gravity_preroll_seconds: 1.,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SensorSample {
    /// Relative to requested clip start, after the explicit time correction.
    pub time_seconds: f64,
    /// Sensor axes; g units (one g is standard gravity).
    pub acceleration_g: [f64; 3],
    /// Sensor axes; radians/second.
    pub radians_per_second: [f64; 3],
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GravityAlignment {
    pub status: String,
    pub warning: Option<String>,
    pub requested_force_norm_g: f64,
    pub observed_preroll_seconds: f64,
    pub initialization_samples: usize,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GyroAnalysis {
    pub status: String,
    pub mounting_source: String,
    pub gravity_alignment: GravityAlignment,
    pub synchronization: String,
    pub timing_warning: Option<String>,
    pub limitations: Vec<String>,
    pub drift_correction: super::gyro_drift::DriftReport,
    pub input_samples: usize,
    pub capture_start_seconds: f64,
    pub capture_end_seconds: f64,
    pub samples: Vec<SensorSample>,
    pub orientation: Vec<Orientation>,
    pub orientation_offset_seconds: f64,
    pub capture_clock: Vec<super::orientation_track::CaptureTime>,
    pub source_frame_clock: Option<super::rolling_shutter::SourceFrameClock>,
    pub original_time_map_offset_seconds: Option<f64>,
}
fn norm(v: [f64; 3]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn mul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}
fn inverse(q: [f64; 4]) -> [f64; 4] {
    [q[0], -q[1], -q[2], -q[3]]
}
fn unit(q: [f64; 4]) -> Result<[f64; 4]> {
    let n = q.iter().map(|x| x * x).sum::<f64>().sqrt();
    ensure!(
        q.iter().all(|x| x.is_finite()) && (n - 1.).abs() < 1e-3,
        "cameraFromSensor must be a unit quaternion"
    );
    Ok(q.map(|v| v / n))
}
fn rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let a = mul(mul(q, [0., v[0], v[1], v[2]]), inverse(q));
    [a[1], a[2], a[3]]
}
fn normalized(q: [f64; 4]) -> [f64; 4] {
    let n = q.iter().map(|x| x * x).sum::<f64>().sqrt();
    q.map(|v| v / n)
}
fn slerp(mut a: [f64; 4], mut b: [f64; 4], t: f64) -> [f64; 4] {
    let mut d = a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    if d < 0. {
        b = b.map(|v| -v);
        d = -d;
    }
    if d > 0.9995 {
        return normalized(std::array::from_fn(|i| a[i] + t * (b[i] - a[i])));
    }
    let theta = d.clamp(-1., 1.).acos();
    let scale = theta.sin();
    for i in 0..4 {
        a[i] = (a[i] * ((1. - t) * theta).sin() + b[i] * (t * theta).sin()) / scale;
    }
    normalized(a)
}
impl GyroRequest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.start_seconds.is_finite() && self.start_seconds >= 0.,
            "invalid gyro startSeconds"
        );
        ensure!(
            self.duration_seconds.is_finite()
                && self.duration_seconds > 0.
                && self.duration_seconds <= 600.,
            "gyro durationSeconds must be in (0,600]"
        );
        ensure!(
            self.sample_rate_hz.is_finite()
                && self.sample_rate_hz >= 1.
                && self.sample_rate_hz <= 120.,
            "gyro sampleRateHz must be in [1,120]"
        );
        ensure!(
            (self.duration_seconds * self.sample_rate_hz).ceil() as usize + 1 <= MAX_OUTPUT,
            "gyro output exceeds 20000 samples"
        );
        ensure!(
            self.time_offset_seconds.is_finite() && self.time_offset_seconds.abs() <= 60.,
            "invalid gyro timeOffsetSeconds"
        );
        self.drift_correction.validate(self.duration_seconds)?;
        ensure!(
            self.motion_padding_seconds.is_finite()
                && (0. ..=0.2).contains(&self.motion_padding_seconds)
                && (self.motion_padding_seconds == 0.
                    || self.orientation_tolerance_degrees.is_some()),
            "motion padding requires a precise orientation track and must be 0..0.2 seconds"
        );
        ensure!(
            !self.visual_drift_correction || self.drift_correction.heading_anchors.is_empty(),
            "choose visual or explicit heading observations"
        );
        if let Some(tolerance) = self.orientation_tolerance_degrees {
            ensure!(
                tolerance.is_finite() && (0.000001..=1.).contains(&tolerance),
                "invalid orientation compression tolerance"
            );
        }
        ensure!(
            self.gravity_preroll_seconds.is_finite()
                && (0. ..=1.).contains(&self.gravity_preroll_seconds),
            "gravityPrerollSeconds must be0..1 observed seconds"
        );
        if let Some(q) = self.camera_from_sensor {
            unit(q)?;
        }
        Ok(())
    }
}
/// Independent body-rate integration with accelerometer correction. Gravity only
/// constrains roll/pitch; yaw is unobservable and can drift. With correction enabled, the first valid
/// gravity magnitude supplies an assumed initial world +Y; heading is arbitrary.
/// Later automatic feedback requires observed low-motion windows and is limited
/// to 0.25 degrees total relative to gyro-only integration. Specific force alone
/// cannot prove gravity in arbitrary motion; larger bias needs explicit calibration.
/// Assumes calibrated g/rad/s input with the declared camera-axis mounting.
pub fn fuse(
    samples: &[SensorSample],
    camera_from_sensor: [f64; 4],
    correct_acceleration: bool,
) -> Result<Vec<Orientation>> {
    fuse_with_acceleration_polarity(samples, camera_from_sensor, correct_acceleration, 1., None)
}
fn fuse_with_acceleration_polarity(
    samples: &[SensorSample],
    camera_from_sensor: [f64; 4],
    correct_acceleration: bool,
    acceleration_polarity: f64,
    initial_acceleration: Option<[f64; 3]>,
) -> Result<Vec<Orientation>> {
    let map = unit(camera_from_sensor)?;
    ensure!(
        samples.len() >= 2 && samples.len() <= MAX_INPUT,
        "invalid gyro fusion sample count"
    );
    let mut previous = -f64::INFINITY;
    for s in samples {
        ensure!(
            s.time_seconds.is_finite()
                && s.time_seconds > previous
                && s.acceleration_g
                    .iter()
                    .chain(&s.radians_per_second)
                    .all(|v| v.is_finite()),
            "invalid or non-increasing IMU samples"
        );
        previous = s.time_seconds;
    }
    let first_accel = rotate(
        map,
        initial_acceleration
            .unwrap_or(samples[0].acceleration_g)
            .map(|v| v * acceleration_polarity),
    );
    let magnitude = norm(first_accel);
    let gravity_reference = if correct_acceleration && (0.85..=1.15).contains(&magnitude) {
        Some([0., 1., 0.])
    } else {
        None
    };
    let mut q = if gravity_reference.is_some() {
        tilt_from_up(first_accel.map(|v| v / magnitude))
    } else {
        [1., 0., 0., 0.]
    };
    let observations=if correct_acceleration {gravity_observations(samples)}else{vec![]};
    let mut gyro_only=q;
    let mut result = Vec::with_capacity(samples.len());
    result.push(Orientation {
        time_seconds: samples[0].time_seconds,
        quaternion: q,
    });
    for (sample_index,pair) in samples.windows(2).enumerate() {
        let dt = pair[1].time_seconds - pair[0].time_seconds;
        ensure!(dt <= 0.1, "gyro sampling gap exceeds 100ms");
        let mut rate = rotate(map,std::array::from_fn(|i| {
            (pair[0].radians_per_second[i]+pair[1].radians_per_second[i])*0.5
        }));
        gyro_only=normalized(mul(gyro_only,rate_increment(rate,dt)));
        if let (Some(reference),Some(raw_mean))=(gravity_reference,observations.get(sample_index+1).copied().flatten()) {
            let a=rotate(map,raw_mean.map(|v|v*acceleration_polarity));
            let measured=a.map(|v|v/norm(a));
            // A stable 1g window is still not a proof of gravity. Require small
            // innovation against an INDEPENDENT gyro-only trajectory; otherwise
            // reject lateral force rather than following it into false tilt.
            let predicted=rotate(inverse(gyro_only),reference);
            let innovation=measured.iter().zip(predicted).map(|(a,b)|a*b).sum::<f64>().clamp(-1.,1.).acos();
            if innovation<=1f64.to_radians() {
                let error=cross(measured,rotate(inverse(q),reference));
                for i in 0..3 {rate[i]+=2.*error[i];}
            }
        }
        q=normalized(mul(q,rate_increment(rate,dt)));
        if correct_acceleration {
            // Absolute correction budget, not a per-sample bound: slow force
            // ramps cannot ratchet the gravity estimate away from observed gyro.
            let mut candidate=q;
            let dot=candidate.iter().zip(gyro_only).map(|(a,b)|a*b).sum::<f64>();
            if dot<0. {candidate=candidate.map(|v|-v);}
            let angle=2.*dot.abs().clamp(-1.,1.).acos();
            let budget=0.25f64.to_radians();
            if angle>budget {let f=budget/angle;q=normalized(std::array::from_fn(|i|gyro_only[i]*(1.-f)+candidate[i]*f));}
        }
        result.push(Orientation {
            time_seconds: pair[1].time_seconds,
            quaternion: q,
        });
    }
    Ok(result)
}
fn rate_increment(rate:[f64;3],dt:f64)->[f64;4] {
    let theta=norm(rate)*dt;
    if theta<1e-12 {[1.,rate[0]*dt/2.,rate[1]*dt/2.,rate[2]*dt/2.]}else {
        let k=(theta/2.).sin()*dt/theta;[(theta/2.).cos(),rate[0]*k,rate[1]*k,rate[2]*k]
    }
}
/// Trailing observed half-second windows; O(n), bounded to existing input cap.
/// A low-motion window is necessary but not sufficient evidence for gravity.
fn gravity_observations(samples:&[SensorSample])->Vec<Option<[f64;3]>> {
    let mut out=vec![None;samples.len()];let mut left=0;let mut sum=[0.;3];let mut squares=0.;let mut bad=0usize;
    let admissible=|s:&SensorSample| (0.97..=1.03).contains(&norm(s.acceleration_g))&&norm(s.radians_per_second)<=0.1;
    for (right,s) in samples.iter().enumerate() {
        for i in 0..3 {sum[i]+=s.acceleration_g[i];}squares+=norm(s.acceleration_g).powi(2);bad+=usize::from(!admissible(s));
        while samples[right].time_seconds-samples[left].time_seconds>0.5 {
            let old=&samples[left];for i in 0..3 {sum[i]-=old.acceleration_g[i];}squares-=norm(old.acceleration_g).powi(2);bad-=usize::from(!admissible(old));left+=1;
        }
        let n=right-left+1;
        if bad==0&&n>=10&&samples[right].time_seconds-samples[left].time_seconds>=0.45 {
            let mean=sum.map(|v|v/n as f64);let scatter=(squares/n as f64-norm(mean).powi(2)).max(0.).sqrt();
            if scatter<=0.003 {out[right]=Some(mean);}
        }
    }
    out
}
fn tilt_from_up(measured: [f64; 3]) -> [f64; 4] {
    if measured[1] < -1. + 1e-9 {
        [0., 1., 0., 0.]
    } else {
        let axis = cross(measured, [0., 1., 0.]);
        normalized([1. + measured[1], axis[0], axis[1], axis[2]])
    }
}
#[derive(Clone, Copy, Debug)]
struct GravitySeed {
    time: f64,
    acceleration: [f64; 3],
    samples: usize,
}
/// An observed low-motion window, not an inferred still/bias declaration. Never
/// searches more than one second and never consumes unobserved clip padding.
fn observed_gravity_seed(
    samples: &[SensorSample],
    before: f64,
    history: f64,
) -> Option<GravitySeed> {
    let first = samples.partition_point(|s| s.time_seconds < before - history);
    let end = samples.partition_point(|s| s.time_seconds < before);
    if end - first > 4096 {
        return None;
    }
    let mut best: Option<(f64, GravitySeed)> = None;
    let mut right = first;
    for left in first..end {
        right = right.max(left + 1);
        while right < end && samples[right - 1].time_seconds - samples[left].time_seconds < 0.05 {
            right += 1;
        }
        let window = &samples[left..right];
        if window.len() < 10 || window.last()?.time_seconds - window[0].time_seconds < 0.05 {
            break;
        }
        if window.last()?.time_seconds - window[0].time_seconds > 0.075 {
            continue;
        }
        if window.iter().any(|s| norm(s.radians_per_second) > 0.35) {
            continue;
        }
        let mean: [f64; 3] = std::array::from_fn(|i| {
            window.iter().map(|s| s.acceleration_g[i]).sum::<f64>() / window.len() as f64
        });
        let magnitude = norm(mean);
        if !(0.9..=1.1).contains(&magnitude) {
            continue;
        }
        let scatter = (window
            .iter()
            .map(|s| {
                (0..3)
                    .map(|i| (s.acceleration_g[i] - mean[i]).powi(2))
                    .sum::<f64>()
            })
            .sum::<f64>()
            / window.len() as f64)
            .sqrt();
        if scatter > 0.03 {
            continue;
        }
        let score = scatter + (magnitude - 1.).abs();
        let seed = GravitySeed {
            time: window[window.len() / 2].time_seconds,
            acceleration: mean,
            samples: window.len(),
        };
        if best.is_none_or(|(old, _)| score < old) {
            best = Some((score, seed));
        }
    }
    best.map(|(_, seed)| seed)
}
fn record<R: Read + Seek>(reader: &mut R, offset: u64) -> Result<(u64, [f64; 3], [f64; 3])> {
    reader.seek(SeekFrom::Start(offset))?;
    let mut bytes = [0u8; 20];
    reader.read_exact(&mut bytes)?;
    let timestamp = u64::from_le_bytes(bytes[..8].try_into()?);
    let words: [f64; 6] = std::array::from_fn(|i| {
        u16::from_le_bytes([bytes[8 + i * 2], bytes[9 + i * 2]]) as f64 - 32768.
    });
    Ok((
        timestamp,
        [words[0], words[1], words[2]],
        [words[3], words[4], words[5]],
    ))
}
/// Reconstruct an ordinary constant-rate exposure clock when the metadata says
/// PTS type2 and the capture timestamp identifies the first exposure exactly.
/// Exposure times are seconds; exposure records carry microsecond timestamps.
fn capture_at(clock: &[super::orientation_track::CaptureTime], time: f64) -> f64 {
    let i = clock.partition_point(|p| p.time_seconds <= time);
    if i == 0 {
        return clock[0].capture_seconds;
    }
    if i == clock.len() {
        return clock[i - 1].capture_seconds;
    }
    let (a, b) = (clock[i - 1], clock[i]);
    a.capture_seconds
        + (b.capture_seconds - a.capture_seconds) * (time - a.time_seconds)
            / (b.time_seconds - a.time_seconds)
}
fn linear_targets(r: &GyroRequest) -> (Vec<f64>, Vec<super::orientation_track::CaptureTime>) {
    let n = (r.duration_seconds * r.sample_rate_hz).ceil() as usize;
    let base = r.start_seconds + r.time_offset_seconds;
    let pad = r.motion_padding_seconds;
    let clock = vec![
        super::orientation_track::CaptureTime {
            time_seconds: 0.,
            capture_seconds: base - pad,
        },
        super::orientation_track::CaptureTime {
            time_seconds: r.duration_seconds + 2. * pad,
            capture_seconds: base + r.duration_seconds + pad,
        },
    ];
    (
        (0..=n)
            .map(|i| base + (i as f64 / r.sample_rate_hz).min(r.duration_seconds))
            .collect(),
        clock,
    )
}
fn targets<R: Read + Seek>(
    reader: &mut R,
    m: &Metadata,
    r: &GyroRequest,
    first: u64,
    trim: Option<super::insta360_time_map::AffineTrim>,
) -> Result<(
    Vec<f64>,
    &'static str,
    Option<String>,
    Vec<super::orientation_track::CaptureTime>,
)> {
    let n = (r.duration_seconds * r.sample_rate_hz).ceil() as usize;
    let Some(block) = m
        .blocks
        .iter()
        .find(|b| b.kind == 4)
        .filter(|_| m.pts_type == Some(2))
    else {
        ensure!(
            trim.is_none(),
            "embedded time map requires a qualified exposure synchronization path"
        );
        let (values, clock) = linear_targets(r);
        return Ok((values, "first_frame_capture_clock", None, clock));
    };
    ensure!(
        block.format == 0 && block.length >= 32 && block.length % 16 == 0,
        "invalid exposure synchronization block"
    );
    let video = super::insta360_video_clock::inspect(reader)?
        .context("exposure synchronization requires a qualified MP4 CFR clock")?;
    let fps = video.numerator as f64 / video.denominator as f64;
    ensure!(
        fps.is_finite() && (1. ..=240.).contains(&fps) && video.sample_count >= 2,
        "unsupported video frame rate/count"
    );
    let count = block.length as usize / 16;
    let read = |reader: &mut R, index: usize| -> Result<(u64, f64)> {
        ensure!(index < count, "missing exposure record");
        reader.seek(SeekFrom::Start(block.offset + index as u64 * 16))?;
        let mut b = [0; 16];
        reader.read_exact(&mut b)?;
        let ts = u64::from_le_bytes(b[..8].try_into()?);
        let exposure = f64::from_le_bytes(b[8..].try_into()?);
        ensure!(
            exposure.is_finite() && exposure > 0. && exposure <= 1. / fps * 1.05,
            "invalid frame exposure duration"
        );
        Ok((ts, exposure))
    };
    let (mut lo, mut hi) = (0, count);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if read(reader, mid)?.0 < first {
            lo = mid + 1
        } else {
            hi = mid
        }
    }
    ensure!(
        lo < count && read(reader, lo)?.0 == first,
        "first-frame timestamp does not identify an exposure record"
    );
    let first_index = lo
        .checked_add(trim.map_or(0, |t| t.source_frame_offset))
        .context("time-map exposure offset overflow")?;
    if trim.is_some() {
        ensure!(
            first_index
                .checked_add(video.sample_count as usize)
                .is_some_and(|end| end <= count),
            "time-map original exposure range is incomplete"
        );
    }

    let last_frame = (r.start_seconds + r.duration_seconds) * fps;
    ensure!(
        last_frame.is_finite() && last_frame <= video.sample_count as f64 + 1e-8,
        "requested exposure window outside video presentation range"
    );
    let pad = r.motion_padding_seconds;
    let start = (r.start_seconds - pad) * fps;
    let end = (r.start_seconds + r.duration_seconds + pad) * fps;
    ensure!(
        start.is_finite()
            && end.is_finite()
            && start >= -49.
            && end <= video.sample_count as f64 + 49.,
        "invalid padded exposure range"
    );
    let begin = start.floor() as i64;
    let finish = end.ceil() as i64;
    ensure!(
        finish > begin && finish - begin <= 145000,
        "exposure clock exceeds knot budget"
    );
    let mut extended = false;
    let record_at = |reader: &mut R, index: i64| -> Result<((f64, f64), bool)> {
        if index >= 0 && (index as u64) < video.sample_count {
            let (ts, shutter) = read(
                reader,
                first_index
                    .checked_add(index as usize)
                    .context("exposure index overflow")?,
            )?;
            return Ok(((ts as f64, shutter), false));
        }
        let (edge, neighbour, steps) = if index < 0 {
            (0, 1, index as f64)
        } else {
            let last = video.sample_count as usize - 1;
            (last, last - 1, index as f64 - last as f64)
        };
        let (ts, shutter) = read(
            reader,
            first_index
                .checked_add(edge)
                .context("exposure index overflow")?,
        )?;
        let (other, _) = read(
            reader,
            first_index
                .checked_add(neighbour)
                .context("exposure index overflow")?,
        )?;
        let delta = if index < 0 {
            other as f64 - ts as f64
        } else {
            ts as f64 - other as f64
        };
        ensure!(
            delta > 0. && (delta / 1e6 - 1. / fps).abs() <= 0.1 / fps,
            "invalid edge capture-clock slope"
        );
        Ok(((ts as f64 + steps * delta, shutter), true))
    };
    let mut events = Vec::with_capacity((finish - begin + 1) as usize);
    let mut last_capture: Option<f64> = None;
    for index in begin..=finish {
        let ((timestamp, shutter), outside) = record_at(reader, index)?;
        extended |= outside;
        if let Some(previous) = last_capture {
            ensure!(
                timestamp > previous
                    && ((timestamp - previous) / 1e6 - 1. / fps).abs() <= 0.1 / fps,
                "irregular exposure clock; explicit PTS mapping required"
            )
        }
        last_capture = Some(timestamp);
        let capture = (timestamp - first as f64) / 1e6 - shutter * 0.5 + r.time_offset_seconds;
        ensure!(events.last().is_none_or(|p:&super::orientation_track::CaptureTime|capture>p.capture_seconds),"non-increasing exposure midpoint clock");
        events.push(super::orientation_track::CaptureTime {
            time_seconds: index as f64 / fps - r.start_seconds + pad,
            capture_seconds: capture,
        });
    }
    let first_time = 0.;
    let last_time = r.duration_seconds + 2. * pad;
    let first_capture = capture_at(&events, first_time);
    let last_capture = capture_at(&events, last_time);
    let mut clock = vec![super::orientation_track::CaptureTime {
        time_seconds: first_time,
        capture_seconds: first_capture,
    }];
    clock.extend(
        events
            .into_iter()
            .filter(|p| p.time_seconds > first_time + 1e-12 && p.time_seconds < last_time - 1e-12),
    );
    clock.push(super::orientation_track::CaptureTime {
        time_seconds: last_time,
        capture_seconds: last_capture,
    });
    // The clock is piecewise-linear in physical capture time. Compression keeps
    // its actual exposure/shutter breakpoints to a one-nanosecond budget.
    let clock = super::orientation_track::compress_clock(&clock, 1e-9)?;
    let values = (0..=n)
        .map(|i| {
            capture_at(
                &clock,
                (i as f64 / r.sample_rate_hz).min(r.duration_seconds) + pad,
            )
        })
        .collect();
    let warning=extended.then(||"Presentation endpoint/guard-band capture-clock labels outside the real video interval use an affine edge slope and held edge shutter solely as an invertible guard-band coordinate; no unobserved exposure is asserted. Actual interior exposure events are retained.".into());
    Ok((
        values,
        if trim.is_some() {
            "exposure_midpoint_trim_map"
        } else {
            "exposure_midpoint_linear"
        },
        warning,
        clock,
    ))
}
/// Extract a bounded source interval, integrating every native sample before
/// resampling. Output times are clip-local. Qualified raw X6/X4 paths are explicit.
pub fn analyze<R: Read + Seek>(
    reader: &mut R,
    request: &GyroRequest,
) -> Result<Option<GyroAnalysis>> {
    request.validate()?;
    let Some(metadata) = insta360_metadata::inspect(reader)? else {
        return Ok(None);
    };
    analyze_with_metadata(reader, &metadata, request).map(Some)
}
pub fn analyze_with_metadata<R: Read + Seek>(
    reader: &mut R,
    m: &Metadata,
    r: &GyroRequest,
) -> Result<GyroAnalysis> {
    r.validate()?;
    ensure!(
        matches!(
            m.camera_model.as_deref(),
            Some("Insta360 X6" | "Insta360 X4")
        ) && m.gyro_type == Some(1)
            && m.is_raw_gyro == Some(true),
        "unsupported embedded IMU model/encoding; no guessed sensor layout"
    );
    // Validate retained native time maps before approximate fallback or manual mounting.
    let trim = super::insta360_time_map::inspect(reader, m)?;
    let [acc_range, gyro_range] = m.gyro_config.context("missing gyro range configuration")?;
    ensure!(
        [2, 4, 8, 16, 32].contains(&acc_range)
            && [125, 250, 500, 1000, 2000, 4000].contains(&gyro_range),
        "unsupported IMU configured range"
    );
    let first = m
        .first_frame_timestamp
        .filter(|v| *v >= 0)
        .context("missing first-frame capture timestamp")? as u64;
    let block = m
        .blocks
        .iter()
        .find(|b| b.kind == 3)
        .context("missing embedded gyro block")?;
    ensure!(
        block.format == 0 && block.length % 20 == 0 && block.length >= 40,
        "invalid raw gyro block"
    );
    let total = block.length as usize / 20;
    let timestamp = |reader: &mut R, index: usize| -> Result<u64> {
        Ok(record(reader, block.offset + index as u64 * 20)?.0)
    };
    let first_imu = timestamp(reader, 0)?;
    let last_imu = timestamp(reader, total - 1)?;
    ensure!(last_imu > first_imu, "non-increasing gyro capture range");
    let origin = first as f64 / 1e6;
    let (targets, synchronization, mut timing_warning, precise_clock) = match targets(
        reader, m, r, first, trim,
    ) {
        Ok(value) => value,
        Err(e) => {
            if trim.is_some() {
                anyhow::bail!("qualified time-map capture synchronization failed: {e}");
            }
            let (values, clock) = linear_targets(r);
            (values,"first_frame_capture_clock",Some(format!("Exposure synchronization unavailable: {e}. Capture-clock alignment is approximate; provide explicit timing calibration.")),clock)
        }
    };
    let source_frame_clock = match super::insta360_video_clock::inspect(reader) {
        Ok(Some(clock)) => Some(super::rolling_shutter::SourceFrameClock {
            numerator: clock.numerator,
            denominator: clock.denominator,
            source_start_seconds: r.start_seconds,
        }),
        Ok(None) => None,
        Err(e) => {
            if timing_warning.is_none() {
                timing_warning=Some(format!("Source presentation frame clock is unqualified: {e}; source-frame gyro phase cannot be inferred."));
            }
            None
        }
    };
    let capture_begin = precise_clock[0].capture_seconds;
    let capture_end = precise_clock[precise_clock.len() - 1].capture_seconds;
    let target_start = origin + capture_begin;
    let target_end = origin + capture_end;
    ensure!(
        target_start >= first_imu as f64 / 1e6 && target_end <= last_imu as f64 / 1e6,
        "requested gyro interval outside capture range"
    );
    // Binary search only finds the range; strict timestamp and gap checks cover
    // every record consumed in the selected interval before any orientation is returned.
    let search = |reader: &mut R, t: f64| -> Result<usize> {
        let (mut lo, mut hi) = (0, total);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if timestamp(reader, mid)? as f64 / 1e6 < t {
                lo = mid + 1
            } else {
                hi = mid
            }
        }
        Ok(lo)
    };
    let available_preroll = if trim.is_some() && r.accelerometer_correction {
        r.gravity_preroll_seconds
    } else {
        0.
    };
    let read_start = (target_start - available_preroll).max(first_imu as f64 / 1e6);
    let begin = search(reader, read_start)?.saturating_sub(1);
    // Keep one outer bracketing record as well: adding the original trim
    // offset can put an exact microsecond endpoint a few floating ULPs above
    // its serialized timestamp. This avoids extrapolation or a false gap.
    let end = (search(reader, target_end)? + 2).min(total);
    ensure!(
        end - begin <= MAX_INPUT,
        "gyro input window exceeds one million samples"
    );
    reader.seek(SeekFrom::Start(block.offset + begin as u64 * 20))?;
    let mut raw = vec![0u8; (end - begin) * 20];
    reader.read_exact(&mut raw)?;
    let mut native = Vec::with_capacity(end - begin);
    let mut previous = None;
    for bytes in raw.chunks_exact(20) {
        let ts = u64::from_le_bytes(bytes[..8].try_into()?);
        if let Some(prev) = previous {
            ensure!(
                ts > prev && ts - prev <= 100_000,
                "invalid gyro timestamp order or gap"
            );
        }
        previous = Some(ts);
        let values: [f64; 6] = std::array::from_fn(|i| {
            (u16::from_le_bytes([bytes[8 + 2 * i], bytes[9 + 2 * i]]) as f64 - 32768.) / 32768.
        });
        native.push(SensorSample {
            time_seconds: ts as f64 / 1e6 - origin,
            acceleration_g: std::array::from_fn(|i| values[i] * acc_range as f64),
            radians_per_second: std::array::from_fn(|i| {
                values[i + 3] * gyro_range as f64 * PI / 180.
            }),
        });
    }
    ensure!(
        native.len() >= 2
            && native[0].time_seconds <= capture_begin
            && native.last().is_some_and(|s| s.time_seconds >= capture_end),
        "gyro samples do not bracket requested interval"
    );
    let index = native
        .partition_point(|s| s.time_seconds <= capture_begin)
        .saturating_sub(1)
        .min(native.len() - 2);
    let (a, b) = (native[index], native[index + 1]);
    let fraction =
        ((capture_begin - a.time_seconds) / (b.time_seconds - a.time_seconds)).clamp(0., 1.);
    let requested_acceleration: [f64; 3] = std::array::from_fn(|i| {
        a.acceleration_g[i] + fraction * (b.acceleration_g[i] - a.acceleration_g[i])
    });
    let requested_norm = norm(requested_acceleration);
    let seed = if r.accelerometer_correction
        && !(0.85..=1.15).contains(&requested_norm)
        && available_preroll > 0.
    {
        observed_gravity_seed(&native, capture_begin, available_preroll)
    } else {
        None
    };
    let integration_begin = seed.map_or(capture_begin, |s| s.time);
    let discard = native
        .partition_point(|s| s.time_seconds <= integration_begin)
        .saturating_sub(1);
    if discard > 0 {
        native.drain(..discard);
    }
    // Start integration exactly at the requested capture midpoint, interpolating
    // the boundary rate/force. A bracket sample is not extra integration time.
    while native.len() > 2 && native[1].time_seconds <= integration_begin {
        native.remove(0);
    }
    if native[0].time_seconds < integration_begin {
        let (a, b) = (native[0], native[1]);
        let f = (integration_begin - a.time_seconds) / (b.time_seconds - a.time_seconds);
        native[0] = SensorSample {
            time_seconds: integration_begin,
            acceleration_g: std::array::from_fn(|i| {
                a.acceleration_g[i] + f * (b.acceleration_g[i] - a.acceleration_g[i])
            }),
            radians_per_second: std::array::from_fn(|i| {
                a.radians_per_second[i] + f * (b.radians_per_second[i] - a.radians_per_second[i])
            }),
        };
    }
    let capture_time = |t: f64| capture_at(&precise_clock, t + r.motion_padding_seconds);
    let capture_windows: Vec<_> = r
        .drift_correction
        .stationary_intervals
        .iter()
        .map(|w| super::gyro_drift::StationaryInterval {
            start_seconds: capture_time(w.start_seconds),
            end_seconds: capture_time(w.end_seconds),
        })
        .collect();
    let mut drift_report =
        super::gyro_drift::calibrate_bias(&native, &r.drift_correction, &capture_windows)?;
    let corrected_native = if drift_report
        .bias_radians_per_second
        .iter()
        .any(|v| *v != 0.)
    {
        Some(super::gyro_drift::bias_corrected(
            &native,
            drift_report.bias_radians_per_second,
        ))
    } else {
        None
    };
    let integration_samples = corrected_native.as_deref().unwrap_or(&native);
    let (mounting, mounting_source) = if let Some(map) = r.camera_from_sensor {
        (Some(map), "explicit")
    } else {
        let profile = super::insta360_camera_profile::inspect(m).profile;
        let x6 = m.camera_model.as_deref() == Some("Insta360 X6")
            && profile
                .as_ref()
                .is_some_and(|p| p.flags == 0x60400 && p.lenses.iter().all(|l| l.lens_type == 193));
        let x4 = m.camera_model.as_deref() == Some("Insta360 X4")
            && profile.as_ref().is_some_and(|p| {
                p.model == "insta360_legacy_polynomial"
                    && p.flags == 1095
                    && p.lenses.iter().all(|l| l.lens_type == 71)
            });
        if x6 || x4 {
            // Native flow-map consumes the enum145 quaternion directly; the
            // separate ARV hint bridge is not an input to that ray mapper.
            // Its captured longitude starts at zero at raster x=0, so the
            // canonical centre ray is native -X. Accounting for that half-turn,
            // editor rates=[-z,x,-y] and specific force=[z,-x,y]. The polar
            // sign is applied only below; profile lens coordinates stay fixed.
            (
                Some([0.5, -0.5, -0.5, 0.5]),
                if x4 {
                    "embedded_legacy_145"
                } else {
                    "embedded_v6_145"
                },
            )
        } else {
            (None, "unresolved")
        }
    };
    let mut fused = if let Some(map) = mounting {
        fuse_with_acceleration_polarity(
            integration_samples,
            map,
            r.accelerometer_correction,
            if matches!(mounting_source, "embedded_v6_145" | "embedded_legacy_145") {
                -1.
            } else {
                1.
            },
            seed.map(|s| s.acceleration),
        )?
    } else {
        vec![]
    };
    let gravity_alignment = if !r.accelerometer_correction {
        GravityAlignment {
            status: "disabled".into(),
            warning: None,
            requested_force_norm_g: requested_norm,
            observed_preroll_seconds: 0.,
            initialization_samples: 0,
        }
    } else if mounting.is_none() {
        GravityAlignment {
            status: "sensor_axes_required".into(),
            warning: Some(
                "Camera mounting is unqualified; absolute gravity alignment is unavailable.".into(),
            ),
            requested_force_norm_g: requested_norm,
            observed_preroll_seconds: 0.,
            initialization_samples: 0,
        }
    } else if let Some(seed) = seed {
        // Preserve the authored clip's canonical yaw origin. Both poses align
        // the same estimated gravity, so their difference is a world-Y phase.
        let initial = super::gyro_drift::orientation_at(&fused, capture_time(0.))?;
        let up = rotate(inverse(initial), [0., 1., 0.]);
        let phase = normalized(mul(tilt_from_up(up), inverse(initial)));
        for pose in &mut fused {
            pose.quaternion = normalized(mul(phase, pose.quaternion));
        }
        GravityAlignment {
            status: "estimated_observed_preroll".into(),
            warning: Some("Horizon is estimated from a short observed pre-roll segment; precise absolute gravity alignment is not established.".into()),
            requested_force_norm_g: requested_norm,
            observed_preroll_seconds: capture_begin - seed.time,
            initialization_samples: seed.samples,
        }
    } else if (0.85..=1.15).contains(&requested_norm) {
        GravityAlignment {
            status: "aligned_initial_sample".into(),
            warning: None,
            requested_force_norm_g: requested_norm,
            observed_preroll_seconds: 0.,
            initialization_samples: 1,
        }
    } else {
        GravityAlignment{status:"unavailable_dynamic_start".into(),warning:Some("Absolute gravity alignment was not established: initial acceleration is outside the gravity band and no admissible observed pre-roll window was found. Gyro-relative orientation remains available.".into()),requested_force_norm_g:requested_norm,observed_preroll_seconds:0.,initialization_samples:0}
    };
    // Padding is an observation guard, not a new authored world origin. With
    // gravity disabled the requested clip start, not the pre-roll start, is identity.
    if !r.accelerometer_correction && !fused.is_empty() {
        let initial = super::gyro_drift::orientation_at(&fused, targets[0])?;
        let inverse_initial = inverse(initial);
        for sample in &mut fused {
            sample.quaternion = normalized(mul(inverse_initial, sample.quaternion));
        }
    }
    let n = (r.duration_seconds * r.sample_rate_hz).ceil() as usize;
    let mut samples = Vec::with_capacity(n + 1);
    let mut orientation = Vec::with_capacity(if fused.is_empty() { 0 } else { n + 1 });
    let mut index = 0;
    for i in 0..=n {
        let t = (i as f64 / r.sample_rate_hz).min(r.duration_seconds);
        let target = targets[i];
        while index + 2 < native.len() && native[index + 1].time_seconds < target {
            index += 1;
        }
        let (a, b) = (native[index], native[index + 1]);
        let fraction =
            ((target - a.time_seconds) / (b.time_seconds - a.time_seconds)).clamp(0., 1.);
        samples.push(SensorSample {
            time_seconds: t,
            acceleration_g: std::array::from_fn(|j| {
                a.acceleration_g[j] + fraction * (b.acceleration_g[j] - a.acceleration_g[j])
            }),
            radians_per_second: std::array::from_fn(|j| {
                a.radians_per_second[j]
                    + fraction * (b.radians_per_second[j] - a.radians_per_second[j])
            }),
        });
        if !fused.is_empty() {
            orientation.push(Orientation {
                time_seconds: t,
                quaternion: slerp(
                    fused[index].quaternion,
                    fused[index + 1].quaternion,
                    fraction,
                ),
            });
        }
    }
    let mut heading_config = r.drift_correction.clone();
    for anchor in &mut heading_config.heading_anchors {
        anchor.time_seconds += r.motion_padding_seconds;
    }
    if let Some(tolerance) = r.orientation_tolerance_degrees {
        if !fused.is_empty() {
            let mut dense =
                super::orientation_track::capture_to_clip_from_clock(&fused, &precise_clock)?;
            super::gyro_drift::apply_observed_headings(
                &mut dense,
                &heading_config,
                &mut drift_report,
            )?;
            orientation = super::orientation_track::compress(&dense, tolerance, MAX_OUTPUT)?;
        }
    } else if !orientation.is_empty() {
        super::gyro_drift::apply_observed_headings(
            &mut orientation,
            &heading_config,
            &mut drift_report,
        )?;
        if orientation.len() > MAX_OUTPUT {
            orientation = super::orientation_track::compress(&orientation, 0.005, MAX_OUTPUT)?;
        }
    }
    if r.visual_drift_correction {
        drift_report.warnings.push("Visual drift correction has not been applied by the raw-reader analysis; use the source-frame service.".into());
    }
    Ok(GyroAnalysis{status:if mounting.is_some(){"orientation_ready"}else{"sensor_axes_required"}.into(),mounting_source:mounting_source.into(),gravity_alignment,synchronization:synchronization.into(),timing_warning,drift_correction:drift_report,limitations:vec!["Synchronization uses qualified ordinary capture/exposure clocks and explicit timeOffsetSeconds. Arbitrary PTS lists and multi-segment retiming require explicit source-time calibration. Per-row correction uses separately configured calibrated source readout.".into(),"Mounting comes from qualified v6 lens193/version4 or X4 legacy flags1095/lens71, or an explicit unit quaternion; other profiles require calibration. Automatic gravity feedback requires observed low-motion windows, <=1 degree gyro-only innovation, and a total 0.25 degree correction budget; arbitrary constant-norm acceleration is not observable gravity. Independent complementary fusion does not establish FlowState equivalence. Yaw drift requires explicit bias/still observations or admitted image-heading observations; acceleration alone cannot observe it.".into()],input_samples:native.len(),capture_start_seconds:first_imu as f64/1e6-first as f64/1e6,capture_end_seconds:last_imu as f64/1e6-first as f64/1e6,samples,orientation,orientation_offset_seconds:r.motion_padding_seconds,source_frame_clock,original_time_map_offset_seconds:trim.map(|t|t.source_start_seconds),capture_clock:if r.orientation_tolerance_degrees.is_some(){precise_clock}else{vec![]}})
}
