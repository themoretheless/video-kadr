use std::{path::Path, process::Command, time::Duration};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::{
        local_enhance::Config,
        local_enhance_video::{self, Input, Tools},
    },
};
fn run(program: &Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new(program).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
#[tokio::test]
#[ignore = "requires native learned enhancer, public model, ONNX Runtime, FFmpeg and FFprobe"]
async fn actual_whole_video_original_audio_no_audio_cancel_and_no_replace() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        model: std::env::var("VIDEO_KADR_TEST_ENHANCE_MODEL")
            .unwrap()
            .into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let ffmpeg = std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFMPEG").unwrap());
    let ffprobe = std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFPROBE").unwrap());
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.mov");
    let silent = root.path().join("silent.mov");
    run(
        &ffmpeg,
        &[
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=16x24:rate=2",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-t",
            "1",
            "-c:v",
            "qtrle",
            "-pix_fmt",
            "argb",
            "-c:a",
            "aac",
            "-b:a",
            "64k",
            source.to_str().unwrap(),
        ],
    );
    run(
        &ffmpeg,
        &[
            "-nostdin",
            "-v",
            "error",
            "-i",
            source.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-c",
            "copy",
            silent.to_str().unwrap(),
        ],
    );
    let fractional = root.path().join("fractional.mov");
    let rotated = root.path().join("rotated.mov");
    run(
        &ffmpeg,
        &[
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=16x24:rate=30000/1001",
            "-frames:v",
            "3",
            "-c:v",
            "qtrle",
            "-pix_fmt",
            "argb",
            fractional.to_str().unwrap(),
        ],
    );
    run(
        &ffmpeg,
        &[
            "-nostdin",
            "-v",
            "error",
            "-display_rotation",
            "90",
            "-i",
            silent.to_str().unwrap(),
            "-c",
            "copy",
            rotated.to_str().unwrap(),
        ],
    );
    let rotation: serde_json::Value = serde_json::from_slice(&run(
        &ffprobe,
        &[
            "-v",
            "error",
            "-show_streams",
            "-of",
            "json",
            rotated.to_str().unwrap(),
        ],
    ))
    .unwrap();
    assert_eq!(
        rotation["streams"][0]["side_data_list"][0]["rotation"], 90,
        "rotation fixture must contain a real display matrix"
    );
    let original_audio = run(
        &ffmpeg,
        &[
            "-nostdin",
            "-v",
            "error",
            "-i",
            source.to_str().unwrap(),
            "-map",
            "0:a:0",
            "-c",
            "copy",
            "-f",
            "data",
            "pipe:1",
        ],
    );
    assert!(!original_audio.is_empty());
    let runtime = ProcessRuntime::local_default();
    for (index, input) in [&source, &silent, &fractional, &rotated]
        .into_iter()
        .enumerate()
    {
        let hash = video_kadr_backend::services::local_speech::fingerprint_model(input)
            .await
            .unwrap()
            .1;
        let output = root.path().join(format!("enhanced-{index}.mp4"));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let meta = local_enhance_video::enhance(
            &runtime,
            Tools {
                ffmpeg: &ffmpeg,
                ffprobe: &ffprobe,
            },
            &config,
            Input {
                source: input,
                source_hash: &hash,
                factor: 2,
                staging_bytes: 16 * 1024 * 1024,
            },
            &output,
            &CancellationToken::new(),
            Duration::from_secs(120),
            &tx,
        )
        .await
        .unwrap();
        assert_eq!(
            (meta.width, meta.height, meta.frames, meta.source_frames),
            if index == 2 {
                (32, 48, 3, 3)
            } else if index == 3 {
                (48, 32, 2, 2)
            } else {
                (32, 48, 2, 2)
            }
        );
        assert_eq!(
            (meta.frame_rate.numerator, meta.frame_rate.denominator),
            if index == 2 { (30000, 1001) } else { (2, 1) }
        );
        assert_eq!(meta.audio_copied, index == 0);
        assert_eq!(
            meta.audio_codec.as_deref(),
            if index == 0 { Some("aac") } else { None }
        );
        let mut progress = vec![];
        while let Ok(v) = rx.try_recv() {
            progress.push(v);
        }
        assert_eq!(progress.last(), Some(&100.));
        assert!(progress.iter().any(|v| *v > 10. && *v < 85.));
        assert!(progress.windows(2).all(|v| v[0] <= v[1]), "{progress:?}");
        if index == 0 {
            let audio = run(
                &ffmpeg,
                &[
                    "-nostdin",
                    "-v",
                    "error",
                    "-i",
                    output.to_str().unwrap(),
                    "-map",
                    "0:a:0",
                    "-c",
                    "copy",
                    "-f",
                    "data",
                    "pipe:1",
                ],
            );
            assert_eq!(
                audio, original_audio,
                "compressed audio packets must be copied unchanged"
            );
        }
        let pixels = run(
            &ffmpeg,
            &[
                "-nostdin",
                "-v",
                "error",
                "-i",
                output.to_str().unwrap(),
                "-map",
                "0:v:0",
                "-an",
                "-pix_fmt",
                "rgba",
                "-f",
                "rawvideo",
                "pipe:1",
            ],
        );
        assert_eq!(
            pixels.len(),
            meta.width as usize * meta.height as usize * 4 * meta.frames as usize
        );
        assert!(pixels.chunks_exact(4).all(|p| p[3] == 255));
        let bytes = std::fs::read(&output).unwrap();
        assert!(local_enhance_video::enhance(
            &runtime,
            Tools {
                ffmpeg: &ffmpeg,
                ffprobe: &ffprobe
            },
            &config,
            Input {
                source: input,
                source_hash: &hash,
                factor: 2,
                staging_bytes: 16 * 1024 * 1024
            },
            &output,
            &CancellationToken::new(),
            Duration::from_secs(120),
            &tx
        )
        .await
        .is_err());
        assert_eq!(std::fs::read(&output).unwrap(), bytes);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    let hash = video_kadr_backend::services::local_speech::fingerprint_model(&source)
        .await
        .unwrap()
        .1;
    let original_source = std::fs::read(&source).unwrap();
    for mode in ["cancel", "changed", "deadline", "wrong_hash", "budget"] {
        let destination = root.path().join(format!("{mode}.mp4"));
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let source_path = source.clone();
        let observer = tokio::spawn(async move {
            while let Some(progress) = rx.recv().await {
                if mode == "changed" && progress > 10. {
                    use std::io::Write;
                    std::fs::OpenOptions::new()
                        .append(true)
                        .open(&source_path)
                        .unwrap()
                        .write_all(b"changed after admission")
                        .unwrap();
                    break;
                }

                if mode == "cancel" && progress > 10. {
                    token.cancel();
                    break;
                }
            }
        });
        let result = local_enhance_video::enhance(
            &runtime,
            Tools {
                ffmpeg: &ffmpeg,
                ffprobe: &ffprobe,
            },
            &config,
            Input {
                source: &source,
                source_hash: if mode == "wrong_hash" { "bad" } else { &hash },
                factor: 2,
                staging_bytes: if mode == "budget" {
                    1
                } else {
                    16 * 1024 * 1024
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
        observer.await.unwrap();
        assert!(result.is_err(), "{mode} must refuse");
        assert!(!destination.exists(), "{mode} must not publish");
        if mode == "changed" {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("video source or engine changed"));
        }
        std::fs::write(&source, &original_source).unwrap();
    }
    assert!(std::fs::read_dir(root.path()).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".enhance-video-")));
}
