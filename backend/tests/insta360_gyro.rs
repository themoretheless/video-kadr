#[path = "support/insta360_mp4.rs"]
mod mp4;
use std::io::Cursor;
use video_kadr_backend::domain::insta360_gyro::{analyze, fuse, GyroRequest, SensorSample};
fn vi(mut v: u64) -> Vec<u8> {
    let mut out = vec![];
    loop {
        let b = (v & 127) as u8;
        v >>= 7;
        out.push(if v > 0 { b | 128 } else { b });
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
fn bytes(n: u64, v: &[u8], out: &mut Vec<u8>) {
    out.extend(vi(n << 3 | 2));
    out.extend(vi(v.len() as u64));
    out.extend(v);
}
fn fixture() -> Vec<u8> {
    fixture_mode(false)
}
fn fixture_mode(exposure: bool) -> Vec<u8> {
    let mut metadata = vec![];
    bytes(2, b"Insta360 X6", &mut metadata);
    field(24, 1_000_000, &mut metadata);
    field(51, 1, &mut metadata);
    field(62, 1, &mut metadata);
    let mut ranges = vec![];
    field(1, 32, &mut ranges);
    field(2, 2000, &mut ranges);
    bytes(65, &ranges, &mut metadata);
    if exposure {
        field(64, 2, &mut metadata);
        field(20, 50, &mut metadata);
    }
    let mut gyro = vec![];
    for i in 0..=2200 {
        gyro.extend((900_000u64 + i * 1000).to_le_bytes());
        for word in [
            32768u16,
            33792,
            32768,
            32768,
            33505 + if exposure { i as u16 } else { 0 },
            32768,
        ] {
            gyro.extend(word.to_le_bytes());
        }
    }
    let mut data = if exposure {
        mp4::movie(50000, &[(106, 1000)], None, None)
    } else {
        vec![0; 32]
    };
    let base = data.len();
    let rows_len = if exposure { 50 } else { 40 };
    let mut rows = vec![0; rows_len];
    let mut blocks = vec![(1u8, 1u8, metadata), (3, 0, gyro)];
    if exposure {
        let mut records = vec![];
        for i in 0..=105 {
            records.extend((1_000_000u64 + i * 20_000).to_le_bytes());
            records.extend(0.01f64.to_le_bytes());
        }
        blocks.push((4, 0, records));
    }
    for (kind, format, payload) in blocks {
        let offset = data.len() - base;
        let n = payload.len() as u32;
        data.extend(payload);
        data.extend([format, kind]);
        data.extend(n.to_le_bytes());
        let at = kind as usize * 10;
        rows[at] = kind;
        rows[at + 1] = format;
        rows[at + 2..at + 6].copy_from_slice(&n.to_le_bytes());
        rows[at + 6..at + 10].copy_from_slice(&(offset as u32).to_le_bytes());
    }
    data.extend(rows);
    data.extend([0, 0]);
    data.extend((rows_len as u32).to_le_bytes());
    let extent = (data.len() - base + 72) as u32;
    data.extend([0; 32]);
    data.extend(extent.to_le_bytes());
    data.extend(3u32.to_le_bytes());
    data.extend(b"8db42d694ccc418790edff439fe026bf");
    data
}
#[test]
fn raw_offset_binary_capture_clock_and_explicit_sensor_axes() {
    let mut request = GyroRequest {
        duration_seconds: 1.,
        sample_rate_hz: 10.,
        ..Default::default()
    };
    let report = analyze(&mut Cursor::new(fixture()), &request)
        .unwrap()
        .unwrap();
    assert_eq!(report.status, "sensor_axes_required");
    assert!(report.orientation.is_empty());
    assert_eq!(report.samples.len(), 11);
    assert_eq!(report.samples[0].time_seconds, 0.);
    assert_eq!(report.samples[10].time_seconds, 1.);
    assert!((report.samples[0].acceleration_g[1] - 1.).abs() < 1e-12);
    let expected = 737. * 2000. / 32768. * std::f64::consts::PI / 180.;
    assert!((report.samples[0].radians_per_second[1] - expected).abs() < 1e-12);
    request.camera_from_sensor = Some([1., 0., 0., 0.]);
    let report = analyze(&mut Cursor::new(fixture()), &request)
        .unwrap()
        .unwrap();
    assert_eq!(report.status, "orientation_ready");
    let q = report.orientation.last().unwrap().quaternion;
    assert!((q[0] - (expected / 2.).cos()).abs() < 1e-9);
    assert!((q[2] - (expected / 2.).sin()).abs() < 1e-9);
    assert_eq!(report.orientation[0].quaternion, [1., 0., 0., 0.]);
    request.time_offset_seconds = 0.2;
    let shifted = analyze(&mut Cursor::new(fixture()), &request)
        .unwrap()
        .unwrap();
    assert!((shifted.capture_start_seconds + 0.1).abs() < 1e-10);
}
#[test]
fn gyro_window_rejects_bounds_nonunit_and_time_discontinuity() {
    let mut r = GyroRequest {
        duration_seconds: 1.,
        ..Default::default()
    };
    r.camera_from_sensor = Some([2., 0., 0., 0.]);
    assert!(analyze(&mut Cursor::new(fixture()), &r).is_err());
    r.camera_from_sensor = None;
    r.duration_seconds = 100.;
    assert!(analyze(&mut Cursor::new(fixture()), &r).is_err());
    r.duration_seconds = 1.;
    r.start_seconds = 5.;
    assert!(analyze(&mut Cursor::new(fixture()), &r).is_err());
    let samples = [
        SensorSample {
            time_seconds: 0.,
            acceleration_g: [0., 1., 0.],
            radians_per_second: [0.; 3],
        },
        SensorSample {
            time_seconds: 0.2,
            acceleration_g: [0., 1., 0.],
            radians_per_second: [0.; 3],
        },
    ];
    assert!(fuse(&samples, [1., 0., 0., 0.], true).is_err());
    r.start_seconds = 0.;
    let mut data = fixture();
    let first_gyro = 32 + 31; // Find raw first timestamp without depending on metadata field varint lengths.
    let needle = 900_000u64.to_le_bytes();
    let at = data.windows(8).position(|w| w == needle).unwrap();
    assert!(at >= first_gyro);
    data[at + 20..at + 28].copy_from_slice(&899_000u64.to_le_bytes()); // Outside requested range; damage a consumed sample as well.
    data[at + 501 * 20..at + 501 * 20 + 8].copy_from_slice(&1_300_000u64.to_le_bytes());
    assert!(analyze(&mut Cursor::new(data), &r).is_err());
}
#[test]
fn automatic_gravity_is_bounded_but_declared_stationary_bias_is_correctable() {
    let samples: Vec<_> = (0..=1000)
        .map(|i| SensorSample {
            time_seconds: i as f64 / 100.,
            acceleration_g: [0., 1., 0.],
            radians_per_second: [0.03, 0., 0.],
        })
        .collect();
    let plain = fuse(&samples, [1., 0., 0., 0.], false).unwrap();
    let corrected = fuse(&samples, [1., 0., 0., 0.], true).unwrap();
    assert!(plain.last().unwrap().quaternion[1].abs() > 0.14);
    // Unobserved large bias is not permission to invent unlimited gravity tilt.
    let separation=2.*plain.last().unwrap().quaternion.iter().zip(corrected.last().unwrap().quaternion).map(|(a,b)|a*b).sum::<f64>().abs().clamp(-1.,1.).acos();
    assert!(separation<=0.251f64.to_radians());
    let mut small=samples[..201].to_vec();
    for sample in &mut small {sample.radians_per_second=[0.0005,0.,0.];}
    let raw=fuse(&small,[1.,0.,0.,0.],false).unwrap();
    let bounded=fuse(&small,[1.,0.,0.,0.],true).unwrap();
    assert!(bounded.last().unwrap().quaternion[1].abs()<raw.last().unwrap().quaternion[1].abs()*0.5);
    // The previous 0.03rad/s / 10s expectation requires an actual stationary
    // declaration; gravity alone cannot distinguish this rate from motion.
    use video_kadr_backend::domain::gyro_drift::{stationary_bias,StationaryInterval};
    let (bias,count)=stationary_bias(&samples,&[StationaryInterval{start_seconds:0.,end_seconds:1.}]).unwrap();
    assert!(count>=100);assert!((bias[0]-0.03).abs()<1e-12);
    let calibrated:Vec<_>=samples.iter().map(|sample|SensorSample{radians_per_second:std::array::from_fn(|i|sample.radians_per_second[i]-bias[i]),..*sample}).collect();
    let declared=fuse(&calibrated,[1.,0.,0.,0.],true).unwrap();
    assert!(declared.last().unwrap().quaternion[1].abs()<1e-12);
    let mut dynamic = samples.clone();
    for s in &mut dynamic[1..] {
        s.acceleration_g = [2., 0., 0.];
    }
    let dynamic = fuse(&dynamic, [1., 0., 0., 0.], true).unwrap();
    for i in 0..4 {
        assert!(
            (dynamic.last().unwrap().quaternion[i] - plain.last().unwrap().quaternion[i]).abs()
                < 1e-12
        );
    }
    // +90 degrees around Z maps sensor X angular velocity onto camera Y.
    let yaw_samples: Vec<_> = samples
        .iter()
        .map(|s| SensorSample {
            acceleration_g: [1., 0., 0.],
            ..*s
        })
        .collect();
    let mapped = fuse(&yaw_samples, [0.5f64.sqrt(), 0., 0., 0.5f64.sqrt()], false).unwrap();
    assert!(mapped.last().unwrap().quaternion[1].abs() < 1e-12);
    assert!(mapped.last().unwrap().quaternion[2] > 0.14);
}
#[test]
#[ignore = "requires explicitly authorized local media inventory"]
fn read_real_x6_capture_gyro() {
    let path = std::env::var("VIDEO_KADR_INSTA360_INVENTORY").unwrap();
    let inventory: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let row = inventory["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["bytes"].as_u64().unwrap() > 0)
        .min_by_key(|r| r["bytes"].as_u64().unwrap())
        .unwrap();
    let mut file = std::fs::File::open(row["path"].as_str().unwrap()).unwrap();
    let r = GyroRequest {
        duration_seconds: 0.5,
        ..Default::default()
    };
    let report = analyze(&mut file, &r).unwrap().unwrap();
    assert_eq!(report.samples.len(), 31);
    assert_eq!(report.status, "orientation_ready");
    assert_eq!(report.mounting_source, "embedded_v6_145");
    assert_eq!(report.synchronization, "exposure_midpoint_linear");
    assert!(report.input_samples > 400);
    assert_eq!(report.orientation.len(), 31);
    println!("real X6 read-only raw IMU: {} integrated-range records, {} output samples, capture coverage {:.6}..{:.6}s",report.input_samples,report.samples.len(),report.capture_start_seconds,report.capture_end_seconds);
}

#[test]
fn fusion_initial_tilt_aligns_specific_force_to_world_up() {
    let acceleration = [0., 0.5f64.sqrt(), -0.5f64.sqrt()];
    let samples = [
        SensorSample {
            time_seconds: 0.,
            acceleration_g: acceleration,
            radians_per_second: [0.; 3],
        },
        SensorSample {
            time_seconds: 0.01,
            acceleration_g: acceleration,
            radians_per_second: [0.; 3],
        },
    ];
    let result = fuse(&samples, [1., 0., 0., 0.], true).unwrap();
    let q = result[0].quaternion;
    assert!((q[0] - (std::f64::consts::PI / 8.).cos()).abs() < 1e-12);
    assert!((q[1] - (std::f64::consts::PI / 8.).sin()).abs() < 1e-12);
    assert!((result[1].quaternion[1] - q[1]).abs() < 1e-12);
    let upside_down = [
        SensorSample {
            acceleration_g: [0., -1., 0.],
            ..samples[0]
        },
        SensorSample {
            acceleration_g: [0., -1., 0.],
            ..samples[1]
        },
    ];
    assert_eq!(
        fuse(&upside_down, [1., 0., 0., 0.], true).unwrap()[0].quaternion,
        [0., 1., 0., 0.]
    );
}

#[test]
fn embedded_exposure_midpoint_clock_samples_earlier_sensor_time() {
    let request = GyroRequest {
        duration_seconds: 1.,
        sample_rate_hz: 50.,
        ..Default::default()
    };
    let result = analyze(&mut Cursor::new(fixture_mode(true)), &request)
        .unwrap()
        .unwrap();
    assert_eq!(result.synchronization, "exposure_midpoint_linear");
    let factor = 2000. / 32768. * std::f64::consts::PI / 180.;
    // First frame capture=1s, exposure=10ms => gyro at995ms, raw index95.
    assert!((result.samples[0].radians_per_second[1] - (737. + 95.) * factor).abs() < 1e-9);
    assert!((result.samples[50].radians_per_second[1] - (737. + 1095.) * factor).abs() < 1e-9);
}

fn synthetic_v6_calibration() -> String {
    let mut fields = vec!["2".to_string()];
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
        lens.extend(std::iter::repeat_n("0".to_string(), 16));
        lens.extend(["15360".into(), "7680".into(), "193".into()]);
        fields.extend(lens);
    }
    fields.push("394240".into());
    fields.join("_")
}
#[test]
fn matching_embedded_calibration_automatically_maps_sensor_axes() {
    use video_kadr_backend::domain::{
        insta360_gyro::analyze_with_metadata,
        insta360_metadata::{inspect, Calibration},
    };
    let mut file = Cursor::new(fixture());
    let mut metadata = inspect(&mut file).unwrap().unwrap();
    metadata.calibrations.push(Calibration {
        field_number: 111,
        value: synthetic_v6_calibration(),
    });
    let mut request = GyroRequest {
        duration_seconds: 1.,
        accelerometer_correction: false,
        ..Default::default()
    };
    let automatic = analyze_with_metadata(&mut file, &metadata, &request).unwrap();
    assert_eq!(automatic.mounting_source, "embedded_v6_145");
    assert_eq!(automatic.status, "orientation_ready");
    assert!(automatic.orientation.last().unwrap().quaternion[3] < -0.3);
    assert!(automatic.orientation.last().unwrap().quaternion[1].abs() < 1e-12);
    assert!(automatic.orientation.last().unwrap().quaternion[2].abs() < 1e-12);
    request.camera_from_sensor = Some([1., 0., 0., 0.]);
    let explicit = analyze_with_metadata(&mut file, &metadata, &request).unwrap();
    assert_eq!(explicit.mounting_source, "explicit");
    assert!(explicit.orientation.last().unwrap().quaternion[2] > 0.3);
    request.camera_from_sensor = None;
    metadata.calibrations[0].value = synthetic_v6_calibration().replace("394240", "393472");
    let unresolved = analyze_with_metadata(&mut file, &metadata, &request).unwrap();
    assert_eq!(unresolved.mounting_source, "unresolved");
    assert!(unresolved.orientation.is_empty());
}

#[test]
fn uncertain_video_pts_falls_back_to_explicitly_warned_capture_clock() {
    let mut file = fixture_mode(true);
    let needle = b"stts";
    let index = file.windows(4).position(|x| x == needle).unwrap();
    // A nonzero media version/flag is outside the qualified timing path.
    file[index + 4] = 1;
    let report = analyze(
        &mut Cursor::new(file),
        &GyroRequest {
            duration_seconds: 0.5,
            ..Default::default()
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(report.synchronization, "first_frame_capture_clock");
    assert!(report
        .timing_warning
        .unwrap()
        .contains("Capture-clock alignment is approximate"));
}

#[test]
fn enormous_finite_window_is_rejected_without_index_overflow() {
    let request = GyroRequest {
        start_seconds: f64::MAX,
        duration_seconds: 1.,
        ..Default::default()
    };
    assert!(analyze(&mut Cursor::new(fixture_mode(true)), &request).is_err());
}

#[test]
fn embedded_gravity_polarity_aligns_physical_up_without_changing_manual_contract() {
    use video_kadr_backend::domain::{
        insta360_gyro::analyze_with_metadata,
        insta360_metadata::{inspect, Calibration},
    };
    for (acceleration, expected) in [
        ([31744u16, 32768, 32768], [1., 0., 0., 0.]),
        ([33792, 32768, 32768], [0., 1., 0., 0.]),
        (
            [32768, 33792, 32768],
            [0.5f64.sqrt(), -0.5f64.sqrt(), 0., 0.],
        ),
    ] {
        let mut bytes = fixture();
        let first = bytes
            .windows(8)
            .position(|w| w == 900_000u64.to_le_bytes())
            .unwrap();
        for i in 0..=2200 {
            let offset = first + i * 20 + 8;
            for axis in 0..3 {
                bytes[offset + axis * 2..offset + axis * 2 + 2]
                    .copy_from_slice(&acceleration[axis].to_le_bytes());
                bytes[offset + 6 + axis * 2..offset + 8 + axis * 2]
                    .copy_from_slice(&32768u16.to_le_bytes());
            }
        }
        let mut file = Cursor::new(bytes);
        let mut metadata = inspect(&mut file).unwrap().unwrap();
        metadata.calibrations.push(Calibration {
            field_number: 111,
            value: synthetic_v6_calibration(),
        });
        let request = GyroRequest {
            duration_seconds: 0.5,
            ..Default::default()
        };
        let automatic = analyze_with_metadata(&mut file, &metadata, &request).unwrap();
        for (a, b) in automatic.orientation[0]
            .quaternion
            .into_iter()
            .zip(expected)
        {
            assert!((a - b).abs() < 1e-12, "gravity case {acceleration:?}");
        }
        assert_eq!(automatic.mounting_source, "embedded_v6_145");
    }
}

#[test]
fn native_longitude_origin_and_axial_polar_basis_are_not_interchanged() {
    let native_ray = |u: f64, v: f64| {
        let lon = std::f64::consts::TAU * (1. - u);
        let lat = std::f64::consts::FRAC_PI_2 - std::f64::consts::PI * v;
        [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()]
    };
    for (u, v, expected) in [
        (0.5, 0.5, [-1., 0., 0.]),
        (0.75, 0.5, [0., 1., 0.]),
        (0.5, 0., [0., 0., 1.]),
    ] {
        for (a, b) in native_ray(u, v).into_iter().zip(expected) {
            assert!((a - b).abs() < 1e-12);
        }
    }
    // B_lambda(editor)=[-Z,+X,Y], determinant -1. Gyro is axial, while
    // specific force uses the recovered native gravity-down→up convention.
    let raw = [0.2, 0.3, 0.4];
    let native_g = [-raw[1], raw[2], -raw[0]];
    let editor_force = [native_g[1], native_g[2], -native_g[0]];
    let editor_rate = editor_force.map(|v| -v);
    assert_eq!(editor_rate, [-raw[2], raw[0], -raw[1]]);
    assert_eq!(editor_force, [raw[2], -raw[0], raw[1]]);
    // Proper quaternion [.5,-.5,-.5,.5] rotates raw gyro to [-Z,X,-Y].
    let samples = [
        SensorSample {
            time_seconds: 0.,
            acceleration_g: [0., 1., 0.],
            radians_per_second: raw,
        },
        SensorSample {
            time_seconds: 0.01,
            acceleration_g: [0., 1., 0.],
            radians_per_second: raw,
        },
    ];
    let pose = fuse(&samples, [0.5, -0.5, -0.5, 0.5], false).unwrap()[1].quaternion;
    let magnitude = raw.iter().map(|v| v * v).sum::<f64>().sqrt();
    let k = (magnitude * 0.005).sin() / magnitude;
    for i in 0..3 {
        assert!((pose[i + 1] - editor_rate[i] * k).abs() < 1e-12);
    }
}

#[test]
fn explicit_and_declared_bias_correct_orientation_but_preserve_raw_sensor_report() {
    use video_kadr_backend::domain::gyro_drift::{DriftCorrection, StationaryInterval};
    let mut data = fixture();
    let metadata = video_kadr_backend::domain::insta360_metadata::inspect(&mut Cursor::new(&data))
        .unwrap()
        .unwrap();
    let block = metadata.blocks.iter().find(|b| b.kind == 3).unwrap();
    for record in data[block.offset as usize..block.offset as usize + block.length as usize]
        .chunks_exact_mut(20)
    {
        record[16..18].copy_from_slice(&32780u16.to_le_bytes());
    }
    let bias = 12. / 32768. * 2000. * std::f64::consts::PI / 180.;
    let request = GyroRequest {
        duration_seconds: 1.,
        camera_from_sensor: Some([1., 0., 0., 0.]),
        orientation_tolerance_degrees: Some(0.005),
        motion_padding_seconds: 0.03,
        ..Default::default()
    };
    let original = analyze(&mut Cursor::new(&data), &request).unwrap().unwrap();
    assert!(original.orientation.last().unwrap().quaternion[2] > 0.005);
    let corrected = analyze(
        &mut Cursor::new(&data),
        &GyroRequest {
            drift_correction: DriftCorrection {
                bias_radians_per_second: Some([0., bias, 0.]),
                ..Default::default()
            },
            ..request.clone()
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(corrected.drift_correction.source, "explicit_bias");
    assert_eq!(corrected.orientation_offset_seconds, 0.03);
    assert!((corrected.samples[0].radians_per_second[1] - bias).abs() < 1e-14);
    assert!(corrected
        .orientation
        .iter()
        .all(|q| q.quaternion[2].abs() < 1e-12));
    let declared = analyze(
        &mut Cursor::new(&data),
        &GyroRequest {
            drift_correction: DriftCorrection {
                stationary_intervals: vec![StationaryInterval {
                    start_seconds: 0.,
                    end_seconds: 1.,
                }],
                ..Default::default()
            },
            ..request
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(declared.drift_correction.source, "declared_stationary");
    assert!(declared.drift_correction.calibration_samples > 900);
    assert!(declared
        .orientation
        .iter()
        .all(|q| q.quaternion[2].abs() < 1e-12));
}
#[test]
fn padded_relative_track_preserves_clip_origin_and_observed_heading_is_not_erased() {
    use video_kadr_backend::domain::{
        gyro_drift::{self, DriftCorrection, HeadingAnchor},
        orientation_track::angular_error_degrees,
    };
    let data = fixture();
    let request = GyroRequest {
        duration_seconds: 1.,
        camera_from_sensor: Some([1., 0., 0., 0.]),
        accelerometer_correction: false,
        orientation_tolerance_degrees: Some(0.005),
        motion_padding_seconds: 0.03,
        ..Default::default()
    };
    let result = analyze(&mut Cursor::new(&data), &request).unwrap().unwrap();
    let clip_start = gyro_drift::orientation_at(&result.orientation, 0.03).unwrap();
    assert!(angular_error_degrees(clip_start, [1., 0., 0., 0.]) < 1e-5);
    assert!(angular_error_degrees(result.orientation[0].quaternion, [1., 0., 0., 0.]) > 1.);
    let observed = analyze(
        &mut Cursor::new(&data),
        &GyroRequest {
            drift_correction: DriftCorrection {
                heading_anchors: vec![
                    HeadingAnchor {
                        time_seconds: 0.,
                        heading_degrees: 30.,
                    },
                    HeadingAnchor {
                        time_seconds: 1.,
                        heading_degrees: 30.,
                    },
                ],
                ..Default::default()
            },
            ..request
        },
    )
    .unwrap()
    .unwrap();
    let pose = gyro_drift::orientation_at(&observed.orientation, 0.03).unwrap();
    assert!(
        angular_error_degrees(
            pose,
            [
                (15f64.to_radians()).cos(),
                0.,
                (15f64.to_radians()).sin(),
                0.
            ]
        ) < 0.01
    );
}
#[test]
fn full_presentation_endpoint_keeps_midpoint_clock_with_bounded_last_period_extension() {
    let mut data = fixture_mode(true);
    let metadata = video_kadr_backend::domain::insta360_metadata::inspect(&mut Cursor::new(&data))
        .unwrap()
        .unwrap();
    let gyro = metadata.blocks.iter().find(|b| b.kind == 3).unwrap();
    // Existing fixture ends IMU capture before the video presentation endpoint;
    // extend the capture timestamps very slightly without changing record count.
    for (index, record) in data[gyro.offset as usize..gyro.offset as usize + gyro.length as usize]
        .chunks_exact_mut(20)
        .enumerate()
    {
        record[..8].copy_from_slice(&(890_000u64 + index as u64 * 1_100).to_le_bytes());
    }
    let result = analyze(
        &mut Cursor::new(&data),
        &GyroRequest {
            duration_seconds: 106. / 50.,
            camera_from_sensor: Some([1., 0., 0., 0.]),
            ..Default::default()
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(result.synchronization, "exposure_midpoint_linear");
    assert!(result
        .timing_warning
        .as_deref()
        .unwrap()
        .contains("endpoint"));
}

#[test]
fn precise_track_keeps_real_exposure_breakpoint_between_uniform_output_samples() {
    use video_kadr_backend::domain::{gyro_drift, orientation_track::angular_error_degrees};
    let mut data = fixture_mode(true);
    let meta = video_kadr_backend::domain::insta360_metadata::inspect(&mut Cursor::new(&data))
        .unwrap()
        .unwrap();
    let block = meta.blocks.iter().find(|b| b.kind == 4).unwrap();
    // Frame51 is at1.02sec, which is not a7Hz output-grid point. Its shutter
    // change must survive in both the physical clock and native-rate motion.
    let at = block.offset as usize + 51 * 16 + 8;
    data[at..at + 8].copy_from_slice(&0.015f64.to_le_bytes());
    let result = analyze(
        &mut Cursor::new(&data),
        &GyroRequest {
            duration_seconds: 2.,
            sample_rate_hz: 7.,
            camera_from_sensor: Some([1., 0., 0., 0.]),
            accelerometer_correction: false,
            orientation_tolerance_degrees: Some(0.005),
            motion_padding_seconds: 0.03,
            ..Default::default()
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(result.synchronization, "exposure_midpoint_linear");
    let event = result
        .capture_clock
        .iter()
        .find(|p| (p.time_seconds - 1.05).abs() < 1e-10)
        .expect("real exposure breakpoint was lost");
    assert!((event.capture_seconds - 1.0125).abs() < 1e-12);
    let actual = gyro_drift::orientation_at(&result.orientation, 1.05).unwrap();
    let (begin, end) = (-0.005f64, 1.0125f64);
    let angle = (2000. / 32768. * std::f64::consts::PI / 180.)
        * (837. * (end - begin) + 500. * (end * end - begin * begin));
    let expected = [(angle * 0.5).cos(), 0., (angle * 0.5).sin(), 0.];
    assert!(angular_error_degrees(actual, expected) < 0.006);
}

#[path = "support/insta360_time_map.rs"]
mod trim_fixture;
fn mapped_fixture() -> (
    Vec<u8>,
    video_kadr_backend::domain::insta360_metadata::Metadata,
) {
    use video_kadr_backend::domain::insta360_metadata::{inspect, Block};
    let mut data = fixture_mode(true);
    let at = data.windows(4).position(|w| w == b"stts").unwrap();
    // Source exposure buffer retains106frames; decoded clip contains60 at50fps.
    data[at + 12..at + 16].copy_from_slice(&60u32.to_be_bytes());
    let mut metadata = inspect(&mut Cursor::new(&data)).unwrap().unwrap();
    let payload = trim_fixture::trim(60, 20., 200.);
    let offset = data.len() as u64;
    let length = payload.len() as u32;
    data.extend(payload);
    metadata.blocks.push(Block {
        kind: 128,
        format: 0,
        length,
        offset,
    });
    (data, metadata)
}
#[test]
fn retained_trim_map_shifts_original_exposure_but_preserves_decoded_source_frame_phase() {
    use video_kadr_backend::domain::insta360_gyro::analyze_with_metadata;
    let (data, metadata) = mapped_fixture();
    let r = analyze_with_metadata(
        &mut Cursor::new(data),
        &metadata,
        &GyroRequest {
            duration_seconds: 1.2,
            motion_padding_seconds: 0.03,
            orientation_tolerance_degrees: Some(0.005),
            camera_from_sensor: Some([1., 0., 0., 0.]),
            accelerometer_correction: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.synchronization, "exposure_midpoint_trim_map");
    assert_eq!(r.original_time_map_offset_seconds, Some(0.2));
    assert_eq!(r.orientation_offset_seconds, 0.03);
    let clock = r.source_frame_clock.unwrap();
    assert_eq!(
        (
            clock.numerator,
            clock.denominator,
            clock.source_start_seconds
        ),
        (50, 1, 0.)
    );
    let capture = |t: f64| {
        let index = r.capture_clock.partition_point(|p| p.time_seconds < t);
        let (a, b) = if index == 0 {
            (r.capture_clock[0], r.capture_clock[1])
        } else if index >= r.capture_clock.len() {
            (r.capture_clock[index - 2], r.capture_clock[index - 1])
        } else {
            (r.capture_clock[index - 1], r.capture_clock[index])
        };
        a.capture_seconds
            + (b.capture_seconds - a.capture_seconds) * (t - a.time_seconds)
                / (b.time_seconds - a.time_seconds)
    };
    assert!((capture(0.03) - 0.195).abs() < 1e-12);
    assert!((capture(1.23) - 1.395).abs() < 1e-12);
    assert!((r.capture_clock[0].capture_seconds - 0.165).abs() < 1e-12);
    assert!((r.capture_clock.last().unwrap().capture_seconds - 1.425).abs() < 1e-12);
    assert!(r.timing_warning.unwrap().contains("endpoint"));
}
#[test]
fn malformed_or_unqualified_retained_map_cannot_bypass_timing_with_manual_mount() {
    use video_kadr_backend::domain::insta360_gyro::analyze_with_metadata;
    let (data, metadata) = mapped_fixture();
    let request = GyroRequest {
        duration_seconds: 0.5,
        camera_from_sensor: Some([1., 0., 0., 0.]),
        ..Default::default()
    };
    let offset = metadata
        .blocks
        .iter()
        .find(|b| b.kind == 128)
        .unwrap()
        .offset as usize;
    let mut bad = data.clone();
    bad[offset + 4..offset + 8].copy_from_slice(&2u32.to_le_bytes());
    assert!(analyze_with_metadata(&mut Cursor::new(bad), &metadata, &request).is_err());
    let mut missing = metadata.clone();
    missing.pts_type = None;
    assert!(analyze_with_metadata(&mut Cursor::new(data.clone()), &missing, &request).is_err());
    let mut bad = data;
    let at = bad.windows(4).position(|w| w == b"stts").unwrap();
    bad[at + 4] = 1;
    assert!(analyze_with_metadata(&mut Cursor::new(bad), &metadata, &request).is_err());
}
#[test]
fn x4_auto_mount_requires_qualified_legacy_profile_not_hardware_flag_alone() {
    use video_kadr_backend::domain::{
        insta360_gyro::analyze_with_metadata,
        insta360_metadata::{inspect, Calibration, WindowCropInfo},
    };
    let data = fixture_mode(true);
    let mut m = inspect(&mut Cursor::new(&data)).unwrap().unwrap();
    m.camera_model = Some("Insta360 X4".into());
    m.calibrations = vec![Calibration {
        field_number: 5,
        value: "2_3000_4000_3000_0_0_90_3000_12000_3000_0_0_90_16000_6000_1095".into(),
    }];
    m.window_crop_info = Some(WindowCropInfo {
        source_size: [8000, 6000],
        destination_size: [5760, 5760],
        offset: [0, 0],
    });
    let r = GyroRequest {
        duration_seconds: 0.5,
        ..Default::default()
    };
    let result = analyze_with_metadata(&mut Cursor::new(&data), &m, &r).unwrap();
    assert_eq!(result.mounting_source, "embedded_legacy_145");
    assert!(!result.orientation.is_empty());
    m.window_crop_info = None;
    let result = analyze_with_metadata(&mut Cursor::new(&data), &m, &r).unwrap();
    assert_eq!(result.mounting_source, "unresolved");
    assert!(result.orientation.is_empty());
    m.camera_model = Some("Insta360 GO Ultra".into());
    assert!(analyze_with_metadata(&mut Cursor::new(&data), &m, &r).is_err());
}
#[test]
#[ignore = "requires explicitly supplied verified original X4 footage"]
fn real_x4_trim_map_and_auto_mount_keep_clip_frame_clock_separate_from_original_capture() {
    let path = std::env::var("VIDEO_KADR_INSTA360_LEGACY_SAMPLE").unwrap();
    let mut file = std::fs::File::open(path).unwrap();
    let r = analyze(
        &mut file,
        &GyroRequest {
            start_seconds: 2.25,
            duration_seconds: 0.5,
            orientation_tolerance_degrees: Some(0.005),
            ..Default::default()
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(r.mounting_source, "embedded_legacy_145");
    assert_eq!(r.synchronization, "exposure_midpoint_trim_map");
    assert_eq!(r.gravity_alignment.status, "aligned_initial_sample");
    assert_eq!(r.gravity_alignment.observed_preroll_seconds, 0.);
    assert_eq!(r.original_time_map_offset_seconds, Some(21.));
    let clock = r.source_frame_clock.unwrap();
    assert_eq!(
        (
            clock.numerator,
            clock.denominator,
            clock.source_start_seconds
        ),
        (100, 1, 2.25)
    );
    assert!(r.capture_clock[0].capture_seconds > 23. && r.capture_clock[0].capture_seconds < 23.3);
    assert!(r.orientation.len() > 1 && r.orientation.len() <= 20000);
    let raw = r.samples[0].acceleration_g;
    let force = [raw[2], -raw[0], raw[1]];
    let magnitude = force.iter().map(|v| v * v).sum::<f64>().sqrt();
    assert!((0.85..=1.15).contains(&magnitude));
    let q = r.orientation[0].quaternion;
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let v = force.map(|v| v / magnitude);
    let u = [q[1], q[2], q[3]];
    let a = cross(u, v);
    let b = cross(u, a);
    let world: [f64; 3] = std::array::from_fn(|i| v[i] + 2. * (q[0] * a[i] + b[i]));
    assert!((world[1] - 1.).abs() < 1e-10);
    println!("real X4 strict145/trim21sec: sourceClock100fps at2.25sec; originalCapture{:.9}sec; {} IMU records; {} orientation samples",r.capture_clock[0].capture_seconds,r.input_samples,r.orientation.len());
}

fn trim_dynamic_start_fixture(
    stable_history: bool,
    sign: f64,
) -> (
    Vec<u8>,
    video_kadr_backend::domain::insta360_metadata::Metadata,
) {
    let (mut data, m) = mapped_fixture();
    let block = m.blocks.iter().find(|b| b.kind == 3).unwrap();
    for row in data[block.offset as usize..block.offset as usize + block.length as usize]
        .chunks_exact_mut(20)
    {
        let ts = u64::from_le_bytes(row[..8].try_into().unwrap());
        let gravity = if stable_history && ts < 1_160_000 {
            1.
        } else {
            1.4
        };
        let y = (32768. + sign * gravity * 1024.).round() as u16;
        for (i, word) in [32768u16, y, 32768, 32768, 32868, 32768]
            .into_iter()
            .enumerate()
        {
            row[8 + 2 * i..10 + 2 * i].copy_from_slice(&word.to_le_bytes());
        }
    }
    (data, m)
}
#[test]
fn observed_trim_preroll_initializes_gravity_but_preserves_clip_yaw_and_all_clocks() {
    use video_kadr_backend::domain::{
        gyro_drift::orientation_at, insta360_gyro::analyze_with_metadata,
        orientation_track::angular_error_degrees,
    };
    let (data, m) = trim_dynamic_start_fixture(true, 1.);
    let request = GyroRequest {
        duration_seconds: 0.5,
        motion_padding_seconds: 0.03,
        orientation_tolerance_degrees: Some(0.005),
        camera_from_sensor: Some([1., 0., 0., 0.]),
        ..Default::default()
    };
    let aligned = analyze_with_metadata(&mut Cursor::new(&data), &m, &request).unwrap();
    let unseeded = analyze_with_metadata(
        &mut Cursor::new(&data),
        &m,
        &GyroRequest {
            gravity_preroll_seconds: 0.,
            ..request.clone()
        },
    )
    .unwrap();
    assert_eq!(aligned.gravity_alignment.status, "estimated_observed_preroll");
    assert!(aligned.gravity_alignment.warning.as_deref().is_some_and(|s| !s.is_empty()));
    assert!((0. ..=1.).contains(&aligned.gravity_alignment.observed_preroll_seconds));
    assert!(aligned.gravity_alignment.initialization_samples >= 10);
    assert_eq!(
        unseeded.gravity_alignment.status,
        "unavailable_dynamic_start"
    );
    assert!(unseeded.gravity_alignment.warning.is_some());
    assert_eq!(aligned.capture_clock, unseeded.capture_clock);
    assert_eq!(
        aligned.original_time_map_offset_seconds,
        unseeded.original_time_map_offset_seconds
    );
    assert_eq!(aligned.orientation_offset_seconds, 0.03);
    let source = aligned.source_frame_clock.unwrap();
    assert_eq!(
        (
            source.numerator,
            source.denominator,
            source.source_start_seconds
        ),
        (50, 1, 0.)
    );
    let clip_q = orientation_at(&aligned.orientation, 0.03).unwrap();
    let error = angular_error_degrees(clip_q, [1., 0., 0., 0.]);
    // Transport uses the existing bounded NLERP/compression contract; its
    //0.005-degree budget also covers the authored origin after guard retiming.
    assert!(error < 0.0001, "clipq{clip_q:?}error{error}");
    let rate = 100. * 2000. / 32768. * std::f64::consts::PI / 180.;
    let expected = [(rate * 0.1).cos(), 0., (rate * 0.1).sin(), 0.];
    assert!(
        angular_error_degrees(
            orientation_at(&aligned.orientation, 0.23).unwrap(),
            expected
        ) < 0.005
    );
}
#[test]
fn absent_gravity_observation_is_explicit_and_untrimmed_or_disabled_paths_do_not_search_history() {
    use video_kadr_backend::domain::insta360_gyro::analyze_with_metadata;
    let (data, m) = trim_dynamic_start_fixture(false, 1.);
    let request = GyroRequest {
        duration_seconds: 0.5,
        camera_from_sensor: Some([1., 0., 0., 0.]),
        ..Default::default()
    };
    let result = analyze_with_metadata(&mut Cursor::new(&data), &m, &request).unwrap();
    assert_eq!(result.gravity_alignment.status, "unavailable_dynamic_start");
    assert_eq!(result.gravity_alignment.initialization_samples, 0);
    assert!(result
        .gravity_alignment
        .warning
        .unwrap()
        .contains("not established"));
    let mut untrimmed = m.clone();
    untrimmed.blocks.retain(|b| b.kind != 128);
    let result = analyze_with_metadata(&mut Cursor::new(&data), &untrimmed, &request).unwrap();
    assert_eq!(result.gravity_alignment.observed_preroll_seconds, 0.);
    assert_eq!(result.gravity_alignment.status, "unavailable_dynamic_start");
    let result = analyze_with_metadata(
        &mut Cursor::new(&data),
        &m,
        &GyroRequest {
            accelerometer_correction: false,
            ..request
        },
    )
    .unwrap();
    assert_eq!(result.gravity_alignment.status, "disabled");
    assert_eq!(result.gravity_alignment.observed_preroll_seconds, 0.);
    assert!(result.gravity_alignment.warning.is_none());
}
#[test]
fn observed_trim_preroll_canonical_yaw_is_finite_at_upside_down_gravity() {
    use video_kadr_backend::domain::{
        gyro_drift::orientation_at, insta360_gyro::analyze_with_metadata,
        orientation_track::angular_error_degrees,
    };
    let (data, m) = trim_dynamic_start_fixture(true, -1.);
    let r = analyze_with_metadata(
        &mut Cursor::new(data),
        &m,
        &GyroRequest {
            duration_seconds: 0.5,
            motion_padding_seconds: 0.03,
            orientation_tolerance_degrees: Some(0.005),
            camera_from_sensor: Some([1., 0., 0., 0.]),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.gravity_alignment.status, "estimated_observed_preroll");
    let q = orientation_at(&r.orientation, 0.03).unwrap();
    assert!(q.iter().all(|v| v.is_finite()));
    let error = angular_error_degrees(q, [0., 1., 0., 0.]);
    assert!(error < 0.0001, "clipq{q:?}error{error}");
    for seconds in [-1., 1.01, f64::NAN] {
        assert!(GyroRequest {
            gravity_preroll_seconds: seconds,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}
#[test]
#[ignore = "requires explicitly supplied verified original X4 footage"]
fn real_x4_dynamic_first_frame_uses_bounded_observed_preroll_and_warns_when_disabled() {
    let path = std::env::var("VIDEO_KADR_INSTA360_LEGACY_SAMPLE").unwrap();
    let mut file = std::fs::File::open(path).unwrap();
    let request = GyroRequest {
        duration_seconds: 0.5,
        orientation_tolerance_degrees: Some(0.005),
        ..Default::default()
    };
    let seeded = analyze(&mut file, &request).unwrap().unwrap();
    let disabled = analyze(
        &mut file,
        &GyroRequest {
            gravity_preroll_seconds: 0.,
            ..request
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(seeded.gravity_alignment.status, "estimated_observed_preroll");
    assert!(seeded.gravity_alignment.warning.as_deref().is_some_and(|s| !s.is_empty()));
    assert!(seeded.gravity_alignment.requested_force_norm_g > 1.15);
    assert!((0.05..=1.).contains(&seeded.gravity_alignment.observed_preroll_seconds));
    assert!(
        seeded.gravity_alignment.initialization_samples >= 10
            && seeded.gravity_alignment.initialization_samples <= 80
    );
    assert_eq!(
        disabled.gravity_alignment.status,
        "unavailable_dynamic_start"
    );
    assert!(disabled.gravity_alignment.warning.is_some());
    assert_eq!(seeded.capture_clock, disabled.capture_clock);
    assert_eq!(seeded.original_time_map_offset_seconds, Some(21.));
    let source = seeded.source_frame_clock.unwrap();
    assert_eq!(
        (
            source.numerator,
            source.denominator,
            source.source_start_seconds
        ),
        (100, 1, 0.)
    );
    assert!(seeded.orientation[0].quaternion[0].abs() < 0.8);
    println!("real X4 dynamic first frame: observed pre-roll{:.6}s/{}samples, source0sec/capture{:.9}sec unchanged; initialization unavailable is explicit when disabled",seeded.gravity_alignment.observed_preroll_seconds,seeded.gravity_alignment.initialization_samples,seeded.capture_clock[0].capture_seconds);
}

#[test]
fn constant_norm_lateral_force_cannot_create_large_gravity_tilt() {
    for ramp_seconds in [0f64,3.,100.] {
        let samples:Vec<_>=(0..=12000).map(|i| {
            let t=i as f64/100.;let angle=if t<1. {0.}else if ramp_seconds==0. {0.3}else {((t-1.)/ramp_seconds).min(1.)*0.3};
            SensorSample{time_seconds:t,acceleration_g:[angle.sin(),angle.cos(),0.],radians_per_second:[0.;3]}
        }).collect();
        let poses=fuse(&samples,[1.,0.,0.,0.],true).unwrap();
        for q in poses {let angle=2.*q.quaternion[0].abs().clamp(-1.,1.).acos();assert!(angle<=0.251f64.to_radians(),"ramp {ramp_seconds} invented {angle}");}
    }
}
#[test]
fn observed_gyro_tilt_survives_dynamic_specific_force() {
    let samples:Vec<_>=(0..=300).map(|i| {
        let t=i as f64/100.;SensorSample{time_seconds:t,acceleration_g:if i==0 {[0.,1.,0.]}else {[0.6,0.8,0.]},radians_per_second:[0.2,0.,0.]}
    }).collect();
    let raw=fuse(&samples,[1.,0.,0.,0.],false).unwrap();let corrected=fuse(&samples,[1.,0.,0.,0.],true).unwrap();
    for (a,b) in raw.iter().zip(corrected) {for i in 0..4 {assert!((a.quaternion[i]-b.quaternion[i]).abs()<1e-12);}}
}
