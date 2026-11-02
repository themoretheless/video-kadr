use video_kadr_backend::domain::spherical_tracking::mask_centre;
fn plane(xs: &[usize]) -> Vec<u8> {
    let mut p = vec![0; 64 * 32];
    for y in 14..18 {
        for &x in xs {
            p[y * 64 + x] = 255;
        }
    }
    p
}
#[test]
fn semantic_centroid_wraps_panorama_seam() {
    let centre = mask_centre(64, 32, &plane(&[62, 63, 0, 1])).unwrap();
    assert!((centre.pan.abs() - 180.).abs() < 1e-8);
    assert!(centre.tilt.abs() < 1e-8);
    let right = mask_centre(64, 32, &plane(&[46, 47, 48, 49])).unwrap();
    assert!((right.pan - 90.).abs() < 1e-8);
}
#[test]
fn absent_ambiguous_and_invalid_semantic_masks_rejected() {
    assert!(mask_centre(64, 32, &vec![0; 64 * 32]).is_err());
    assert!(mask_centre(64, 32, &plane(&[14, 15, 16, 17, 46, 47, 48, 49])).is_err());
    assert!(mask_centre(64, 32, &[255; 8]).is_err());
    assert!(mask_centre(1, 4, &[255; 4]).is_err());
}
#[tokio::test]
#[ignore = "requires host FFmpeg/FFprobe; procedural semantic masks, no SAM inference"]
async fn lossless_sam_transport_vfr_clock_verified_planes_and_native_camera_centres() {
    use sha2::{Digest, Sha256};
    use tokio_util::sync::CancellationToken;
    use video_kadr_backend::{
        domain::{composition::FrameRateRatio, video_region::Attachment},
        process_control::ProcessRuntime,
        services::{
            concat_video_regions,
            local_person_mask_video::{self, Input, Tools},
        },
    };
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("semantic.raw");
    let movie = root.path().join("semantic.mkv");
    let decoded = root.path().join("decoded.raw");
    let mut raw = vec![0; 3 * 256 * 256];
    for frame in 0..3 {
        for y in 120..136 {
            for dx in 0..4 {
                let x = (254 + frame * 2 + dx) % 256;
                raw[frame * 256 * 256 + y * 256 + x] = 255;
            }
        }
    }
    std::fs::write(&source, &raw).unwrap();
    let hash = format!("{:x}", Sha256::digest(&raw));
    let runtime = ProcessRuntime::local_default();
    let cancel = CancellationToken::new();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let ffmpeg = std::path::PathBuf::from("/opt/homebrew/bin/ffmpeg");
    let ffprobe = std::path::PathBuf::from("/opt/homebrew/bin/ffprobe");
    let rate = FrameRateRatio {
        numerator: 10,
        denominator: 1,
    };
    let limit = std::time::Duration::from_secs(30);
    let metadata = local_person_mask_video::encode(
        &runtime,
        Tools {
            ffmpeg: &ffmpeg,
            ffprobe: &ffprobe,
        },
        Input {
            masks: &source,
            mask_hash: &hash,
            width: 256,
            height: 256,
            frames: 3,
            frame_rate: rate,
            staging_bytes: 16 * 1024 * 1024,
        },
        &movie,
        &cancel,
        limit,
        &tx,
    )
    .await
    .unwrap();
    local_person_mask_video::decode(
        &runtime,
        Tools {
            ffmpeg: &ffmpeg,
            ffprobe: &ffprobe,
        },
        &movie,
        &metadata,
        &decoded,
        16 * 1024 * 1024,
        &cancel,
        limit,
        &tx,
    )
    .await
    .unwrap();
    let points = vec![[0.99, 0.5]];
    let snapshot = Attachment {
        source_id: "source".into(),
        mask_source_id: "mask".into(),
        source_hash: "a".repeat(64),
        mask_hash: hash,
        mask_file_hash: metadata.file_hash,
        encoder_hash: "b".repeat(64),
        decoder_hash: "c".repeat(64),
        runtime_hash: "d".repeat(64),
        prompt_hash: format!("{:x}", Sha256::digest(serde_json::to_vec(&points).unwrap())),
        points,
        width: 64,
        height: 32,
        frames: 3,
        frame_rate: rate,
        frame_times_us: Some(vec![0, 100000, 400000, 500000]),
        seed_frame: 1,
        tracked_frames: 3,
        forward_lost_at: None,
        backward_lost_at: None,
    };
    let reader = concat_video_regions::open(&decoded, &snapshot, &cancel)
        .await
        .unwrap();
    for (time, expected) in [(50000, 180.0_f64), (200000, -177.1875), (450000, -174.375)] {
        let mask = reader.read(&snapshot.source_hash, time, &cancel).unwrap();
        let centre = mask_centre(mask.width, mask.height, &mask.probabilities).unwrap();
        assert!((centre.pan.abs() - expected.abs()).abs() < 1e-8);
        assert!(centre.tilt.abs() < 1e-8);
    }
    let mut changed = std::fs::read(&decoded).unwrap();
    changed[0] = 255;
    std::fs::write(&decoded, changed).unwrap();
    assert!(reader.read(&snapshot.source_hash, 50000, &cancel).is_err());
}
#[test]
fn semantic_framing_follows_size_without_seam_or_pole_spikes() {
    use video_kadr_backend::domain::spherical_tracking::{mask_fov, Framing};
    let framing = Framing {
        aspect_ratio: 2.,
        target_fill: 0.65,
        initial_fov: 90.,
    };
    let small = plane(&[30, 31, 32, 33]);
    let large = plane(&[26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37]);
    let fov =
        |mask: &[u8]| mask_fov(64, 32, mask, mask_centre(64, 32, mask).unwrap(), framing).unwrap();
    assert!(fov(&large) > fov(&small) + 20.);
    assert!((fov(&small) - fov(&plane(&[62, 63, 0, 1]))).abs() < 1e-8);
    let mut polar = vec![0; 64 * 32];
    for y in 0..4 {
        for x in 26..38 {
            polar[y * 64 + x] = 255;
        }
    }
    let polar_fov = fov(&polar);
    assert!(polar_fov.is_finite() && (10.0..=179.0).contains(&polar_fov));
    let mut taller = framing;
    taller.aspect_ratio = 0.75;
    assert!(
        mask_fov(64, 32, &small, mask_centre(64, 32, &small).unwrap(), taller).unwrap()
            < fov(&small)
    );
    let mut invalid = framing;
    invalid.target_fill = 0.;
    assert!(mask_fov(
        64,
        32,
        &small,
        mask_centre(64, 32, &small).unwrap(),
        invalid
    )
    .is_err());
}
#[test]
fn semantic_zoom_preserves_first_authored_fov_and_bounds_later_change() {
    use video_kadr_backend::domain::spherical_tracking::{frame_sequence, Centre};
    let mut centres = vec![
        Centre {
            pan: 0.,
            tilt: 0.,
            fov: None
        };
        4
    ];
    frame_sequence(
        &mut centres,
        &[20., 120., 120., 20.],
        &[0, 200000, 400000, 600000],
        90.,
    )
    .unwrap();
    assert_eq!(centres[0].fov, Some(90.));
    assert!(centres[1].fov.unwrap() > 90.);
    assert!(centres[2].fov.unwrap() > centres[1].fov.unwrap());
    assert!(centres[3].fov.unwrap() < centres[2].fov.unwrap());
    for adjacent in centres.windows(2) {
        assert!((adjacent[1].fov.unwrap() - adjacent[0].fov.unwrap()).abs() <= 6.000001);
    }
    assert!(frame_sequence(&mut centres, &[20., 120., 120., 20.], &[0, 0, 1, 2], 90.).is_err());
    assert!(serde_json::to_value(Centre {
        pan: 0.,
        tilt: 0.,
        fov: None
    })
    .unwrap()
    .get("fov")
    .is_none());
}

#[test]
fn semantic_framing_contains_elongated_subject_at_any_camera_roll() {
    use video_kadr_backend::domain::spherical_tracking::{mask_fov, Framing};
    let mask = plane(&[26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37]);
    let framing = Framing {
        aspect_ratio: 2.,
        target_fill: 0.65,
        initial_fov: 90.,
    };
    let fov = mask_fov(64, 32, &mask, mask_centre(64, 32, &mask).unwrap(), framing).unwrap();
    let extent = (fov.to_radians() * 0.5).tan();
    for roll in [0.0_f64, 45., 90., 137.] {
        let (s, c) = roll.to_radians().sin_cos();
        for x in 26..=38 {
            for y in 14..=18 {
                let lon = (x as f64 / 64. - 0.5) * std::f64::consts::TAU;
                let lat = (0.5 - y as f64 / 32.) * std::f64::consts::PI;
                let dx = lat.cos() * lon.sin();
                let dy = lat.sin();
                let z = lat.cos() * lon.cos();
                assert!((c * dx + s * dy).abs() / z <= extent * framing.target_fill + 1e-9);
                assert!(
                    (-s * dx + c * dy).abs() / z
                        <= extent / framing.aspect_ratio * framing.target_fill + 1e-9
                );
            }
        }
    }
    let mut tall = vec![0; 64 * 32];
    for y in 4..28 {
        for x in 31..33 {
            tall[y * 64 + x] = 255;
        }
    }
    assert!(mask_fov(64, 32, &tall, mask_centre(64, 32, &tall).unwrap(), framing).is_ok());
}
