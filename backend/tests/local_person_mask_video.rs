use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    domain::composition::FrameRateRatio,
    process_control::ProcessRuntime,
    services::local_person_mask_video::{self, Input, Tools},
};
#[tokio::test]
#[ignore = "requires host FFmpeg/FFprobe and independent original mask sequence"]
async fn actual_mask_video_exact_pixels_fractional_clock_cancel_mutation_and_no_replace() {
    let ffmpeg = PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFMPEG").unwrap());
    let ffprobe = PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFPROBE").unwrap());
    let oracle = PathBuf::from(std::env::var("VIDEO_KADR_TEST_PERSON_ORACLE_DIR").unwrap());
    let bytes = std::fs::read(oracle.join("reference.masks")).unwrap();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    for mode in [
        "whole",
        "fractional",
        "cancel",
        "changed",
        "occupied",
        "deadline",
        "budget",
        "stale",
    ] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.masks");
        let target = root.path().join("masks.mkv");
        std::fs::write(&source, &bytes).unwrap();
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let sp = source.clone();
        let tp = target.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let observer = tokio::spawn(async move {
            let mut acted = false;
            let mut max = 0f64;
            while let Some(value) = rx.recv().await {
                assert!(value >= max);
                max = value;
                if value > 0. && !acted {
                    acted = true;
                    match mode {
                        "cancel" => trigger.cancel(),
                        "changed" => std::fs::write(&sp, b"changed retained masks").unwrap(),
                        "occupied" => std::fs::write(&tp, b"occupied").unwrap(),
                        _ => {}
                    }
                }
            }
            (max, acted)
        });
        let rate = if mode == "fractional" {
            FrameRateRatio {
                numerator: 30000,
                denominator: 1001,
            }
        } else {
            FrameRateRatio {
                numerator: 4,
                denominator: 1,
            }
        };
        let bad = "0".repeat(64);
        let result = local_person_mask_video::encode(
            &ProcessRuntime::local_default(),
            Tools {
                ffmpeg: &ffmpeg,
                ffprobe: &ffprobe,
            },
            Input {
                masks: &source,
                mask_hash: if mode == "stale" { &bad } else { &hash },
                width: 512,
                height: 320,
                frames: 4,
                frame_rate: rate,
                staging_bytes: if mode == "budget" {
                    1
                } else {
                    64 * 1024 * 1024 * 1024
                },
            },
            &target,
            &cancel,
            if mode == "deadline" {
                Duration::from_millis(1)
            } else {
                Duration::from_secs(30)
            },
            &tx,
        )
        .await;
        drop(tx);
        let (max, acted) = observer.await.unwrap();
        if mode == "whole" || mode == "fractional" {
            let metadata = result.unwrap();
            assert_eq!(max, 100.);
            assert_eq!(metadata.mask_hash, hash);
            assert_eq!(metadata.frames, 4);
            assert_eq!(metadata.frame_rate, rate);
            assert_eq!(
                format!("{:x}", Sha256::digest(std::fs::read(&target).unwrap())),
                metadata.file_hash
            );
            let decoded = std::process::Command::new(&ffmpeg)
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(&target)
                .args([
                    "-map",
                    "0:v:0",
                    "-an",
                    "-fps_mode",
                    "passthrough",
                    "-pix_fmt",
                    "gray",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ])
                .output()
                .unwrap();
            assert!(decoded.status.success());
            assert_eq!(decoded.stdout, bytes);
            for decode_mode in [
                "exact",
                "cancel",
                "changed",
                "occupied",
                "deadline",
                "budget",
                "file_hash",
                "raw_hash",
                "count",
                "dimensions",
                "rate",
            ] {
                let movie = root.path().join(format!("{decode_mode}.mkv"));
                std::fs::copy(&target, &movie).unwrap();
                let restored = root.path().join(format!("{decode_mode}.raw"));
                let mut expected = metadata.clone();
                match decode_mode {
                    "file_hash" => expected.file_hash = "0".repeat(64),
                    "raw_hash" => expected.mask_hash = "0".repeat(64),
                    "count" => expected.frames += 1,
                    "dimensions" => expected.width -= 16,
                    "rate" => expected.frame_rate.numerator += 1,
                    _ => {}
                }
                let cancel = CancellationToken::new();
                let trigger = cancel.clone();
                let changed = movie.clone();
                let occupied = restored.clone();
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                let observer = tokio::spawn(async move {
                    let mut acted = false;
                    let mut max = 0.;
                    while let Some(value) = rx.recv().await {
                        assert!(value >= max);
                        max = value;
                        if value > 0. && !acted {
                            acted = true;
                            match decode_mode {
                                "cancel" => trigger.cancel(),
                                "changed" => {
                                    std::fs::write(&changed, b"changed archived mask").unwrap()
                                }
                                "occupied" => std::fs::write(&occupied, b"foreign output").unwrap(),
                                _ => {}
                            }
                        }
                    }
                    (max, acted)
                });
                let result = local_person_mask_video::decode(
                    &ProcessRuntime::local_default(),
                    Tools {
                        ffmpeg: &ffmpeg,
                        ffprobe: &ffprobe,
                    },
                    &movie,
                    &expected,
                    &restored,
                    if decode_mode == "budget" {
                        1
                    } else {
                        64 * 1024 * 1024 * 1024
                    },
                    &cancel,
                    if decode_mode == "deadline" {
                        Duration::from_millis(1)
                    } else {
                        Duration::from_secs(30)
                    },
                    &tx,
                )
                .await;
                drop(tx);
                let (max, acted) = observer.await.unwrap();
                if decode_mode == "exact" {
                    result.unwrap();
                    assert_eq!(max, 100.);
                    assert_eq!(std::fs::read(&restored).unwrap(), bytes);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        assert_eq!(
                            std::fs::metadata(&restored).unwrap().permissions().mode() & 0o777,
                            0o600
                        );
                    }
                } else {
                    assert!(result.is_err(), "{mode}/{decode_mode}");
                    if ["cancel", "changed", "occupied"].contains(&decode_mode) {
                        assert!(acted);
                    }
                    if decode_mode == "occupied" {
                        assert_eq!(std::fs::read(&restored).unwrap(), b"foreign output");
                    } else {
                        assert!(!restored.exists(), "{mode}/{decode_mode}");
                    }
                }
            }

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        } else {
            assert!(result.is_err(), "{mode}");
            if ["cancel", "changed", "occupied"].contains(&mode) {
                assert!(acted);
            }
            if mode == "occupied" {
                assert_eq!(std::fs::read(&target).unwrap(), b"occupied");
            } else {
                assert!(!target.exists(), "{mode}");
            }
        }
        assert!(!std::fs::read_dir(root.path()).unwrap().any(|v| v
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".person-mask-video-")));
    }
}
