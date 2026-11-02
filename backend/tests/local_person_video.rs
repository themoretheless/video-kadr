use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::{
        local_person::Config,
        local_person_video::{self, Input, Tools},
    },
};
fn run(tool: &Path, args: &[&str]) -> Vec<u8> {
    let result = Command::new(tool).args(args).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}
#[tokio::test]
#[ignore = "requires native RVM/model/ORT, FFmpeg/FFprobe and original frame oracle"]
async fn actual_alpha_movie_audio_tracks_fractional_clock_and_failures() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        model: std::env::var("VIDEO_KADR_TEST_PERSON_MODEL")
            .unwrap()
            .into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let ffmpeg = PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFMPEG").unwrap());
    let ffprobe = PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFPROBE").unwrap());
    let oracle = PathBuf::from(std::env::var("VIDEO_KADR_TEST_PERSON_ORACLE_DIR").unwrap());
    let root = tempfile::tempdir().unwrap();
    let raw = root.path().join("input.rgba");
    std::fs::copy(oracle.join("input.rgba"), &raw).unwrap();
    let source = root.path().join("source.mov");
    let silent = root.path().join("silent.mov");
    let fractional = root.path().join("fractional.mov");
    run(
        &ffmpeg,
        &[
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
            "4",
            "-i",
            raw.to_str().unwrap(),
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=16000",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=16000",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-t",
            "1.5",
            "-c:v",
            "qtrle",
            "-pix_fmt",
            "argb",
            "-c:a:0",
            "pcm_s16le",
            "-c:a:1",
            "aac",
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
    run(
        &ffmpeg,
        &[
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
            "30000/1001",
            "-i",
            raw.to_str().unwrap(),
            "-c:v",
            "qtrle",
            "-pix_fmt",
            "argb",
            fractional.to_str().unwrap(),
        ],
    );
    let delayed = root.path().join("delayed.mov");
    let rotated = root.path().join("rotated.mov");
    run(
        &ffmpeg,
        &[
            "-nostdin",
            "-v",
            "error",
            "-copyts",
            "-itsoffset",
            "0.25",
            "-i",
            silent.to_str().unwrap(),
            "-i",
            source.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-map",
            "1:a",
            "-c",
            "copy",
            "-avoid_negative_ts",
            "disabled",
            delayed.to_str().unwrap(),
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
            source.to_str().unwrap(),
            "-map",
            "0",
            "-c",
            "copy",
            rotated.to_str().unwrap(),
        ],
    );
    let tagged = root.path().join("tagged.mov");
    run(
        &ffmpeg,
        &[
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
            "4",
            "-i",
            raw.to_str().unwrap(),
            "-vf",
            "pad=38:24",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-x264-params",
            "colorprim=bt709:transfer=bt709:colormatrix=bt709",
            tagged.to_str().unwrap(),
        ],
    );
    let tags: serde_json::Value = serde_json::from_slice(&run(
        &ffprobe,
        &[
            "-v",
            "error",
            "-show_streams",
            "-of",
            "json",
            tagged.to_str().unwrap(),
        ],
    ))
    .unwrap();
    assert_eq!(tags["streams"][0]["color_transfer"], "bt709");
    assert_eq!(tags["streams"][0]["color_primaries"], "bt709");
    let runtime = ProcessRuntime::local_default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    for path in [&source, &silent, &fractional, &delayed, &rotated, &tagged] {
        eprintln!(
            "person movie fixture {}",
            path.file_name().unwrap().to_string_lossy()
        );
        let hash = video_kadr_backend::services::local_speech::fingerprint_model(path)
            .await
            .unwrap()
            .1;
        let output = path.with_file_name(format!(
            "cut-{}",
            path.file_name().unwrap().to_str().unwrap()
        ));
        let masks = output.with_extension("masks");
        let result = local_person_video::segment(
            &runtime,
            Tools {
                ffmpeg: &ffmpeg,
                ffprobe: &ffprobe,
            },
            &config,
            Input {
                source: path,
                source_hash: &hash,
                staging_bytes: 64 * 1024 * 1024 * 1024,
                mask_destination: Some(&masks),
            },
            &output,
            &CancellationToken::new(),
            Duration::from_secs(60),
            &tx,
        )
        .await
        .unwrap();
        assert_eq!(
            (result.width, result.height, result.frames),
            if path == &tagged {
                (38, 24, 4)
            } else if path == &rotated {
                (23, 37, 4)
            } else {
                (37, 23, 4)
            }
        );
        assert_eq!(result.codec, "qtrle");
        assert_eq!(
            result.audio_tracks,
            if path == &source || path == &delayed || path == &rotated {
                2
            } else {
                0
            }
        );
        if path == &fractional {
            assert_eq!(
                (result.frame_rate.numerator, result.frame_rate.denominator),
                (30000, 1001)
            );
        }
        let decoded = run(
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
                "-fps_mode",
                "passthrough",
                "-pix_fmt",
                "rgba",
                "-f",
                "rawvideo",
                "-",
            ],
        );
        if path != &rotated && path != &tagged {
            let expected = std::fs::read(oracle.join("reference.rgba")).unwrap();
            assert_eq!(decoded.len(), expected.len());
            assert!(
                decoded == expected,
                "first RGBA mismatch {:?}",
                decoded.iter().zip(&expected).position(|(a, b)| a != b)
            );
            assert_eq!(
                std::fs::read(&masks).unwrap(),
                std::fs::read(oracle.join("reference.masks")).unwrap()
            );
        } else {
            assert_eq!(
                decoded.len(),
                result.width as usize * result.height as usize * 4 * 4
            );
            assert_eq!(std::fs::metadata(&masks).unwrap().len(), 320 * 512 * 4);
        }

        let original_masks = std::fs::read(&masks).unwrap();
        for erase in [false, true] {
            let corrected = path.with_file_name(format!(
                "corrected-{erase}-{}",
                path.file_name().unwrap().to_str().unwrap()
            ));
            let corrected_masks = corrected.with_extension("masks");
            let points: Vec<[f64; 2]> = (0..9)
                .flat_map(|y| (0..9).map(move |x| [x as f64 / 8., y as f64 / 8.]))
                .collect();
            let edits: Vec<video_kadr_backend::services::local_person_correct::FrameCorrection> =
                if erase {
                    vec![serde_json::from_value(serde_json::json!({"frame":1,"correction":{"custom":true,"strokes":[{"tool":"eraser","size":0.5,"points":points}],"feather":0}})).unwrap()]
                } else {
                    vec![]
                };
            let metadata = local_person_video::correct(
                &runtime,
                Tools {
                    ffmpeg: &ffmpeg,
                    ffprobe: &ffprobe,
                },
                &config.executable,
                local_person_video::CorrectionInput {
                    video: Input {
                        source: path,
                        source_hash: &hash,
                        staging_bytes: 64 * 1024 * 1024 * 1024,
                        mask_destination: Some(&corrected_masks),
                    },
                    masks: &masks,
                    mask_hash: &result.mask_hash,
                    corrections: &edits,
                },
                &corrected,
                &CancellationToken::new(),
                Duration::from_secs(60),
                &tx,
            )
            .await
            .unwrap();
            assert_eq!(metadata.audio_tracks, result.audio_tracks);
            assert_eq!(metadata.frame_rate, result.frame_rate);
            assert_ne!(metadata.engine_hash, result.engine_hash);
            assert_eq!(
                metadata.correction.unwrap().source_mask_hash,
                result.mask_hash
            );
            let got = run(
                &ffmpeg,
                &[
                    "-nostdin",
                    "-v",
                    "error",
                    "-i",
                    corrected.to_str().unwrap(),
                    "-map",
                    "0:v:0",
                    "-an",
                    "-fps_mode",
                    "passthrough",
                    "-pix_fmt",
                    "rgba",
                    "-f",
                    "rawvideo",
                    "-",
                ],
            );
            let mut expected = decoded.clone();
            let mut expected_masks = original_masks.clone();
            if erase {
                let frame = result.width as usize * result.height as usize * 4;
                for i in (frame + 3..2 * frame).step_by(4) {
                    expected[i] = 0;
                }
                let frame = result.mask_width as usize * result.mask_height as usize;
                expected_masks[frame..2 * frame].fill(0);
            }
            assert_eq!(got.len(), expected.len());
            assert!(
                got == expected,
                "corrected RGBA differs in {erase} mode for {path:?}"
            );
            assert_eq!(std::fs::read(corrected_masks).unwrap(), expected_masks);
        }

        if path == &source {
            for mutate in [false, true] {
                let denied = root.path().join(format!("denied-cached-{mutate}.mov"));
                let denied_masks = denied.with_extension("masks");
                let (updates, mut values) = tokio::sync::mpsc::unbounded_channel();
                let original_path = masks.clone();
                let observer = tokio::spawn(async move {
                    let mut changed = false;
                    while let Some(value) = values.recv().await {
                        if mutate && value > 10. && !changed {
                            std::fs::write(&original_path, b"changed retained masks").unwrap();
                            changed = true;
                        }
                    }
                    changed
                });
                let bad_hash = "0".repeat(64);
                let failure = local_person_video::correct(
                    &runtime,
                    Tools {
                        ffmpeg: &ffmpeg,
                        ffprobe: &ffprobe,
                    },
                    &config.executable,
                    local_person_video::CorrectionInput {
                        video: Input {
                            source: path,
                            source_hash: &hash,
                            staging_bytes: 64 * 1024 * 1024 * 1024,
                            mask_destination: Some(&denied_masks),
                        },
                        masks: &masks,
                        mask_hash: if mutate { &result.mask_hash } else { &bad_hash },
                        corrections: &[],
                    },
                    &denied,
                    &CancellationToken::new(),
                    Duration::from_secs(60),
                    &updates,
                )
                .await;
                drop(updates);
                assert_eq!(observer.await.unwrap(), mutate);
                assert!(
                    failure.is_err(),
                    "stale or changed retained mask must refuse"
                );
                assert!(!denied.exists() && !denied_masks.exists());
                std::fs::write(&masks, &original_masks).unwrap();
            }
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(local_person_video::segment(
            &runtime,
            Tools {
                ffmpeg: &ffmpeg,
                ffprobe: &ffprobe
            },
            &config,
            Input {
                source: path,
                source_hash: &hash,
                staging_bytes: 64 * 1024 * 1024 * 1024,
                mask_destination: None
            },
            &output,
            &CancellationToken::new(),
            Duration::from_secs(60),
            &tx
        )
        .await
        .is_err());
    }
    let mut progress = vec![];
    while let Ok(value) = rx.try_recv() {
        progress.push(value);
    }
    assert_eq!(progress.last(), Some(&100.));
    let original = std::fs::read(&source).unwrap();
    let hash = video_kadr_backend::services::local_speech::fingerprint_model(&source)
        .await
        .unwrap()
        .1;
    for mode in [
        "cancel",
        "changed",
        "occupied",
        "mask_occupied",
        "deadline",
        "wrong_hash",
        "budget",
    ] {
        let output = root.path().join(format!("{mode}.mov"));
        let mask = output.with_extension("masks");
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let path = source.clone();
        let target = output.clone();
        let target_mask = mask.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let observer = tokio::spawn(async move {
            while let Some(value) = rx.recv().await {
                if value > 10. {
                    match mode {
                        "cancel" => token.cancel(),
                        "changed" => std::fs::write(path, b"changed").unwrap(),
                        "occupied" => std::fs::write(target, b"occupied").unwrap(),
                        "mask_occupied" => std::fs::write(target_mask, b"occupied mask").unwrap(),
                        _ => {}
                    }
                    break;
                }
            }
        });
        let result = local_person_video::segment(
            &runtime,
            Tools {
                ffmpeg: &ffmpeg,
                ffprobe: &ffprobe,
            },
            &config,
            Input {
                source: &source,
                source_hash: if mode == "wrong_hash" {
                    "invalid"
                } else {
                    &hash
                },
                staging_bytes: if mode == "budget" {
                    1
                } else {
                    64 * 1024 * 1024 * 1024
                },
                mask_destination: Some(&mask),
            },
            &output,
            &cancel,
            if mode == "deadline" {
                Duration::from_millis(1)
            } else {
                Duration::from_secs(60)
            },
            &tx,
        )
        .await;
        drop(tx);
        observer.await.unwrap();
        assert!(result.is_err(), "{mode} must refuse");
        if mode == "occupied" {
            assert_eq!(std::fs::read(&output).unwrap(), b"occupied");
        } else {
            assert!(!output.exists());
        }
        if mode == "mask_occupied" {
            assert_eq!(std::fs::read(&mask).unwrap(), b"occupied mask");
        } else {
            assert!(!mask.exists());
        }
        std::fs::write(&source, &original).unwrap();
    }
    for (hdr, name) in [(false, "deep"), (true, "hdr")] {
        let path = root
            .path()
            .join(format!("{name}.{}", if hdr { "mov" } else { "mkv" }));
        let mut args = vec![
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
            "4",
            "-i",
            raw.to_str().unwrap(),
        ];
        if hdr {
            args.extend([
                "-vf",
                "pad=38:24",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-x264-params",
                "colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc",
            ]);
        } else {
            args.extend(["-c:v", "ffv1", "-pix_fmt", "gbrap16le"]);
        }
        args.push(path.to_str().unwrap());
        run(&ffmpeg, &args);
        let hash = video_kadr_backend::services::local_speech::fingerprint_model(&path)
            .await
            .unwrap()
            .1;
        let output = root.path().join(format!("{name}-cut.mov"));
        let result = local_person_video::segment(
            &runtime,
            Tools {
                ffmpeg: &ffmpeg,
                ffprobe: &ffprobe,
            },
            &config,
            Input {
                source: &path,
                source_hash: &hash,
                staging_bytes: 64 * 1024 * 1024 * 1024,
                mask_destination: None,
            },
            &output,
            &CancellationToken::new(),
            Duration::from_secs(60),
            &tx,
        )
        .await;
        let error = result.unwrap_err();
        assert!(
            error
                .to_string()
                .contains(if hdr { "SDR" } else { "8 bits" }),
            "{name}: {error:?}"
        );
        assert!(!output.exists());
    }
    let vfr = root.path().join("variable.mov");
    run(
        &ffmpeg,
        &[
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
            "4",
            "-i",
            raw.to_str().unwrap(),
            "-vf",
            "settb=1/1000,setpts=N*N*62.5",
            "-fps_mode",
            "vfr",
            "-enc_time_base",
            "1:1000",
            "-video_track_timescale",
            "1000",
            "-c:v",
            "qtrle",
            "-pix_fmt",
            "argb",
            vfr.to_str().unwrap(),
        ],
    );
    let hash = video_kadr_backend::services::local_speech::fingerprint_model(&vfr)
        .await
        .unwrap()
        .1;
    let output = root.path().join("vfr-cut.mov");
    let result = local_person_video::segment(
        &runtime,
        Tools {
            ffmpeg: &ffmpeg,
            ffprobe: &ffprobe,
        },
        &config,
        Input {
            source: &vfr,
            source_hash: &hash,
            staging_bytes: 64 * 1024 * 1024 * 1024,
            mask_destination: None,
        },
        &output,
        &CancellationToken::new(),
        Duration::from_secs(60),
        &tx,
    )
    .await;
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("variable frame timing"));
    assert!(!output.exists());
    assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".person-video-")));
}
