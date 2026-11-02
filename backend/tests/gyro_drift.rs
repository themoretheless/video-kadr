use video_kadr_backend::domain::{
    gyro_drift::{self, DriftCorrection, DriftReport, HeadingAnchor, StationaryInterval},
    insta360_gyro::{fuse, SensorSample},
    spherical_optical_flow::Correspondence,
    spherical_video::Orientation,
    visual_gyro_drift::{self, WorldFrame},
};
fn yaw(q: [f64; 4]) -> f64 {
    (2. * (q[1] * q[3] + q[0] * q[2])).atan2(1. - 2. * (q[1] * q[1] + q[2] * q[2]))
}
fn samples(rate: f64) -> Vec<SensorSample> {
    (0..=2000)
        .map(|i| SensorSample {
            time_seconds: i as f64 / 100.,
            acceleration_g: [0., 1., 0.],
            radians_per_second: [0., rate, 0.],
        })
        .collect()
}
#[test]
fn declared_stationary_bias_is_removed_but_constant_intentional_pan_is_not_assumed_stationary() {
    let data = samples(0.01);
    let config = DriftCorrection {
        stationary_intervals: vec![StationaryInterval {
            start_seconds: 0.,
            end_seconds: 2.,
        }],
        ..Default::default()
    };
    config.validate(20.).unwrap();
    let report = gyro_drift::calibrate_bias(&data, &config, &config.stationary_intervals).unwrap();
    assert!((report.bias_radians_per_second[1] - 0.01).abs() < 1e-12);
    let corrected = gyro_drift::bias_corrected(&data, report.bias_radians_per_second);
    let fixed = fuse(&corrected, [1., 0., 0., 0.], true).unwrap();
    assert!(yaw(fixed.last().unwrap().quaternion).abs() < 1e-10);
    let unobserved = gyro_drift::calibrate_bias(&data, &DriftCorrection::default(), &[]).unwrap();
    assert_eq!(unobserved.source, "none");
    let unchanged = fuse(&data, [1., 0., 0., 0.], true).unwrap();
    assert!((yaw(unchanged.last().unwrap().quaternion) - 0.2).abs() < 1e-10);
    let mut moving = data.clone();
    for (i, s) in moving.iter_mut().enumerate() {
        s.acceleration_g[0] = (i as f64 * 0.3).sin() * 0.08;
    }
    assert!(gyro_drift::stationary_bias(&moving, &config.stationary_intervals).is_err());
    let mut unstable = data.clone();
    for (i, s) in unstable.iter_mut().enumerate() {
        s.radians_per_second[1] += (i as f64 * 0.7).sin() * 0.02;
    }
    assert!(gyro_drift::stationary_bias(&unstable, &config.stationary_intervals).is_err());
}
#[test]
fn observed_heading_corrects_yaw_drift_without_erasing_real_pan_or_tilt() {
    let data = samples(0.025);
    let mut track = fuse(&data, [1., 0., 0., 0.], true).unwrap();
    let config = DriftCorrection {
        heading_anchors: vec![
            HeadingAnchor {
                time_seconds: 0.,
                heading_degrees: 0.,
            },
            HeadingAnchor {
                time_seconds: 20.,
                heading_degrees: (0.02f64 * 20.).to_degrees(),
            },
        ],
        ..Default::default()
    };
    config.validate(20.).unwrap();
    let mut report = DriftReport::default();
    gyro_drift::apply_observed_headings(&mut track, &config, &mut report).unwrap();
    assert!((yaw(track.last().unwrap().quaternion) - 0.4).abs() < 1e-9);
    assert!(yaw(track[1000].quaternion) > 0.19);
    assert_eq!(report.heading_anchors, 2);
    assert!((report.corrected_heading_degrees - (-0.1f64).to_degrees()).abs() < 1e-9);
    // Full turns are preserved because correction is relative to the integrated path.
    let mut turning = fuse(&samples(0.4), [1., 0., 0., 0.], true).unwrap();
    let observation = DriftCorrection {
        heading_anchors: vec![
            HeadingAnchor {
                time_seconds: 0.,
                heading_degrees: 0.,
            },
            HeadingAnchor {
                time_seconds: 10.,
                heading_degrees: 4f64.to_degrees(),
            },
            HeadingAnchor {
                time_seconds: 20.,
                heading_degrees: 8f64.to_degrees(),
            },
        ],
        ..Default::default()
    };
    let before = turning.clone();
    gyro_drift::apply_observed_headings(&mut turning, &observation, &mut DriftReport::default())
        .unwrap();
    for (a, b) in turning.iter().zip(before) {
        assert!(
            video_kadr_backend::domain::orientation_track::angular_error_degrees(
                a.quaternion,
                b.quaternion
            ) < 1e-5
        );
    }
    let mut tilted = vec![
        Orientation {
            time_seconds: 0.,
            quaternion: [0.8, 0.6, 0., 0.],
        },
        Orientation {
            time_seconds: 1.,
            quaternion: [0.8, 0.6, 0., 0.],
        },
    ];
    let original = tilted[0].quaternion;
    gyro_drift::apply_heading_offsets(
        &mut tilted,
        &[
            gyro_drift::HeadingOffset {
                time_seconds: 0.,
                radians: 0.2,
            },
            gyro_drift::HeadingOffset {
                time_seconds: 1.,
                radians: 0.4,
            },
        ],
    )
    .unwrap();
    let up_y = |q: [f64; 4]| 1. - 2. * (q[1] * q[1] + q[3] * q[3]);
    assert!((up_y(tilted[0].quaternion) - up_y(original)).abs() < 1e-12);
}
#[test]
fn contradictory_or_unbounded_calibration_is_rejected() {
    for value in [
        serde_json::json!({"biasRadiansPerSecond":[1,0,0]}),
        serde_json::json!({"stationaryIntervals":[{"startSeconds":0,"endSeconds":0.1}]}),
        serde_json::json!({"headingAnchors":[{"timeSeconds":0,"headingDegrees":0}]}),
    ] {
        let c: DriftCorrection = serde_json::from_value(value).unwrap();
        assert!(c.validate(20.).is_err());
    }
}
fn matches(shift: f64) -> Vec<Correspondence> {
    (0..12)
        .flat_map(|x| {
            (0..5).map(move |y| {
                let u = (x as f64 + 0.5) / 12.;
                let v = 0.23 + y as f64 * 0.12;
                Correspondence {
                    from: [u, v],
                    to: [(u + shift).rem_euclid(1.), v],
                    confidence: 0.9,
                }
            })
        })
        .collect()
}
#[test]
fn yaw_observations_require_distributed_background_and_reject_foreground_motion() {
    let mut points = matches(0.3 / 360.);
    for x in 0..20 {
        let u = 0.1 + (x % 4) as f64 / 100.;
        let v = 0.3 + (x / 4) as f64 / 12.;
        points.push(Correspondence {
            from: [u, v],
            to: [u + 3. / 360., v],
            confidence: 0.9,
        });
    }
    let measured = visual_gyro_drift::yaw_consensus(&points).unwrap();
    assert!((measured.radians.to_degrees() - 0.3).abs() < 1e-10);
    assert!(measured.longitude_sectors >= 6);
    let foreground: Vec<_> = (0..60)
        .map(|i| Correspondence {
            from: [0.2 + (i % 10) as f64 / 500., 0.2 + (i / 10) as f64 / 12.],
            to: [0.21 + (i % 10) as f64 / 500., 0.2 + (i / 10) as f64 / 12.],
            confidence: 1.,
        })
        .collect();
    assert!(visual_gyro_drift::yaw_consensus(&foreground).is_none());
    let mut vertical = matches(0.3 / 360.);
    for m in &mut vertical {
        m.to[1] += 0.1;
    }
    assert!(visual_gyro_drift::yaw_consensus(&vertical).is_none());
}
fn texture() -> WorldFrame {
    let (w, h) = (256, 128);
    let mut v: Vec<_> = (0..w * h)
        .map(|i| {
            let mut n = (i as u32).wrapping_mul(2654435761);
            n = (n ^ (n >> 13)).wrapping_mul(1274126177);
            (n & 255) as f32 / 255.
        })
        .collect();
    for _ in 0..3 {
        let old = v.clone();
        for y in 0..h {
            for x in 0..w {
                let mut s = 0.;
                for dy in -1isize..=1 {
                    for dx in -1isize..=1 {
                        s += old[(y as isize + dy).clamp(0, h as isize - 1) as usize * w
                            + (x as isize + dx).rem_euclid(w as isize) as usize];
                    }
                }
                v[y * w + x] = s / 9.;
            }
        }
    }
    WorldFrame {
        width: w,
        height: h,
        pixels: v.into_iter().map(|v| [v, v, v, 1.]).collect(),
    }
}
#[test]
fn actual_subpixel_image_flow_observes_small_drift_and_flat_images_do_not() {
    let a = texture();
    let shift = 0.4 / 256.;
    let b = WorldFrame {
        width: a.width,
        height: a.height,
        pixels: (0..a.width * a.height)
            .map(|i| {
                a.sample(
                    (i % a.width) as f64 / a.width as f64 + 0.5 / a.width as f64 - shift,
                    (i / a.width) as f64 / a.height as f64 + 0.5 / a.height as f64,
                )
                .unwrap()
            })
            .collect(),
    };
    let observed = visual_gyro_drift::observe(&a, &b, &mut || Ok(()))
        .unwrap()
        .unwrap();
    assert!(
        (observed.radians.to_degrees() - shift * 360.).abs() < 0.12,
        "observed {observed:?}"
    );
    let flat = WorldFrame {
        width: 256,
        height: 128,
        pixels: vec![[0.5, 0.5, 0.5, 1.]; 256 * 128],
    };
    assert!(visual_gyro_drift::observe(&flat, &flat, &mut || Ok(()))
        .unwrap()
        .is_none());
}

#[test]
fn compressed_gyro_track_keeps_interior_heading_knots_and_noncommuting_motion() {
    use gyro_drift::HeadingOffset;
    let track = vec![
        Orientation {
            time_seconds: 0.,
            quaternion: [1., 0., 0., 0.],
        },
        Orientation {
            time_seconds: 10.,
            quaternion: [0.5, 3f64.sqrt() / 2., 0., 0.],
        },
    ];
    let offsets = [
        HeadingOffset {
            time_seconds: 0.,
            radians: 0.,
        },
        HeadingOffset {
            time_seconds: 5.,
            radians: 0.1,
        },
        HeadingOffset {
            time_seconds: 10.,
            radians: 0.,
        },
    ];
    let corrected = gyro_drift::corrected_heading_track(&track, &offsets).unwrap();
    assert!(corrected.iter().any(|q| q.time_seconds == 5.));
    for i in 0..=100 {
        let t = i as f64 / 10.;
        let x = t * std::f64::consts::PI / 15.;
        let yaw = if t <= 5. { 0.02 * t } else { 0.02 * (10. - t) };
        let (sx, cx) = (x * 0.5).sin_cos();
        let (sy, cy) = (yaw * 0.5).sin_cos();
        let expected = [cy * cx, cy * sx, sy * cx, -sy * sx];
        let actual = gyro_drift::orientation_at(&corrected, t).unwrap();
        let error =
            video_kadr_backend::domain::orientation_track::angular_error_degrees(actual, expected);
        assert!(error < 0.0002, "at{t}: {error}");
    }
}

#[test]
fn spherical_rotation_consensus_separates_yaw_from_residual_gravity_tilt() {
    let (mut data, mut expected) = (vec![], 0.);
    for x in 0..16 {
        for y in 0..6 {
            let u = (x as f64 + 0.3) / 16.;
            let v = 0.18 + y as f64 * 0.12;
            let lon = (u - 0.5) * std::f64::consts::TAU;
            let lat = (0.5 - v) * std::f64::consts::PI;
            let a = [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()];
            let (pitch, yaw) = (0.075f64, 0.006f64);
            expected = yaw;
            let rx = [
                a[0],
                pitch.cos() * a[1] - pitch.sin() * a[2],
                pitch.sin() * a[1] + pitch.cos() * a[2],
            ];
            let b = [
                yaw.cos() * rx[0] + yaw.sin() * rx[2],
                rx[1],
                -yaw.sin() * rx[0] + yaw.cos() * rx[2],
            ];
            let to = [
                0.5 + b[0].atan2(b[2]) / std::f64::consts::TAU,
                0.5 - b[1].asin() / std::f64::consts::PI,
            ];
            data.push(Correspondence {
                from: [u, v],
                to,
                confidence: 0.9,
            });
        }
    }
    let result = visual_gyro_drift::spherical_rotation_yaw(&data).unwrap();
    assert!((result.radians - expected).abs() < 1e-8);
    assert!(result.residual_degrees < 1e-5);
    let localized: Vec<_> = data
        .into_iter()
        .filter(|p| p.from[0] > 0.2 && p.from[0] < 0.4)
        .collect();
    assert!(visual_gyro_drift::spherical_rotation_yaw(&localized).is_none());
}

#[test]
fn essential_background_rotation_separates_translation_and_rejects_degeneracy() {
    use std::f64::consts::{PI, TAU};
    fn uv(v: [f64; 3]) -> [f64; 2] {
        let n = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        [0.5 + v[0].atan2(v[2]) / TAU, 0.5 - (v[1] / n).asin() / PI]
    }
    let angle = 0.006f64;
    let (c, s) = (angle.cos(), angle.sin());
    let mut matches = Vec::new();
    for y in 0..7 {
        for x in 0..36 {
            let u = (x as f64 + 0.3) / 36.;
            let v = 0.22 + y as f64 * 0.08;
            let lon = (u - 0.5) * TAU;
            let lat = (0.5 - v) * PI;
            let a = [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()];
            let depth = 3. + ((x * 17 + y * 11) % 23) as f64 * 0.3;
            let p = a.map(|x| x * depth);
            let b = [
                c * p[0] + s * p[2] + 0.23,
                p[1] + 0.04,
                -s * p[0] + c * p[2] + 0.1,
            ];
            matches.push(Correspondence {
                from: [u, v],
                to: uv(b),
                confidence: 0.95,
            });
        }
    }
    let fit = visual_gyro_drift::essential_yaw(&matches, &mut || Ok(()))
        .unwrap()
        .expect("observable translated sphere");
    assert!((fit.radians - angle).abs() < 0.0002, "{fit:?}");
    let mut distant = matches.clone();
    for m in &mut distant {
        let lon = (m.from[0] - 0.5) * TAU;
        let lat = (0.5 - m.from[1]) * PI;
        let a = [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()];
        m.to = uv([c * a[0] + s * a[2] + 0.00001, a[1], -s * a[0] + c * a[2]]);
    }
    assert!(
        visual_gyro_drift::essential_yaw(&distant, &mut || Ok(()))
            .unwrap()
            .is_none(),
        "unobservable distant scene"
    );
    let mut plane = matches.clone();
    for m in &mut plane {
        let lon = (m.from[0] - 0.5) * TAU;
        let lat = (0.5 - m.from[1]) * PI;
        let a = [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()];
        let d = 4. / a[2];
        let p = a.map(|x| x * d);
        m.to = uv([
            c * p[0] + s * p[2] + 0.23,
            p[1] + 0.04,
            -s * p[0] + c * p[2] + 0.1,
        ]);
    }
    assert!(
        visual_gyro_drift::essential_yaw(&plane, &mut || Ok(()))
            .unwrap()
            .is_none(),
        "planar design ambiguity"
    );
    // Pure rotation has no triangulation baseline and must use the rigid solver.
    for m in &mut matches {
        let lon = (m.from[0] - 0.5) * TAU;
        let lat = (0.5 - m.from[1]) * PI;
        let a = [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()];
        m.to = uv([c * a[0] + s * a[2], a[1], -s * a[0] + c * a[2]]);
    }
    assert!(visual_gyro_drift::essential_yaw(&matches, &mut || Ok(()))
        .unwrap()
        .is_none());
    assert!(
        visual_gyro_drift::essential_yaw(&matches, &mut || anyhow::bail!("cancelled")).is_err()
    );
}
