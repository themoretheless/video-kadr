use video_kadr_backend::domain::{
    insta360_camera_profile::{inspect, parse_legacy, parse_v6},
    insta360_metadata,
};
fn calibration() -> String {
    let mut fields = vec!["2".to_owned()];
    for cx in [3840, 11520] {
        let mut lens = vec![
            "2.4".into(),
            "7200".into(),
            "7200".into(),
            cx.to_string(),
            "3840".into(),
            "0".into(),
            "0".into(),
            "90".into(),
        ];
        lens.extend(std::iter::repeat_n("0".into(), 16));
        lens.extend(["15360".into(), "7680".into(), "193".into()]);
        fields.extend(lens);
    }
    fields.push("394240".into());
    fields.join("_")
}
#[test]
fn v6_profile_retains_native_model_without_equidistant_approximation() {
    let p = parse_v6(&calibration()).unwrap();
    assert_eq!(p.reference_size, [15360, 7680]);
    assert_eq!(p.lenses[1].principal, [11520., 3840.]);
    assert_eq!(
        p.lenses[0].project_optical([0., 0., 1.]),
        Some([3840., 3840.])
    );
    let point = p.lenses[0].project_optical([1., 0., 1.]).unwrap();
    assert!((point[0] - (3840. + 7200. / (1. + 2.4 * 2f64.sqrt()))).abs() < 1e-10);
    assert!(p.lenses[0].project_optical([0., 0., -1.]).is_none());
    assert!(p.lenses[0].project_optical([f64::NAN, 0., 1.]).is_none());
    let json = serde_json::to_string(&p).unwrap();
    assert_eq!(
        serde_json::from_str::<video_kadr_backend::domain::insta360_camera_profile::CameraProfile>(
            &json
        )
        .unwrap(),
        p
    );
}
#[test]
fn v6_profile_checks_shape_flags_and_nonfinite_terms() {
    let good = calibration();
    for bad in [
        good.replace("2.4", "NaN"),
        good.replace("394240", "196608"),
        format!("{good}_0"),
        good.replace("193", "255"),
    ] {
        assert!(parse_v6(&bad).is_err());
    }
    let mut p = parse_v6(&good).unwrap();
    p.lenses[0].distortion[0] = f64::INFINITY;
    assert!(p.validate().is_err());
}
#[test]
fn unqualified_camera_calibrations_never_fall_back_to_model_defaults() {
    use insta360_metadata::{Calibration, Metadata};
    let mut metadata = Metadata {
        version: 3,
        camera_model: Some("Insta360 X4".into()),
        firmware: None,
        dimension: None,
        window_crop_info: None,
        resolution: None,
        frame_rate: None,
        first_frame_timestamp: None,
        gyro_type: None,
        gyro_config: None,
        gyro_timestamp: None,
        has_gyro_timestamp: None,
        rolling_shutter_time: None,
        pts_type: None,
        is_raw_gyro: None,
        calibrations: vec![],
        blocks: vec![],
    };
    assert_eq!(inspect(&metadata).status, "missing_calibration");
    // A v6-shaped payload in an older field is not proof of that field's model.
    for field in [5, 17, 197] {
        metadata.calibrations = vec![Calibration {
            field_number: field,
            value: calibration(),
        }];
        let report = inspect(&metadata);
        assert_eq!(report.status, "unsupported_calibration");
        assert!(!report.render_ready && report.profile.is_none());
    }
    // A malformed current calibration cannot silently select the factory copy.
    metadata.calibrations = vec![
        Calibration {
            field_number: 111,
            value: "unknown".into(),
        },
        Calibration {
            field_number: 112,
            value: calibration(),
        },
    ];
    let report = inspect(&metadata);
    assert_eq!(report.calibration_field, Some(111));
    assert!(!report.render_ready && report.profile.is_none());
    // Native parser branch lengths are source-derived; the v6 renderer must not
    // reinterpret their shorter records as its extended distortion model.
    for (terms_per_lens, flags) in [(6, 0), (16, 2 << 16), (19, 3 << 16)] {
        let mut tokens = vec!["2".to_owned()];
        tokens.extend(std::iter::repeat_n("0".to_owned(), terms_per_lens * 2));
        if flags == 0 {
            tokens.extend(["7680".to_owned(), "3840".to_owned()]);
        }
        tokens.push(flags.to_string());
        assert!(parse_v6(&tokens.join("_")).is_err());
    }
    metadata.calibrations = vec![Calibration {
        field_number: 5,
        value: "2_3000_4000_3000_0_0_90_3000_12000_3000_0_0_90_16000_6000_1095".into(),
    }];
    assert!(
        !inspect(&metadata).render_ready,
        "source crop must not be guessed"
    );
    metadata.window_crop_info = Some(insta360_metadata::WindowCropInfo {
        source_size: [8000, 6000],
        destination_size: [5760, 5760],
        offset: [0, 0],
    });
    assert_eq!(
        inspect(&metadata).profile.unwrap().reference_size,
        [11520, 5760]
    );
}
#[test]
fn observed_single_lens_v6_shape_is_not_enabled_as_dual_fisheye() {
    // A real X6 MP4 had count1, square7744 reference, lens193 and flags0x60000.
    // Preserve only that structural observation; all intrinsics here are synthetic.
    let dual = calibration();
    let mut mono: Vec<String> = dual.split('_').take(28).map(str::to_owned).collect();
    mono[0] = "1".into();
    mono[25] = "7744".into();
    mono[26] = "7744".into();
    mono.push((6u32 << 16).to_string());
    assert_eq!(mono.len(), 29);
    assert!(parse_v6(&mono.join("_")).is_err());
    // Actual GO Ultra metadata supplies a10-token mono legacy lens132/flags1156.
    // Numeric intrinsics remain synthetic; the dual-lens71 contract must refuse it.
    assert!(parse_legacy("1_4800_4100_3090_0_0_90_8192_6144_1156").is_err());
}
#[test]
fn legacy_lens71_uses_native_angle_polynomial_and_anisotropic_reference() {
    // Synthetic numeric intrinsics; shape/flags are source-derived from official X4 footage.
    let p = parse_legacy("2_3000_4000_3000_0_0_90_3000_12000_3000_0_0_90_16000_6000_1095").unwrap();
    assert_eq!(p.reference_size, [16000, 6000]);
    assert_eq!(p.model, "insta360_legacy_polynomial");
    let lens = &p.lenses[0];
    assert_eq!(lens.project_optical([0., 0., 1.]), Some([4000., 3000.]));
    let theta = 30f64.to_radians();
    // Independent native degree-domain polynomial oracle, before normalized/radian conversion.
    let degree_radius =
        |d: f64| d * (0.02249 + d * (-0.0000579 + d * (0.000001932 + d * -0.00000001295)));
    let expected = 4000. + 3000. * degree_radius(30.) / degree_radius(100.);
    let uv = lens
        .project_optical([theta.sin(), 0., theta.cos()])
        .unwrap();
    assert!((uv[0] - expected).abs() < 1e-9);
    assert_eq!(uv[1], 3000.);
    let edge = 100f64.to_radians();
    assert!((lens.project_optical([edge.sin(), 0., edge.cos()]).unwrap()[0] - 7000.).abs() < 1e-9);
    assert!(lens.project_optical([0., 0., -1.]).is_none());
    for bad in [
        "2_0",
        "2_3000_4000_3000_0_0_90_3000_12000_3000_0_0_90_16000_6000_1096",
    ] {
        assert!(parse_legacy(bad).is_err());
    }
    let mut invalid = p.clone();
    invalid.lenses[0].angular_polynomial = None;
    assert!(invalid.validate().is_err());
    let json = serde_json::to_string(&p).unwrap();
    assert_eq!(
        serde_json::from_str::<video_kadr_backend::domain::insta360_camera_profile::CameraProfile>(
            &json
        )
        .unwrap(),
        p
    );
}
#[test]
#[ignore = "requires explicitly supplied verified original X4 footage"]
fn legacy_x4_original_metadata_selects_source_owned_profile() {
    let path = std::env::var("VIDEO_KADR_INSTA360_LEGACY_SAMPLE").unwrap();
    let mut file = std::fs::File::open(path).unwrap();
    let metadata = insta360_metadata::inspect(&mut file).unwrap().unwrap();
    assert_eq!(metadata.camera_model.as_deref(), Some("Insta360 X4"));
    let report = inspect(&metadata);
    assert!(report.render_ready, "{}", report.reason);
    assert_eq!(report.calibration_field, Some(5));
    let p = report.profile.unwrap();
    assert_eq!(p.flags, 1095);
    assert_eq!(p.reference_size, [11520, 5760]);
    assert!(p
        .lenses
        .iter()
        .all(|l| l.lens_type == 71 && l.angular_polynomial.is_some()));
    println!("source-owned X4 legacy2/lens71 profile; corrected active reference11520x5760; field5; flags1095");
}
#[test]
fn legacy_sensor_window_center_crop_preserves_physical_fisheye_scale() {
    use insta360_metadata::WindowCropInfo;
    let mut p =
        parse_legacy("2_3000_4000_3000_0_0_90_3000_12000_3000_0_0_90_16000_6000_1095").unwrap();
    let focal = p.lenses[0].focal;
    p.apply_window_crop(&WindowCropInfo {
        source_size: [8000, 6000],
        destination_size: [5760, 5760],
        offset: [0, 0],
    })
    .unwrap();
    assert_eq!(p.reference_size, [11520, 5760]);
    assert_eq!(p.lenses[0].principal, [2880., 2880.]);
    assert_eq!(p.lenses[1].principal, [8640., 2880.]);
    assert_eq!(p.lenses[0].focal, focal);
    let theta = 30f64.to_radians();
    let x = p.lenses[0]
        .project_optical([theta.sin(), 0., theta.cos()])
        .unwrap();
    let y = p.lenses[0]
        .project_optical([0., theta.sin(), theta.cos()])
        .unwrap();
    assert!((x[0] - 2880. - (y[1] - 2880.)).abs() < 1e-10);
    assert!(p
        .apply_window_crop(&WindowCropInfo {
            source_size: [8000, 6000],
            destination_size: [5760, 5760],
            offset: [2000, 0]
        })
        .is_err());
}
#[test]
fn v6_extended_tangential_and_prism_terms_are_not_dropped() {
    let mut p = parse_v6(&calibration()).unwrap();
    let l = &mut p.lenses[0];
    l.xi = 0.;
    l.focal = [1., 1.];
    l.principal = [0., 0.];
    l.distortion = [1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12., 13.];
    let x: f64 = 0.1;
    let y: f64 = 0.2;
    // Hand-evaluated decimal oracle exercises every coefficient's position.
    let expected = [1.38154015625, 2.0180803125];
    let actual = l.project_optical([x, y, 1.]).unwrap();
    for i in 0..2 {
        assert!((actual[i] - expected[i]).abs() < 1e-12);
    }
}
#[test]
fn v6_native_pose_places_both_lenses_in_opposite_hemispheres() {
    let p = parse_v6(&calibration()).unwrap();
    let m = p.optical_rotations();
    let ray = [0.3, 0.4, 0.5];
    for (i, expected) in [[0.3, -0.4, 0.5], [-0.3, -0.4, -0.5]]
        .into_iter()
        .enumerate()
    {
        for (row, value) in m[i].iter().zip(expected) {
            assert!((row.iter().zip(ray).map(|(a, b)| a * b).sum::<f64>() - value).abs() < 1e-12);
        }
    }
}
#[test]
fn v6_calibration_is_used_by_native_render_stack() {
    use video_kadr_backend::domain::{
        composition::VideoEffect,
        flat_surface,
        group_surface::GroupSurface,
        spherical_video::{Input, SphericalVideo},
    };
    let profile = parse_v6(&calibration()).unwrap();
    let mut source = GroupSurface::transparent(64, 32).unwrap();
    for y in 0..32 {
        for x in 0..64 {
            let i = (y * 64 + x) * 4;
            source.rgba[i..i + 4].copy_from_slice(if x < 32 {
                &[255, 0, 0, 255]
            } else {
                &[0, 0, 255, 255]
            });
        }
    }
    for (pan, expected) in [(0., [255, 0, 0, 255]), (180., [0, 0, 255, 255])] {
        let mut frame = source.clone();
        let p = SphericalVideo {
            input: Input::DualFisheye,
            camera_profile: Some(profile.clone()),
            pan,
            ..Default::default()
        };
        flat_surface::apply_effect_stack(
            &mut frame,
            &[VideoEffect::SphericalVideo {
                parameters: p,
                animation: None,
            }],
            0,
            &[],
            &mut || Ok(()),
        )
        .unwrap();
        assert_eq!(
            &frame.rgba[(16 * 64 + 32) * 4..(16 * 64 + 32) * 4 + 4],
            &expected
        );
    }
    let mut invalid = SphericalVideo {
        camera_profile: Some(profile),
        ..Default::default()
    };
    assert!(invalid.validate().is_err());
    invalid.input = Input::DualFisheye;
    assert!(invalid.validate().is_ok());
}
#[test]
#[ignore = "requires an explicitly supplied read-only local media inventory"]
fn v6_profiles_local_footage() {
    let path = std::env::var("VIDEO_KADR_INSTA360_INVENTORY").unwrap();
    let rows: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut count = 0;
    for row in rows["files"].as_array().unwrap() {
        if row["bytes"].as_u64().unwrap() == 0 {
            continue;
        }
        let mut file = std::fs::File::open(row["path"].as_str().unwrap()).unwrap();
        let Some(m) = insta360_metadata::inspect(&mut file).unwrap() else {
            continue;
        };
        let report = inspect(&m);
        assert_eq!(report.status, "available", "{}", report.reason);
        let p = report.profile.unwrap();
        p.validate().unwrap();
        assert_eq!(p.reference_size[0], 2 * p.reference_size[1]);
        count += 1;
    }
    assert!(count > 0);
    println!("parsed and validated {count} source-owned v6 camera profiles");
}
#[test]
#[ignore = "requires explicitly supplied local footage inventory and ffmpeg"]
fn v6_real_dual_track_panorama() {
    use video_kadr_backend::domain::{
        composition::VideoEffect,
        flat_surface, group_media,
        spherical_video::{Input, Output, SphericalVideo},
        video_clock::FrameTime,
    };
    let path = std::env::var("VIDEO_KADR_INSTA360_INVENTORY").unwrap();
    let rows: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut files: Vec<_> = rows["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            r["bytes"].as_u64().unwrap()
                > std::env::var("VIDEO_KADR_INSTA360_MIN_BYTES")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0)
        })
        .collect();
    files.sort_by_key(|r| r["bytes"].as_u64().unwrap());
    let (input, profile, metadata) = files
        .iter()
        .find_map(|r| {
            let input = std::path::Path::new(r["path"].as_str().unwrap());
            let m = insta360_metadata::inspect(&mut std::fs::File::open(input).unwrap())
                .ok()
                .flatten()?;
            Some((input, inspect(&m).profile?, m))
        })
        .expect("one qualified camera source");
    let ffmpeg = std::env::var("VIDEO_KADR_FFMPEG").unwrap_or("/opt/homebrew/bin/ffmpeg".into());
    let mut frame = group_media::decode_source_frame_policy_layout(
        std::path::Path::new(&ffmpeg),
        input,
        FrameTime::Microseconds(0),
        1024,
        512,
        std::time::Duration::from_secs(120),
        &std::sync::atomic::AtomicBool::new(false),
        false,
        true,
    )
    .unwrap();
    let folder = std::env::var_os("VIDEO_KADR_INSTA360_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("video-kadr-insta360-camera-profile"));
    std::fs::create_dir_all(&folder).unwrap();
    image::save_buffer(
        folder.join("real-packed.png"),
        &frame.rgba,
        1024,
        512,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let packed = frame.clone();
    let parameters = SphericalVideo {
        input: Input::DualFisheye,
        output: Output::Equirectangular,
        camera_profile: Some(profile.clone()),
        ..Default::default()
    };
    flat_surface::apply_effect_stack(
        &mut frame,
        &[VideoEffect::SphericalVideo {
            parameters,
            animation: None,
        }],
        0,
        &[],
        &mut || Ok(()),
    )
    .unwrap();
    image::save_buffer(
        folder.join("real-panorama.png"),
        &frame.rgba,
        1024,
        512,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let pixels = frame.rgba.chunks_exact(4);
    let visible = pixels.clone().filter(|p| p[3] > 250).count();
    let nonblack = pixels.filter(|p| p[..3].iter().any(|v| *v > 8)).count();
    println!("real dual-track frame: {visible}/524288 opaque; {nonblack}/524288 nonblack pixels");
    assert!(
        visible > 524288 * 95 / 100,
        "insufficient calibrated angular coverage"
    );
    assert!(nonblack > 524288 / 4, "unexpected black panorama");
    use video_kadr_backend::domain::{insta360_gyro, spherical_video::StabilizationMode};
    let video_clock = video_kadr_backend::domain::insta360_video_clock::inspect(
        &mut std::fs::File::open(input).unwrap(),
    )
    .unwrap();
    println!("qualified MP4 video clock={video_clock:?}");
    let gyro = insta360_gyro::analyze_with_metadata(
        &mut std::fs::File::open(input).unwrap(),
        &metadata,
        &insta360_gyro::GyroRequest {
            duration_seconds: 0.25,
            ..Default::default()
        },
    )
    .unwrap();
    println!(
        "synchronization={}; first orientation={:?}; first sensor acceleration={:?}",
        gyro.synchronization,
        gyro.orientation.first().map(|v| v.quaternion),
        gyro.samples[0].acceleration_g
    );
    assert_eq!(gyro.status, "orientation_ready");
    assert_eq!(gyro.mounting_source, "embedded_v6_145");
    assert_eq!(gyro.synchronization, "exposure_midpoint_linear");
    // Independent source-frame invariant: native v6 IMU gravity-down becomes
    // editor specific-force-up [sensorZ,-sensorX,sensorY]. Its transformed
    // direction must agree with world up; the rejected Rz(pi) bridge fails here.
    let a = gyro.samples[0].acceleration_g;
    let force = [a[2], -a[0], a[1]];
    let norm = force.iter().map(|v| v * v).sum::<f64>().sqrt();
    let [w, x, y, z] = gyro.orientation[0].quaternion;
    let world_up_dot = (2. * (x * y + w * z) * force[0]
        + (1. - 2. * (x * x + z * z)) * force[1]
        + 2. * (y * z - w * x) * force[2])
        / norm;
    println!("source specific-force/world-up agreement={world_up_dot}");
    assert!(
        world_up_dot > 0.999,
        "embedded camera mounting does not align source gravity"
    );
    for (orientation, name) in [
        (vec![], "real-raw-perspective.png"),
        (gyro.orientation.clone(), "real-stabilized-perspective.png"),
    ] {
        let mut perspective = packed.clone();
        let parameters = SphericalVideo {
            input: Input::DualFisheye,
            output: Output::Perspective,
            pan: 160.,
            fov: 110.,
            camera_profile: Some(profile.clone()),
            orientation,
            stabilization_mode: StabilizationMode::Horizon,
            ..Default::default()
        };
        flat_surface::apply_effect_stack(
            &mut perspective,
            &[VideoEffect::SphericalVideo {
                parameters,
                animation: None,
            }],
            0,
            &[],
            &mut || Ok(()),
        )
        .unwrap();
        image::save_buffer(
            folder.join(name),
            &perspective.rgba,
            1024,
            512,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    let mut stabilized = packed;
    let parameters = SphericalVideo {
        input: Input::DualFisheye,
        output: Output::Equirectangular,
        camera_profile: Some(profile),
        orientation: gyro.orientation,
        stabilization_mode: StabilizationMode::Horizon,
        ..Default::default()
    };
    flat_surface::apply_effect_stack(
        &mut stabilized,
        &[VideoEffect::SphericalVideo {
            parameters,
            animation: None,
        }],
        0,
        &[],
        &mut || Ok(()),
    )
    .unwrap();
    image::save_buffer(
        folder.join("real-stabilized.png"),
        &stabilized.rgba,
        1024,
        512,
        image::ColorType::Rgba8,
    )
    .unwrap();
    assert!(
        stabilized
            .rgba
            .chunks_exact(4)
            .filter(|p| p[3] > 250)
            .count()
            > 524288 * 95 / 100
    );
    println!("embedded gyro auto mounting, capture synchronization and horizon render passed");
}
