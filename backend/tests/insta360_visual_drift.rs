//! Deterministic end-to-end IMU + encoded dual-lens movie + visual heading tests.
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
use video_kadr_backend::{
    domain::{
        insta360_camera_profile::{CameraProfile, OmniLens},
        insta360_gyro::GyroRequest,
    },
    services::insta360_visual_drift::analyze_path,
};
fn vi(mut v: u64) -> Vec<u8> {
    let mut out = vec![];
    loop {
        let b = (v & 127) as u8;
        v >>= 7;
        out.push(b | if v > 0 { 128 } else { 0 });
        if v == 0 {
            break;
        }
    }
    out
}
fn field(n: u64, v: u64, out: &mut Vec<u8>) {
    out.extend(vi(n << 3));
    out.extend(vi(v));
}
fn bytes(n: u64, value: &[u8], out: &mut Vec<u8>) {
    out.extend(vi(n << 3 | 2));
    out.extend(vi(value.len() as u64));
    out.extend(value);
}
fn profile() -> CameraProfile {
    let lens = |x| OmniLens {
        xi: 1.,
        focal: [50., 50.],
        principal: [x, 63.5],
        euler_degrees: [0., 0., 90.],
        translation: [0.; 3],
        distortion: [0.; 13],
        lens_type: 193,
        angular_polynomial: None,
    };
    CameraProfile {
        model: "insta360_v6_omni".into(),
        reference_size: [256, 128],
        lenses: [lens(63.5), lens(191.5)],
        flags: 0x60400,
    }
}
fn calibration(p: &CameraProfile) -> String {
    let mut values = vec!["2".into()];
    for l in &p.lenses {
        for x in [l.xi, l.focal[0], l.focal[1], l.principal[0], l.principal[1]] {
            values.push(x.to_string())
        }
        for x in l
            .euler_degrees
            .into_iter()
            .chain(l.translation)
            .chain(l.distortion)
        {
            values.push(x.to_string())
        }
        values.extend(["256".into(), "128".into(), "193".into()]);
    }
    values.push(p.flags.to_string());
    values.join("_")
}
fn scene(d: [f64; 3]) -> f64 {
    let lon = d[0].atan2(d[2]);
    let lat = d[1].clamp(-1., 1.).asin();
    let mut value = 0.5;
    for (i, (a, b)) in [
        (3., 9.),
        (7., 5.),
        (13., 11.),
        (17., 3.),
        (23., 17.),
        (29., 7.),
        (31., 13.),
        (11., 19.),
    ]
    .into_iter()
    .enumerate()
    {
        value += 0.042 * (a * lon + b * lat + i as f64 * 0.713).sin();
    }
    value.clamp(0.03, 0.97)
}
fn make(path: &Path, rate_word: u16, actual_rate: f64) {
    let p = profile();
    p.validate().unwrap();
    let rotations = p.optical_rotations();
    let mut input = Vec::with_capacity(100 * 256 * 128 * 4);
    for frame in 0..100 {
        let yaw = actual_rate * frame as f64 / 10.;
        let (s, c) = yaw.sin_cos();
        for y in 0..128 {
            for x in 0..256 {
                let lens = x / 128;
                let l = &p.lenses[lens];
                let px = (x as f64 - l.principal[0]) / l.focal[0];
                let py = (y as f64 - l.principal[1]) / l.focal[1];
                let r = px * px + py * py;
                let optical = [2. * px / (1. + r), 2. * py / (1. + r), (1. - r) / (1. + r)];
                let body: [f64; 3] = std::array::from_fn(|i| {
                    (0..3).map(|k| rotations[lens][k][i] * optical[k]).sum()
                });
                let world = [
                    c * body[0] + s * body[2],
                    body[1],
                    -s * body[0] + c * body[2],
                ];
                let v = (scene(world) * 255.).round() as u8;
                input.extend([v, v, v, 255]);
            }
        }
    }
    let mut child = Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            "256x128",
            "-framerate",
            "10",
            "-i",
            "pipe:0",
            "-filter_complex",
            "[0:v]split=2[a][b];[a]crop=128:128:0:0[l];[b]crop=128:128:128:0[r]",
            "-map",
            "[l]",
            "-map",
            "[r]",
            "-c:v",
            "libx264rgb",
            "-crf",
            "0",
            "-bf",
            "0",
            "-g",
            "10",
            "-pix_fmt",
            "rgb24",
            "-video_track_timescale",
            "10000",
            "-threads",
            "1",
        ])
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&input).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut metadata = vec![];
    bytes(2, b"Insta360 X6", &mut metadata);
    field(24, 1_000_000, &mut metadata);
    field(51, 1, &mut metadata);
    field(62, 1, &mut metadata);
    field(64, 2, &mut metadata);
    field(20, 10, &mut metadata);
    bytes(111, calibration(&p).as_bytes(), &mut metadata);
    let mut config = vec![];
    field(1, 32, &mut config);
    field(2, 2000, &mut config);
    bytes(65, &config, &mut metadata);
    let mut gyro = vec![];
    for i in 0..=1020 {
        gyro.extend((900_000u64 + i * 10_000).to_le_bytes());
        for word in [31744u16, 32768, 32768, rate_word, 32768, 32768] {
            gyro.extend(word.to_le_bytes())
        }
    }
    let mut exposure = vec![];
    for i in 0..100 {
        exposure.extend((1_000_000u64 + i * 100_000).to_le_bytes());
        exposure.extend(0.02f64.to_le_bytes());
    }
    let mut trailer = vec![];
    let mut index = vec![0u8; 50];
    for (kind, format, payload) in [(1usize, 1u8, metadata), (3, 0, gyro), (4, 0, exposure)] {
        index[kind * 10] = kind as u8;
        index[kind * 10 + 1] = format;
        index[kind * 10 + 2..kind * 10 + 6].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        index[kind * 10 + 6..kind * 10 + 10].copy_from_slice(&(trailer.len() as u32).to_le_bytes());
        let length = payload.len();
        trailer.extend(payload);
        trailer.extend([format, kind as u8]);
        trailer.extend((length as u32).to_le_bytes());
    }
    trailer.extend(index);
    trailer.extend([0, 0]);
    trailer.extend(50u32.to_le_bytes());
    let extent = trailer.len() + 72;
    trailer.extend([0; 32]);
    trailer.extend((extent as u32).to_le_bytes());
    trailer.extend(3u32.to_le_bytes());
    trailer.extend(b"8db42d694ccc418790edff439fe026bf");
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(&trailer)
        .unwrap();
}
fn yaw(q: [f64; 4]) -> f64 {
    (2. * (q[1] * q[3] + q[0] * q[2])).atan2(1. - 2. * (q[1] * q[1] + q[2] * q[2]))
}
#[test]
#[ignore = "requires local ffmpeg/libx264rgb; explicit integration qualification"]
fn automatic_visual_heading_removes_bias_and_preserves_real_constant_pan() {
    let dir = tempfile::tempdir().unwrap();
    let stationary = dir.path().join("stationary.mp4");
    make(&stationary, 32772, 0.);
    let cancel = tokio_util::sync::CancellationToken::new();
    let request = GyroRequest {
        duration_seconds: 8.,
        orientation_tolerance_degrees: Some(0.005),
        ..Default::default()
    };
    let baseline = analyze_path(&stationary, &request, &cancel)
        .unwrap()
        .unwrap();
    let corrected = analyze_path(
        &stationary,
        &GyroRequest {
            visual_drift_correction: true,
            ..request.clone()
        },
        &cancel,
    )
    .unwrap()
    .unwrap();
    let before = yaw(baseline.orientation.last().unwrap().quaternion).to_degrees();
    let after = yaw(corrected.orientation.last().unwrap().quaternion).to_degrees();
    println!(
        "stationary injected gyro bias: before={before:.6}deg after={after:.6}deg report={:?}",
        corrected.drift_correction
    );
    assert!(before > 1.8);
    assert!(corrected.drift_correction.visual_pairs >= 2);
    assert!(after.abs() < 0.45 && after.abs() < before.abs() * 0.25);
    let pan = dir.path().join("pan.mp4");
    let rate = 28. / 32768. * 2000. * std::f64::consts::PI / 180.;
    make(&pan, 32796, rate);
    let base = analyze_path(&pan, &request, &cancel).unwrap().unwrap();
    let preserved = analyze_path(
        &pan,
        &GyroRequest {
            visual_drift_correction: true,
            ..request
        },
        &cancel,
    )
    .unwrap()
    .unwrap();
    let a = yaw(base.orientation.last().unwrap().quaternion).to_degrees();
    let b = yaw(preserved.orientation.last().unwrap().quaternion).to_degrees();
    println!(
        "genuine constant pan: before={a:.6}deg after={b:.6}deg report={:?}",
        preserved.drift_correction
    );
    assert!(a > 13.);
    assert!(preserved.drift_correction.visual_pairs >= 2);
    assert!((a - b).abs() < 0.35 && b > 12.5);
}
#[test]
fn cancelled_source_analysis_never_opens_a_source() {
    let token = tokio_util::sync::CancellationToken::new();
    token.cancel();
    let result = analyze_path(
        Path::new("/no-such-source"),
        &GyroRequest::default(),
        &token,
    );
    assert!(result.unwrap_err().to_string().contains("cancelled"));
}

#[test]
#[ignore = "requires explicitly authorized local media inventory and ffmpeg"]
fn real_x6_background_observation_admission_and_injected_heading_error() {
    use video_kadr_backend::domain::{
        group_media, gyro_drift, insta360_metadata, insta360_video_clock,
        linear_color_frame::LinearColorFrame,
        spherical_video::{Input, Orientation, Output, SphericalVideo},
        visual_gyro_drift::{self, WorldFrame},
    };
    use video_kadr_backend::services::insta360_visual_drift::correct_analysis;
    let inventory =
        std::env::var("VIDEO_KADR_INSTA360_INVENTORY").expect("supply authorized inventory");
    let rows: serde_json::Value =
        serde_json::from_slice(&std::fs::read(inventory).unwrap()).unwrap();
    let minimum_bytes = std::env::var("VIDEO_KADR_INSTA360_REAL_MIN_BYTES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100_000_000);
    let require_visual =
        std::env::var("VIDEO_KADR_INSTA360_REQUIRE_VISUAL").map_or(true, |v| v != "0");
    let row = rows["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["bytes"].as_u64().unwrap_or(0) >= minimum_bytes)
        .min_by_key(|r| r["bytes"].as_u64().unwrap())
        .unwrap();
    let path = Path::new(row["path"].as_str().unwrap());
    let mut file = std::fs::File::open(path).unwrap();
    let meta = insta360_metadata::inspect(&mut file).unwrap().unwrap();
    let profile = video_kadr_backend::domain::insta360_camera_profile::inspect(&meta)
        .profile
        .unwrap();
    let clock = insta360_video_clock::inspect(&mut file).unwrap().unwrap();
    let fps = clock.numerator as f64 / clock.denominator as f64;
    let duration = (clock.sample_count as f64 / fps * 0.7).min(2.5);
    let request = GyroRequest {
        duration_seconds: duration,
        orientation_tolerance_degrees: Some(0.005),
        motion_padding_seconds: 0.03,
        ..Default::default()
    };
    let token = tokio_util::sync::CancellationToken::new();
    let mut injected = analyze_path(path, &request, &token).unwrap().unwrap();
    let offset = injected.orientation_offset_seconds;
    for s in &mut injected.orientation {
        let angle = 0.004 * (s.time_seconds - offset);
        let (hc, hs) = ((angle * 0.5).cos(), (angle * 0.5).sin());
        let q = s.quaternion;
        s.quaternion = [
            hc * q[0] - hs * q[2],
            hc * q[1] + hs * q[3],
            hc * q[2] + hs * q[0],
            hc * q[3] - hs * q[1],
        ];
    }
    let before = injected.clone();
    correct_analysis(
        path,
        &meta,
        &GyroRequest {
            visual_drift_correction: true,
            ..request.clone()
        },
        &mut injected,
        &token,
    )
    .unwrap();
    if injected.drift_correction.visual_pairs == 0 {
        println!(
            "real X6 visual correction declined: {:?}",
            injected.drift_correction
        );
        assert!(
            !require_visual,
            "required real-background evidence was not admitted"
        );
        assert_eq!(
            injected.orientation, before.orientation,
            "unobserved heading was modified"
        );
        return;
    }
    fn world(path: &Path, p: &CameraProfile, t: f64, q: [f64; 4]) -> WorldFrame {
        let src = group_media::decode_source_frame_policy_layout(
            Path::new("ffmpeg"),
            path,
            video_kadr_backend::domain::video_clock::FrameTime::Microseconds(
                (t * 1e6).round() as u64
            ),
            512,
            256,
            std::time::Duration::from_secs(20),
            &std::sync::atomic::AtomicBool::new(false),
            true,
            true,
        )
        .unwrap();
        let mut linear = LinearColorFrame::from_surface(&src, &mut || Ok(())).unwrap();
        SphericalVideo {
            input: Input::DualFisheye,
            output: Output::Equirectangular,
            camera_profile: Some(p.clone()),
            orientation: vec![Orientation {
                time_seconds: 0.,
                quaternion: q,
            }],
            ..Default::default()
        }
        .apply_linear(&mut linear, 0., &mut || Ok(()))
        .unwrap();
        let surface = linear.to_surface(&mut || Ok(())).unwrap();
        WorldFrame {
            width: surface.width,
            height: surface.height,
            pixels: surface
                .rgba
                .chunks_exact(4)
                .map(|p| {
                    [
                        p[0] as f32 / 255.,
                        p[1] as f32 / 255.,
                        p[2] as f32 / 255.,
                        p[3] as f32 / 255.,
                    ]
                })
                .collect(),
        }
    }
    let end = ((duration - 0.000001) * fps).floor() / fps;
    let first = world(
        path,
        &profile,
        0.,
        gyro_drift::orientation_at(&before.orientation, offset).unwrap(),
    );
    let old = world(
        path,
        &profile,
        end,
        gyro_drift::orientation_at(&before.orientation, end + offset).unwrap(),
    );
    let new = world(
        path,
        &profile,
        end,
        gyro_drift::orientation_at(&injected.orientation, end + offset).unwrap(),
    );
    let old_error = visual_gyro_drift::observe(&first, &old, &mut || Ok(()))
        .unwrap()
        .expect("real scene needs admitted background evidence");
    let new_error = visual_gyro_drift::observe(&first, &new, &mut || Ok(()))
        .unwrap()
        .expect("corrected real scene background evidence");
    println!("real X6 background drift: injected_extra={:.6}deg, visual_before={:.6}deg visual_after={:.6}deg report={:?}",(0.004*end).to_degrees(),old_error.radians.to_degrees(),new_error.radians.to_degrees(),injected.drift_correction);
    assert!(injected.drift_correction.visual_pairs >= 1);
    assert!(new_error.radians.abs() < (old_error.radians.abs() * 0.5).max(0.08f64.to_radians()));
}
