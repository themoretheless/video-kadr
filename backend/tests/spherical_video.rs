use video_kadr_backend::domain::{
    composition::VideoEffect,
    flat_surface,
    group_surface::GroupSurface,
    spherical_video::{Input, Orientation, Output, SphericalVideo},
};
fn grid() -> GroupSurface {
    let mut s = GroupSurface::transparent(64, 32).unwrap();
    for y in 0..32 {
        for x in 0..64 {
            let i = (y * 64 + x) * 4;
            s.rgba[i..i + 4].copy_from_slice(&[(x * 4) as u8, (y * 8) as u8, 60, 255]);
        }
    }
    s
}
fn render(s: &mut GroupSurface, p: SphericalVideo, time: u64) {
    flat_surface::apply_effect_stack(
        s,
        &[VideoEffect::SphericalVideo {
            parameters: p,
            animation: None,
        }],
        time,
        &[],
        &mut || Ok(()),
    )
    .unwrap();
}
#[test]
fn spherical_video_identity_panorama_and_render_dispatch() {
    let source = grid();
    let mut result = source.clone();
    let p = SphericalVideo {
        output: Output::Equirectangular,
        ..Default::default()
    };
    let effect = VideoEffect::SphericalVideo {
        parameters: p.clone(),
        animation: None,
    };
    assert!(flat_surface::supported_effect_stack(&[effect.clone()]));
    assert!(p.validate().is_ok());
    let bytes = serde_json::to_vec(&effect).unwrap();
    let restored: VideoEffect = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        serde_json::to_value(&restored).unwrap(),
        serde_json::to_value(&effect).unwrap()
    );
    render(&mut result, p, 0);
    assert_eq!(result, source);
}
#[test]
fn spherical_video_camera_pan_is_geometric_and_orientation_cancels_rotation() {
    let source = grid();
    let mut front = source.clone();
    render(&mut front, SphericalVideo::default(), 0);
    let center = (16 * 64 + 32) * 4;
    assert!((front.rgba[center] as i32 - 126).abs() < 5);
    assert!((front.rgba[center + 1] as i32 - 124).abs() < 5);
    let mut right = source.clone();
    render(
        &mut right,
        SphericalVideo {
            pan: 90.,
            ..Default::default()
        },
        0,
    );
    assert!((right.rgba[center] as i32 - 190).abs() < 5);
    let mut cancelled = source;
    render(
        &mut cancelled,
        SphericalVideo {
            pan: 90.,
            orientation: vec![
                Orientation {
                    time_seconds: 0.,
                    quaternion: [1., 0., 0., 0.],
                },
                Orientation {
                    time_seconds: 1.,
                    quaternion: [0., 0., 1., 0.],
                },
            ],
            ..Default::default()
        },
        500000,
    );
    for (a, b) in cancelled.rgba.iter().zip(front.rgba.iter()) {
        assert!(a.abs_diff(*b) <= 1)
    }
}
#[test]
fn spherical_video_stitch_blends_lenses_without_dark_seam() {
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
    render(
        &mut source,
        SphericalVideo {
            input: Input::DualFisheye,
            output: Output::Equirectangular,
            ..Default::default()
        },
        0,
    );
    let get = |x: usize| &source.rgba[(16 * 64 + x) * 4..(16 * 64 + x) * 4 + 4];
    assert!(get(32)[0] > 250);
    assert!(get(0)[2] > 250);
    assert!(get(16)[0] > 50 && get(16)[2] > 50);
    assert!(source.rgba.chunks_exact(4).all(|p| p[3] == 255));
}
#[test]
fn spherical_video_rejects_bad_data_and_checks_cancellation() {
    let mut p = SphericalVideo::default();
    p.orientation = vec![Orientation {
        time_seconds: 0.,
        quaternion: [0.; 4],
    }];
    assert!(p.validate().is_err());
    p.orientation.clear();
    p.fov = 180.;
    assert!(p.validate().is_err());
    p.fov = 90.;
    let mut s = grid();
    let before = s.clone();
    let e = VideoEffect::SphericalVideo {
        parameters: p,
        animation: None,
    };
    assert!(
        flat_surface::apply_effect_stack(&mut s, &[e], 0, &[], &mut || anyhow::bail!("cancelled"))
            .is_err()
    );
    assert_eq!(s, before);
}
#[test]
fn spherical_video_keyframes_are_applied_at_clip_time() {
    let animation=serde_json::from_value(serde_json::json!({"pan":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":1000000,"value":90}]}}})).unwrap();
    let mut animated = grid();
    flat_surface::apply_effect_stack(
        &mut animated,
        &[VideoEffect::SphericalVideo {
            parameters: SphericalVideo::default(),
            animation: Some(animation),
        }],
        500000,
        &[],
        &mut || Ok(()),
    )
    .unwrap();
    let mut reference = grid();
    render(
        &mut reference,
        SphericalVideo {
            pan: 45.,
            ..Default::default()
        },
        0,
    );
    assert_eq!(animated, reference);
}
#[test]
fn spherical_video_tiny_planet_and_antipodal_quaternions_preserve_uniform_panorama() {
    let mut surface = GroupSurface::transparent(32, 16).unwrap();
    for pixel in surface.rgba.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[40, 100, 220, 128]);
    }
    let original = surface.clone();
    render(
        &mut surface,
        SphericalVideo {
            output: Output::TinyPlanet,
            orientation: vec![
                Orientation {
                    time_seconds: 0.,
                    quaternion: [1., 0., 0., 0.],
                },
                Orientation {
                    time_seconds: 1.,
                    quaternion: [-1., 0., 0., 0.],
                },
            ],
            ..Default::default()
        },
        500000,
    );
    for (a, b) in surface.rgba.iter().zip(original.rgba) {
        assert!(a.abs_diff(b) <= 1)
    }
}

#[test]
fn spherical_video_exact_180_degree_lens_rims_do_not_leave_transparent_seams() {
    let mut surface = GroupSurface::transparent(2, 1).unwrap();
    surface.rgba = [120, 60, 20, 255].repeat(2);
    let mut parameters = SphericalVideo {
        input: Input::DualFisheye,
        output: Output::Equirectangular,
        ..Default::default()
    };
    for lens in &mut parameters.lenses {
        lens.fov = 180.;
    }
    render(&mut surface, parameters, 0);
    assert!(surface.rgba.chunks_exact(4).all(|p| p[3] == 255));
}

#[test]
fn horizon_preserves_heading_while_locked_removes_it() {
    use video_kadr_backend::domain::spherical_video::StabilizationMode;
    let mut baseline=grid(); render(&mut baseline,SphericalVideo::default(),0);
    let mut heading=grid(); render(&mut heading,SphericalVideo{
        orientation:vec![Orientation{time_seconds:0.,quaternion:[std::f64::consts::FRAC_1_SQRT_2,0.,std::f64::consts::FRAC_1_SQRT_2,0.]}],
        stabilization_mode:StabilizationMode::Horizon,..Default::default()
    },0);
    assert_eq!(heading.rgba,baseline.rgba);
    let mut rolled=grid(); render(&mut rolled,SphericalVideo{
        orientation:vec![Orientation{time_seconds:0.,quaternion:[std::f64::consts::FRAC_1_SQRT_2,0.,0.,std::f64::consts::FRAC_1_SQRT_2]}],
        stabilization_mode:StabilizationMode::Horizon,..Default::default()
    },0);
    let mut locked=grid(); render(&mut locked,SphericalVideo{
        orientation:vec![Orientation{time_seconds:0.,quaternion:[std::f64::consts::FRAC_1_SQRT_2,0.,0.,std::f64::consts::FRAC_1_SQRT_2]}],..Default::default()
    },0);
    assert_eq!(rolled.rgba,locked.rgba);
    assert_ne!(rolled.rgba,baseline.rgba);
}

#[test]
fn symmetric_smoothing_removes_short_jitter_and_handles_antipodes() {
    use video_kadr_backend::domain::spherical_video::StabilizationMode;
    let orientation:Vec<_>=(0..101).map(|i| {
        let angle=if i==50 {0.2f64}else{0.};
        let sign=if i%2==0 {1.}else{-1.};
        Orientation{time_seconds:i as f64/100.,quaternion:[sign*angle.cos(),0.,sign*angle.sin(),0.]}
    }).collect();
    let mut locked=grid();render(&mut locked,SphericalVideo{orientation:orientation.clone(),..Default::default()},500000);
    let mut smoothed=grid();render(&mut smoothed,SphericalVideo{orientation,stabilization_mode:StabilizationMode::Smooth,smooth_seconds:0.15,..Default::default()},500000);
    // Desired smooth path remains near zero while the measured camera has a short impulse.
    let center=(16*64+32)*4;
    assert!(locked.rgba[center].abs_diff(smoothed.rgba[center])<4);
    let invalid:SphericalVideo=serde_json::from_value(serde_json::json!({"smoothSeconds":0})).unwrap();
    assert!(invalid.validate().is_err());
}

#[test]
fn horizon_pole_crossing_matches_locked_render_and_tracking_frame() {
    use video_kadr_backend::domain::{spherical_video::StabilizationMode,spherical_tracking::Centre};
    let orientation:Vec<_>=[80f64,90.,100.,170.,180.,190.].iter().enumerate().map(|(i,p)| {
        let half=p.to_radians()/2.;
        Orientation{time_seconds:i as f64,quaternion:[half.cos(),half.sin(),0.,0.]}
    }).collect();
    let horizon=SphericalVideo{orientation,stabilization_mode:StabilizationMode::Horizon,..Default::default()};
    let locked=SphericalVideo{stabilization_mode:StabilizationMode::Locked,..horizon.clone()};
    for time in [0u64,1_000_000,2_000_000,4_000_000,5_000_000] {
        let mut actual=grid();let mut expected=grid();
        render(&mut actual,horizon.clone(),time);render(&mut expected,locked.clone(),time);
        assert_eq!(actual.rgba,expected.rgba,"pitch pole at {time}");
        let centre=Centre{pan:90.,tilt:0.,fov:None};
        let actual=horizon.map_tracking_centre(centre,time as f64/1_000_000.).unwrap();
        let expected=locked.map_tracking_centre(centre,time as f64/1_000_000.).unwrap();
        assert!((actual.pan-expected.pan).abs()<1e-10);
        assert!((actual.tilt-expected.tilt).abs()<1e-10);
    }
}
