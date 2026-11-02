use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    domain::composition::FrameRateRatio,
    process_control::ProcessRuntime,
    services::{
        local_person_mask_video::Metadata,
        local_region_frame::{self, Tools},
    },
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[tokio::test]
#[ignore = "requires host FFmpeg/FFprobe and retained region streams; no inference models"]
async fn exact_region_planes_png_sha_fractional_clock_and_refusals() {
    let ffmpeg = PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFMPEG").unwrap());
    let ffprobe = PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFPROBE").unwrap());
    let oracle = PathBuf::from(std::env::var("VIDEO_KADR_TEST_PERSON_REGIONS_ORACLE_DIR").unwrap());
    let runtime = ProcessRuntime::local_default();
    for geometry in ["37x23", "23x37", "80x80"] {
        let request: Value = serde_json::from_slice(
            &std::fs::read(oracle.join(geometry).join("valid.json")).unwrap(),
        )
        .unwrap();
        let input = PathBuf::from(request["regions"][0]["input"].as_str().unwrap());
        let raw = std::fs::read(&input).unwrap();
        for (numerator, denominator) in [(4, 1), (30000, 1001)] {
            let root = tempfile::tempdir().unwrap();
            let movie = root.path().join("regions.mkv");
            let rate = format!("{numerator}/{denominator}");
            let output = std::process::Command::new(&ffmpeg)
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-f",
                    "rawvideo",
                    "-pixel_format",
                    "gray",
                    "-video_size",
                    "256x256",
                    "-framerate",
                    &rate,
                    "-i",
                    input.to_str().unwrap(),
                    "-c:v",
                    "ffv1",
                    "-pix_fmt",
                    "gray",
                    movie.to_str().unwrap(),
                ])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let bytes = std::fs::read(&movie).unwrap();
            let metadata = Metadata {
                width: 256,
                height: 256,
                frames: 4,
                frame_rate: FrameRateRatio {
                    numerator,
                    denominator,
                },
                mask_hash: hash(&raw),
                file_hash: hash(&bytes),
                size_bytes: bytes.len() as u64,
            };
            for frame in 0..4u64 {
                let result = local_region_frame::png(
                    &runtime,
                    Tools {
                        ffmpeg: &ffmpeg,
                        ffprobe: &ffprobe,
                    },
                    &movie,
                    &metadata,
                    frame,
                    &CancellationToken::new(),
                    Duration::from_secs(15),
                )
                .await
                .unwrap();
                let plane = &raw[frame as usize * 65536..(frame as usize + 1) * 65536];
                assert_eq!(result.mask_hash, hash(plane));
                assert_eq!(
                    image::load_from_memory(&result.png)
                        .unwrap()
                        .to_luma8()
                        .as_raw(),
                    plane
                );
            }
            if geometry == "37x23" && numerator == 4 {
                for mode in [
                    "ordinal", "hash", "size", "clock", "count", "shape", "cancel", "deadline",
                    "corrupt",
                ] {
                    let mut expected = metadata.clone();
                    let cancel = CancellationToken::new();
                    if mode == "cancel" {
                        cancel.cancel();
                    }
                    match mode {
                        "hash" => expected.file_hash = "0".repeat(64),
                        "size" => expected.size_bytes += 1,
                        "clock" => expected.frame_rate.numerator = 5,
                        "count" => expected.frames = 5,
                        "shape" => expected.width = 128,
                        "corrupt" => std::fs::write(&movie, b"changed").unwrap(),
                        _ => {}
                    }
                    assert!(
                        local_region_frame::png(
                            &runtime,
                            Tools {
                                ffmpeg: &ffmpeg,
                                ffprobe: &ffprobe
                            },
                            &movie,
                            &expected,
                            if mode == "ordinal" { 4 } else { 0 },
                            &cancel,
                            if mode == "deadline" {
                                Duration::ZERO
                            } else {
                                Duration::from_secs(15)
                            }
                        )
                        .await
                        .is_err(),
                        "{mode}"
                    );
                    if mode == "corrupt" {
                        std::fs::write(&movie, &bytes).unwrap();
                    }
                }
            }
            assert_eq!(std::fs::read(&movie).unwrap(), bytes);
        }
    }
}
