//! Explicit, read-only original-media qualification. Images and paths stay in a
//! caller-selected PRIVATE output directory; coverage is not a parity certificate.
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
use video_kadr_backend::domain::{
    composition::VideoEffect,
    flat_surface, group_media, insta360_camera_profile, insta360_gyro, insta360_metadata,
    insta360_video_clock,
    spherical_optical_flow::{Mode, Stitching},
    spherical_video::{Input, Output, SourceLayout, SphericalVideo, StabilizationMode},
    video_clock::FrameTime,
};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Case {
    id: String,
    source: PathBuf,
    layout: SourceLayout,
    times_seconds: Vec<f64>,
    expected_model: Option<String>,
    stabilization_mode: Option<StabilizationMode>,
}
fn qualify(case: &Case, out: &Path) -> Result<Value> {
    let mut file = File::open(&case.source)?;
    let before = file.metadata()?;
    let Some(metadata) = insta360_metadata::inspect(&mut file)? else {
        return Ok(json!({"status":"unsupported_no_insta360_trailer"}));
    };
    if let Some(expected) = &case.expected_model {
        ensure!(
            metadata.camera_model.as_ref() == Some(expected),
            "camera model differs from declared fixture"
        );
    }
    let report = insta360_camera_profile::inspect(&metadata);
    let mut result = json!({"model":metadata.camera_model,"firmware":metadata.firmware,"sourceBytes":before.len(),"profileStatus":report.status,"calibrationField":report.calibration_field,"sourceLayout":case.layout});
    let Some(profile) = report.profile else {
        result["status"] = json!("unsupported_calibration");
        result["reason"] = json!(report.reason);
        return Ok(result);
    };
    result["profileReferenceSize"] = json!(profile.reference_size);
    result["hasSourceTimeMap"] = json!(metadata.blocks.iter().any(|b| b.kind == 128));
    result["timingBoundary"] = json!(
        "CFR is a presentation-clock check; capture/gyro time-map qualification is separate."
    );
    // Keep timing qualification separate: a stitchable source need not prove CFR.
    result["clock"] = match insta360_video_clock::inspect(&mut file) {
        Ok(clock) => serde_json::to_value(clock)?,
        Err(e) => json!({"unqualified":e.to_string()}),
    };
    let stabilization_mode = case
        .stabilization_mode
        .unwrap_or(StabilizationMode::Disabled);
    let gyro = if stabilization_mode != StabilizationMode::Disabled {
        let duration = case.times_seconds.iter().copied().fold(0., f64::max) + 0.1;
        let analysis = insta360_gyro::analyze_with_metadata(
            &mut file,
            &metadata,
            &insta360_gyro::GyroRequest {
                duration_seconds: duration,
                sample_rate_hz: 120.,
                orientation_tolerance_degrees: Some(0.05),
                ..Default::default()
            },
        )?;
        ensure!(
            analysis.status == "orientation_ready",
            "automatic gyro orientation unavailable"
        );
        result["gyro"] = serde_json::to_value(&analysis)?;
        Some(analysis)
    } else {
        None
    };
    result["stabilizationMode"] = serde_json::to_value(stabilization_mode)?;
    let folder = out.join(&case.id);
    std::fs::create_dir(&folder)?;
    let mut frames = vec![];
    for (index, &time) in case.times_seconds.iter().enumerate() {
        let packed = group_media::decode_source_frame_policy_layout(
            Path::new("ffmpeg"),
            &case.source,
            FrameTime::Microseconds((time * 1e6).round() as u64),
            1024,
            512,
            Duration::from_secs(45),
            &AtomicBool::new(false),
            true,
            case.layout == SourceLayout::DualVideoStreams,
        )?;
        let decoded_hash = format!("{:x}", Sha256::digest(&packed.rgba));
        let mut variants = vec![];
        for (name, mode) in [("feather", Mode::Feather), ("flow", Mode::OpticalFlow)] {
            let mut image = packed.clone();
            let parameters = SphericalVideo {
                input: Input::DualFisheye,
                output: Output::Equirectangular,
                source_layout: case.layout,
                camera_profile: Some(profile.clone()),
                stabilization_mode,
                orientation: gyro
                    .as_ref()
                    .map_or_else(Vec::new, |g| g.orientation.clone()),
                orientation_offset_seconds: gyro
                    .as_ref()
                    .map_or(0., |g| g.orientation_offset_seconds),
                source_frame_clock: gyro.as_ref().and_then(|g| g.source_frame_clock.clone()),
                stitching: Stitching {
                    mode,
                    ..Default::default()
                },
                ..Default::default()
            };
            let start = Instant::now();
            flat_surface::apply_effect_stack(
                &mut image,
                &[VideoEffect::SphericalVideo {
                    parameters,
                    animation: None,
                }],
                (time * 1e6).round() as u64,
                &[],
                &mut || {
                    ensure!(
                        start.elapsed() < Duration::from_secs(30),
                        "qualification render deadline"
                    );
                    Ok(())
                },
            )?;
            let destination = folder.join(format!("{index:02}-{name}.png"));
            image::save_buffer(
                &destination,
                &image.rgba,
                1024,
                512,
                image::ColorType::Rgba8,
            )?;
            let opaque = image.rgba.chunks_exact(4).filter(|p| p[3] == 255).count();
            ensure!(
                opaque >= 1024 * 512 * 995 / 1000,
                "calibrated render has insufficient opaque coverage"
            );
            variants.push(json!({"variant":name,"rgbaSha256":format!("{:x}",Sha256::digest(&image.rgba)),"pngSha256":format!("{:x}",Sha256::digest(std::fs::read(&destination)?)),"opaquePixels":opaque,"totalPixels":1024*512,"renderSeconds":start.elapsed().as_secs_f64()}));
        }
        frames
            .push(json!({"timeSeconds":time,"decodedRgbaSha256":decoded_hash,"variants":variants}));
    }
    let after = file.metadata()?;
    ensure!(
        before.len() == after.len() && before.modified()? == after.modified()?,
        "original source changed during qualification"
    );
    result["status"] = json!("rendered_requires_reference_review");
    result["frames"] = json!(frames);
    result["identityBoundary"]=json!("Source size/mtime rechecked; exact decoded RGBA and output hashes retained. Original full-content hash was not computed.");
    Ok(result)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    ensure!(
        args.len() == 3,
        "usage: qualify_insta360_matrix /absolute/cases.json /absolute/NEW-private-output"
    );
    let input = Path::new(&args[1]);
    let out = Path::new(&args[2]);
    ensure!(
        input.is_absolute() && out.is_absolute() && !out.exists(),
        "use absolute input and a fresh private output directory"
    );
    let raw = std::fs::read(input)?;
    ensure!(raw.len() < 65536, "case manifest exceeds budget");
    let cases: Vec<Case> = serde_json::from_slice(&raw)?;
    ensure!(
        !cases.is_empty() && cases.len() <= 8,
        "supply1..8 explicit camera cases"
    );
    let mut ids = std::collections::BTreeSet::new();
    for c in &cases {
        ensure!(
            !c.id.is_empty()
                && c.id.len() <= 48
                && c.id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                && ids.insert(&c.id),
            "case IDs must be unique safe names"
        );
        ensure!(
            c.source.is_absolute() && c.source.is_file(),
            "original source must be an existing absolute file"
        );
        ensure!(
            !c.times_seconds.is_empty()
                && c.times_seconds.len() <= 4
                && c.times_seconds
                    .iter()
                    .all(|t| t.is_finite() && (0. ..=600.).contains(t)),
            "supply1..4 finite frame times within600seconds"
        );
    }
    std::fs::create_dir(out)?;
    let mut rows = vec![];
    let mut failed = 0;
    for case in &cases {
        let mut r = qualify(case, out)
            .unwrap_or_else(|e| json!({"status":"failed","reason":format!("{e:#}")}));
        if r["status"] != "rendered_requires_reference_review" {
            failed += 1;
        }
        r["id"] = json!(case.id);
        println!("{}: {}", case.id, r["status"]);
        rows.push(r);
    }
    let value = json!({"cases":rows,"manifestSha256":format!("{:x}",Sha256::digest(&raw)),"boundary":"Read-only native source/calibration/render checks. Successful decode and opaque coverage do not establish vendor quality parity. Compare fixed source frames and Studio settings independently; private source/output images must not be published."});
    std::fs::write(
        out.join("qualification.json"),
        serde_json::to_vec_pretty(&value)?,
    )
    .context("write private qualification ledger")?;
    ensure!(
        failed == 0,
        "{failed} camera cases unsupported or failed; inspect private qualification.json"
    );
    Ok(())
}
