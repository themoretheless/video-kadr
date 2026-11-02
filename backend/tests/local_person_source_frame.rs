use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::local_person_source_frame::{self, Input},
};
#[tokio::test]
#[ignore = "requires host FFmpeg/FFprobe and independent source RGBA sequence"]
async fn actual_original_source_ordinals_fractional_rotation_and_refusals() {
    let ffmpeg = PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFMPEG").unwrap());
    let ffprobe = PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFPROBE").unwrap());
    let oracle = PathBuf::from(std::env::var("VIDEO_KADR_TEST_PERSON_ORACLE_DIR").unwrap());
    let rgba = std::fs::read(oracle.join("input.rgba")).unwrap();
    let size = 37 * 23 * 4;
    let root = tempfile::tempdir().unwrap();
    for mode in ["whole", "fractional", "rotation"] {
        let original = root.path().join(format!("{mode}.mov"));
        let output = std::process::Command::new(&ffmpeg)
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                "37x23",
                "-framerate",
                if mode == "fractional" {
                    "30000/1001"
                } else {
                    "4"
                },
                "-i",
            ])
            .arg(oracle.join("input.rgba"))
            .args(["-c:v", "qtrle", "-pix_fmt", "argb", "-an"])
            .arg(&original)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let movie = if mode == "rotation" {
            let rotated = root.path().join("rotated.mov");
            let output = std::process::Command::new(&ffmpeg)
                .args(["-nostdin", "-v", "error", "-display_rotation", "90", "-i"])
                .arg(&original)
                .args(["-c", "copy"])
                .arg(&rotated)
                .output()
                .unwrap();
            assert!(output.status.success());
            rotated
        } else {
            original
        };
        let hash = format!("{:x}", Sha256::digest(std::fs::read(&movie).unwrap()));
        let (width, height) = if mode == "rotation" {
            (23, 37)
        } else {
            (37, 23)
        };
        for frame in 0..4 {
            let png = local_person_source_frame::png(
                &ProcessRuntime::local_default(),
                local_person_source_frame::Tools {
                    ffmpeg: &ffmpeg,
                    ffprobe: &ffprobe,
                },
                Input {
                    source: &movie,
                    source_hash: &hash,
                    width,
                    height,
                    frames: 4,
                    frame,
                },
                &CancellationToken::new(),
                Duration::from_secs(15),
            )
            .await
            .unwrap();
            let decoded = image::load_from_memory(&png).unwrap().into_rgba8();
            assert_eq!(decoded.dimensions(), (width, height));
            let expected = &rgba[frame as usize * size..(frame as usize + 1) * size];
            let expected = if mode == "rotation" {
                image::imageops::rotate270(
                    &image::RgbaImage::from_raw(37, 23, expected.to_vec()).unwrap(),
                )
                .into_raw()
            } else {
                expected.to_vec()
            };
            assert_eq!(decoded.into_raw(), expected, "{mode}/{frame}");
        }
        for refusal in ["hash", "frame", "cancel", "deadline", "shape", "transpose"] {
            let cancel = CancellationToken::new();
            if refusal == "cancel" {
                cancel.cancel();
            }
            let bad = "0".repeat(64);
            let result = local_person_source_frame::png(
                &ProcessRuntime::local_default(),
                local_person_source_frame::Tools {
                    ffmpeg: &ffmpeg,
                    ffprobe: &ffprobe,
                },
                Input {
                    source: &movie,
                    source_hash: if refusal == "hash" { &bad } else { &hash },
                    width: if refusal == "shape" {
                        0
                    } else if refusal == "transpose" {
                        height
                    } else {
                        width
                    },
                    height: if refusal == "transpose" {
                        width
                    } else {
                        height
                    },
                    frames: 4,
                    frame: if refusal == "frame" { 4 } else { 0 },
                },
                &cancel,
                if refusal == "deadline" {
                    Duration::ZERO
                } else {
                    Duration::from_secs(15)
                },
            )
            .await;
            assert!(result.is_err(), "{mode}/{refusal}");
        }
    }
}
