use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::{
        local_sam::Config,
        local_sam_video::{self, Input, Tools},
    },
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn run(tool: &Path, args: &[&str]) {
    let output = Command::new(tool).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[tokio::test]
#[ignore = "requires native SAM, pinned models/ORT, FFmpeg/FFprobe and original tracking oracles"]
async fn actual_movie_seed_tracking_portable_masks_and_failure_cleanup() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        encoder: std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap().into(),
        decoder: std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap().into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let ffmpeg = PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFMPEG").unwrap());
    let ffprobe = PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFPROBE").unwrap());
    let oracles = PathBuf::from(std::env::var("VIDEO_KADR_TEST_SAM_TRACK_ORACLE_DIR").unwrap());
    for geometry in ["37x23", "23x37", "80x80"] {
        let oracle = oracles.join(geometry);
        let reference: serde_json::Value =
            serde_json::from_slice(&std::fs::read(oracle.join("reference.json")).unwrap()).unwrap();
        let width = reference["width"].as_u64().unwrap() as u32;
        let height = reference["height"].as_u64().unwrap() as u32;
        let source_rgba = std::fs::read(oracle.join("source.rgba")).unwrap();
        let masks = std::fs::read(oracle.join("reference.masks")).unwrap();
        let modes = if geometry == "37x23" {
            vec![
                "whole",
                "fractional",
                "rotation",
                "delay",
                "cancel",
                "changed",
                "late_occupied",
                "occupied",
                "budget",
                "deadline",
                "seed_frame",
                "stale",
                "vfr",
                "hdr",
                "deep",
            ]
        } else {
            vec!["whole"]
        };
        for mode in modes {
            let root = tempfile::tempdir().unwrap();
            let raw = root.path().join("input.rgba");
            let movie = root.path().join("source.mov");
            let destination = root.path().join("regions.mkv");
            let raw_dest = root.path().join("regions.gray");
            let (coded_width, coded_height) = if mode == "rotation" {
                (height, width)
            } else {
                (width, height)
            };
            let bytes = if mode == "rotation" {
                source_rgba
                    .chunks_exact((width * height * 4) as usize)
                    .flat_map(|frame| {
                        image::imageops::rotate90(
                            &image::RgbaImage::from_raw(width, height, frame.to_vec()).unwrap(),
                        )
                        .into_raw()
                    })
                    .collect()
            } else {
                source_rgba.clone()
            };
            std::fs::write(&raw, bytes).unwrap();
            let size = format!("{coded_width}x{coded_height}");
            let rate = if mode == "fractional" {
                "30000/1001"
            } else {
                "4"
            };
            let mut args = vec![
                "-nostdin",
                "-v",
                "error",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                &size,
                "-framerate",
                rate,
                "-i",
                raw.to_str().unwrap(),
            ];
            if mode == "whole" {
                args.extend([
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=733:sample_rate=48000:duration=1",
                    "-c:a",
                    "aac",
                    "-shortest",
                ]);
            } else {
                args.push("-an");
            }
            if mode == "vfr" {
                args.extend([
                    "-vf",
                    "setpts=if(lt(N\\,2)\\,N\\,N+1)/(4*TB)",
                    "-fps_mode",
                    "vfr",
                    "-c:v",
                    "ffv1",
                    "-pix_fmt",
                    "bgra",
                    "-f",
                    "matroska",
                ]);
            } else if mode == "hdr" || mode == "deep" {
                args.extend(["-c:v", "ffv1", "-pix_fmt", "yuv444p10le", "-f", "matroska"]);
                if mode == "hdr" {
                    args.extend(["-color_trc", "smpte2084"]);
                }
            } else {
                args.extend(["-c:v", "qtrle", "-pix_fmt", "argb"]);
            }
            if mode == "delay" {
                args.extend(["-output_ts_offset", "1"]);
            }
            args.push(movie.to_str().unwrap());
            run(&ffmpeg, &args);
            if mode == "rotation" {
                let rotated = root.path().join("rotated.mov");
                run(
                    &ffmpeg,
                    &[
                        "-nostdin",
                        "-v",
                        "error",
                        "-display_rotation",
                        "90",
                        "-i",
                        movie.to_str().unwrap(),
                        "-map",
                        "0",
                        "-c",
                        "copy",
                        rotated.to_str().unwrap(),
                    ],
                );
                std::fs::rename(rotated, &movie).unwrap();
            }
            let original_movie = std::fs::read(&movie).unwrap();
            let movie_hash = hash(&original_movie);
            let bad = "0".repeat(64);
            if mode == "occupied" {
                std::fs::write(&destination, b"foreign mask movie").unwrap();
            }
            let cancel = CancellationToken::new();
            let trigger = cancel.clone();
            let sp = movie.clone();
            let tp = destination.clone();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let observer = tokio::spawn(async move {
                let mut maximum = 0.;
                let mut acted = false;
                while let Some(value) = rx.recv().await {
                    assert!(value >= maximum, "{mode}: progress {value} < {maximum}");
                    maximum = value;
                    if value > 40. && !acted {
                        acted = true;
                        match mode {
                            "cancel" => trigger.cancel(),
                            "changed" => {
                                std::fs::write(&sp, b"changed source movie after decoding").unwrap()
                            }
                            "late_occupied" => std::fs::write(&tp, b"foreign mask movie").unwrap(),
                            _ => {}
                        }
                    }
                }
                (maximum, acted)
            });
            let result = local_sam_video::track(
                &ProcessRuntime::local_default(),
                Tools {
                    ffmpeg: &ffmpeg,
                    ffprobe: &ffprobe,
                },
                &config,
                Input {
                    source: &movie,
                    source_hash: if mode == "stale" { &bad } else { &movie_hash },
                    seed_frame: if mode == "seed_frame" { 4 } else { 1 },
                    points: &[[0.5, 0.5]],
                    mask_destination: Some(&raw_dest),
                    staging_bytes: if mode == "budget" {
                        1
                    } else {
                        64 * 1024 * 1024 * 1024
                    },
                },
                &destination,
                &cancel,
                if mode == "deadline" {
                    Duration::from_millis(1)
                } else {
                    Duration::from_secs(120)
                },
                &tx,
            )
            .await;
            drop(tx);
            let (maximum, acted) = observer.await.unwrap();
            if ["whole", "fractional", "rotation", "delay"].contains(&mode) {
                let result = result.unwrap_or_else(|e| panic!("{geometry}/{mode}: {e:?}"));
                assert_eq!(maximum, 100.);
                assert_eq!(
                    (
                        result.width,
                        result.height,
                        result.frames,
                        result.seed_frame
                    ),
                    (width, height, 4, 1)
                );
                assert_eq!(
                    result.frame_rate.numerator,
                    if mode == "fractional" { 30000 } else { 4 }
                );
                assert_eq!(
                    result.frame_rate.denominator,
                    if mode == "fractional" { 1001 } else { 1 }
                );
                assert_eq!(result.source_hash, movie_hash);
                assert_eq!(result.decoded_hash, reference["sourceHash"]);
                assert_eq!(result.seed_region.mask_hash, reference["seedHash"]);
                assert_eq!(result.tracking.output_hash, reference["outputHash"]);
                assert_eq!(std::fs::read(&raw_dest).unwrap(), masks);
                assert_eq!(
                    hash(&std::fs::read(&destination).unwrap()),
                    result.region_video.file_hash
                );
                assert_eq!(std::fs::read(&movie).unwrap(), original_movie);
                let decoded = Command::new(&ffmpeg)
                    .args(["-nostdin", "-v", "error", "-i"])
                    .arg(&destination)
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
                assert_eq!(decoded.stdout, masks);
                let probe = Command::new(&ffprobe)
                    .args(["-v", "error", "-show_streams", "-of", "json"])
                    .arg(&destination)
                    .output()
                    .unwrap();
                assert!(probe.status.success());
                let probe: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
                assert_eq!(probe["streams"].as_array().unwrap().len(), 1);
                assert_eq!(probe["streams"][0]["codec_type"], "video");
                assert_eq!(probe["streams"][0]["codec_name"], "ffv1");
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    for path in [&destination, &raw_dest] {
                        assert_eq!(
                            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                            0o600
                        );
                    }
                }
            } else {
                assert!(result.is_err(), "{geometry}/{mode}");
                if ["cancel", "changed", "late_occupied"].contains(&mode) {
                    assert!(acted, "{mode}");
                }
                if mode == "occupied" || mode == "late_occupied" {
                    assert_eq!(std::fs::read(&destination).unwrap(), b"foreign mask movie");
                } else {
                    assert!(!destination.exists(), "{mode}");
                }
                assert!(!raw_dest.exists(), "{mode}: raw publication leaked");
            }
            assert!(
                !std::fs::read_dir(root.path()).unwrap().any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".sam-video-")),
                "{geometry}/{mode}: owned staging leaked"
            );
        }
    }
}
