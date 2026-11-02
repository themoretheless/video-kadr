//! Read-only, cancellation-bound source-frame visual heading observations.
//! IMU removes intentional camera rotation before matching; only a distributed
//! static-background consensus becomes a correction. No foreground/flat fallback.
use crate::domain::{
    gyro_drift::{self, HeadingOffset},
    insta360_camera_profile,
    insta360_gyro::{self, GyroAnalysis, GyroRequest},
    insta360_metadata,
    linear_color_frame::LinearColorFrame,
    spherical_video::{Input, Orientation, Output, SphericalVideo},
    visual_gyro_drift::{self, WorldFrame},
};
use anyhow::{ensure, Context, Result};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
struct CancelMonitor {
    done: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}
impl Drop for CancelMonitor {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
fn monitor(token: CancellationToken) -> (Arc<AtomicBool>, CancelMonitor) {
    let stopped = Arc::new(AtomicBool::new(false));
    let flag = stopped.clone();
    let done = Arc::new(AtomicBool::new(false));
    let finished = done.clone();
    let handle = std::thread::spawn(move || {
        while !finished.load(Ordering::Relaxed) {
            if token.is_cancelled() {
                flag.store(true, Ordering::Relaxed);
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    });
    (
        stopped,
        CancelMonitor {
            done,
            handle: Some(handle),
        },
    )
}
/// Must run under the caller's existing source authorization and Analysis permit.
pub fn analyze_path(
    path: &Path,
    request: &GyroRequest,
    cancel: &CancellationToken,
) -> Result<Option<GyroAnalysis>> {
    request.validate()?;
    ensure!(!cancel.is_cancelled(), "gyro analysis cancelled");
    let mut file = std::fs::File::open(path)?;
    let Some(metadata) = insta360_metadata::inspect(&mut file)? else {
        return Ok(None);
    };
    let mut result = insta360_gyro::analyze_with_metadata(&mut file, &metadata, request)?;
    if !request.visual_drift_correction {
        return Ok(Some(result));
    }
    correct_analysis(path, &metadata, request, &mut result, cancel)?;
    Ok(Some(result))
}
/// Source-authorized callers can use this for an existing orientation solution;
/// e.g. a separately calibrated factory/bias solution. The source path is still
/// decoded read-only and all heading observations retain the same admission.
pub fn correct_analysis(
    path: &Path,
    metadata: &insta360_metadata::Metadata,
    request: &GyroRequest,
    result: &mut GyroAnalysis,
    cancel: &CancellationToken,
) -> Result<()> {
    request.validate()?;
    ensure!(!cancel.is_cancelled(), "visual gyro correction cancelled");
    result
        .drift_correction
        .warnings
        .retain(|w| !w.starts_with("Visual drift correction has not been applied"));
    if result.orientation.len() < 2 {
        result.drift_correction.warnings.push(
            "Image heading correction requires a qualified camera orientation/mounting.".into(),
        );
        return Ok(());
    }
    let profile = insta360_camera_profile::inspect(metadata)
        .profile
        .context("visual gyro correction requires a qualified dual-lens camera profile")?;
    let clock = crate::domain::insta360_video_clock::inspect(&mut std::fs::File::open(path)?)?
        .context("visual gyro correction requires a qualified constant presentation clock")?;
    let fps = clock.numerator as f64 / clock.denominator as f64;
    ensure!(
        request.start_seconds + request.duration_seconds <= clock.sample_count as f64 / fps + 1e-6,
        "visual drift window exceeds the video presentation range"
    );
    let frame_count = ((request.duration_seconds / 0.5).floor() as usize + 1).clamp(2, 32);
    let mut times = vec![];
    let mut last_ordinal = None;
    for i in 0..frame_count {
        let t = if frame_count == 1 {
            0.
        } else {
            request.duration_seconds * (i as f64 / (frame_count - 1) as f64)
        };
        let source = (request.start_seconds + t)
            .min(request.start_seconds + request.duration_seconds - 0.000001);
        let ordinal = (source * fps).floor() as u64;
        if last_ordinal == Some(ordinal) {
            continue;
        }
        let t = ordinal as f64 / fps - request.start_seconds;
        times.push((ordinal, t));
        last_ordinal = Some(ordinal);
    }
    ensure!(
        times.len() >= 2,
        "visual drift correction needs at least two distinct source frames"
    );
    if times
        .first()
        .is_some_and(|(_, t)| *t + result.orientation_offset_seconds < -1e-9)
    {
        result.drift_correction.warnings.push("The first held source frame predates the available orientation guard; request sufficient motion padding before visual correction.".into());
        return Ok(());
    }
    let deadline = Instant::now() + Duration::from_secs(180);
    let (flag, _monitor) = monitor(cancel.clone());
    let mut check = || {
        ensure!(
            !flag.load(Ordering::Relaxed) && !cancel.is_cancelled(),
            "visual gyro correction cancelled"
        );
        ensure!(Instant::now() < deadline, "visual gyro correction timeout");
        Ok(())
    };
    let mut reference: Option<WorldFrame> = None;
    let mut reference_correction = 0.;
    let mut previous: Option<WorldFrame> = None;
    let mut previous_correction = 0.;
    let mut offsets = vec![HeadingOffset {
        time_seconds: result.orientation_offset_seconds,
        radians: 0.,
    }];
    for (ordinal, t) in times {
        check()?;
        let source_seconds = ordinal as f64 / fps;
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .context("visual gyro correction timeout")?
            .min(Duration::from_secs(20));
        let mut source = crate::domain::group_media::decode_source_frame_policy_layout(
            Path::new("ffmpeg"),
            path,
            crate::domain::video_clock::FrameTime::Microseconds(
                (source_seconds * 1e6).round() as u64
            ),
            512,
            256,
            remaining,
            &flag,
            true,
            true,
        )?;
        let mut linear = LinearColorFrame::from_surface(&source, &mut check)?;
        source.rgba.clear();
        let q =
            gyro_drift::orientation_at(&result.orientation, t + result.orientation_offset_seconds)?;
        SphericalVideo {
            input: Input::DualFisheye,
            output: Output::Equirectangular,
            camera_profile: Some(profile.clone()),
            orientation: vec![Orientation {
                time_seconds: 0.,
                quaternion: q,
            }],
            ..Default::default()
        }
        .apply_linear(&mut linear, 0., &mut check)?;
        let frame = WorldFrame {
            width: 512,
            height: 256,
            pixels: linear.pixels().to_vec(),
        };
        result.drift_correction.visual_frames += 1;
        let mut admitted = false;
        if let Some(anchor) = reference.as_ref() {
            let direct = visual_gyro_drift::observe(anchor, &frame, &mut check)?;
            let (observation, base, reset) = if let Some(o) = direct {
                (Some(o), reference_correction, false)
            } else if let Some(last) = previous.as_ref() {
                (
                    visual_gyro_drift::observe(last, &frame, &mut check)?,
                    previous_correction,
                    true,
                )
            } else {
                (None, 0., false)
            };
            if let Some(o) = observation {
                let mut correction = base - o.radians;
                // Below this measured angle, uncertain interpolation noise is not accumulated.
                if correction.abs() < 0.03f64.to_radians() {
                    correction = 0.;
                }
                let last = *offsets.last().unwrap();
                let track_time = t + result.orientation_offset_seconds;
                let dt = track_time - last.time_seconds;
                if dt > 0. && ((correction - last.radians) / dt).abs() <= 0.5f64.to_radians() {
                    offsets.push(HeadingOffset {
                        time_seconds: track_time,
                        radians: correction,
                    });
                    admitted = true;
                    previous_correction = correction;
                    result.drift_correction.visual_pairs += 1;
                    if reset {
                        reference = Some(frame.clone());
                        reference_correction = correction;
                    }
                } else {
                    result.drift_correction.visual_rejected_pairs += 1;
                }
            } else {
                result.drift_correction.visual_rejected_pairs += 1;
            }
        } else {
            reference = Some(frame.clone());
            admitted = true;
        }
        if admitted {
            previous = Some(frame);
        }
    }
    if offsets.len() >= 2 {
        let extent = offsets.last().unwrap().time_seconds - offsets[0].time_seconds;
        if extent >= 0.25 && result.drift_correction.visual_pairs >= 1 {
            result.orientation =
                gyro_drift::corrected_heading_track(&result.orientation, &offsets)?;
            if let Some(tolerance) = request.orientation_tolerance_degrees {
                result.orientation = crate::domain::orientation_track::compress(
                    &result.orientation,
                    tolerance,
                    20_000,
                )?;
                result.drift_correction.warnings.push(format!("Visual heading correction uses sampled orientation knots; combined input/compression knot error budget is {} degrees, not a continuous-time sensor certificate.",tolerance*2.));
            }
            result.drift_correction.corrected_heading_degrees =
                offsets.last().unwrap().radians.to_degrees();
            result.drift_correction.heading_anchors += offsets.len();
            if result.drift_correction.source == "none" {
                result.drift_correction.source = "visual_heading".into()
            } else {
                result.drift_correction.source.push_str("+visual_heading")
            };
        }
    }
    if result.drift_correction.visual_pairs == 0 {
        result.drift_correction.warnings.push("No spatially distributed static-background image consensus; heading was left unchanged.".into());
    }
    if result.drift_correction.visual_rejected_pairs > 0 {
        result.drift_correction.warnings.push("Some source intervals lacked a reliable static-background heading observation; correction holds across unobserved ends and interpolates only admitted observations.".into());
    }
    result.drift_correction.warnings.push("Visual correction observes relative world yaw in a distributed static background. Translation is separated only with admitted calibrated two-view geometry; dynamic backgrounds and absolute compass heading remain unobserved.".into());
    Ok(())
}
