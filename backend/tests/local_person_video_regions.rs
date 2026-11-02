use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    domain::video_region::Attachment,
    process_control::ProcessRuntime,
    services::{
        local_person_correct::FrameCorrection,
        local_person_video::{CorrectionInput, Input, Tools},
        local_person_video_regions::{self, EncodedRegion},
    },
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn run(tool: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new(tool).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
#[tokio::test]
#[ignore = "requires native vision binary, host FFmpeg/FFprobe and independent tracked replay fixtures; no models/ORT"]
async fn actual_encoded_regions_alpha_movie_audio_clock_and_atomic_failures() {
    let binary = PathBuf::from(std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap());
    let ffmpeg = PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFMPEG").unwrap());
    let ffprobe = PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFPROBE").unwrap());
    let oracle = PathBuf::from(std::env::var("VIDEO_KADR_TEST_PERSON_REGIONS_ORACLE_DIR").unwrap());
    for geometry in ["37x23", "23x37", "80x80"] {
        let fixture = oracle.join(geometry);
        let request: Value =
            serde_json::from_slice(&std::fs::read(fixture.join("valid.json")).unwrap()).unwrap();
        let source = std::fs::read(request["input"].as_str().unwrap()).unwrap();
        let auto = std::fs::read(request["mask"].as_str().unwrap()).unwrap();
        let regions = std::fs::read(request["regions"][0]["input"].as_str().unwrap()).unwrap();
        let width = request["width"].as_u64().unwrap() as u32;
        let height = request["height"].as_u64().unwrap() as u32;
        let modes = if geometry == "37x23" {
            vec![
                "whole",
                "fractional",
                "rotation",
                "delay",
                "cancel",
                "source",
                "region",
                "mask",
                "executable",
                "occupied",
                "occupied-mask",
                "late-output",
                "late-mask",
                "budget",
                "deadline",
                "clock",
                "shape",
                "stale",
                "plane",
                "stream",
            ]
        } else {
            vec!["whole"]
        };
        for mode in modes {
            eprintln!("tracked movie {geometry}/{mode}");
            let root = tempfile::tempdir().unwrap();
            let source_raw = root.path().join("source.rgba");
            let region_raw = root.path().join("region.gray");
            let masks = root.path().join("auto.gray");
            let movie = root.path().join("source.mov");
            let encoded = root.path().join("region.mkv");
            let output = root.path().join("out.mov");
            let mask_output = root.path().join("out.gray");
            let (coded_w, coded_h) = if mode == "rotation" {
                (height, width)
            } else {
                (width, height)
            };
            let coded = if mode == "rotation" {
                source
                    .chunks_exact((width * height * 4) as usize)
                    .flat_map(|frame| {
                        image::imageops::rotate90(
                            &image::RgbaImage::from_raw(width, height, frame.to_vec()).unwrap(),
                        )
                        .into_raw()
                    })
                    .collect::<Vec<_>>()
            } else {
                source.clone()
            };
            std::fs::write(&source_raw, coded).unwrap();
            std::fs::write(&region_raw, &regions).unwrap();
            std::fs::write(&masks, &auto).unwrap();
            let rate = if mode == "fractional" {
                "30000/1001"
            } else {
                "4"
            };
            let size = format!("{coded_w}x{coded_h}");
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
                source_raw.to_str().unwrap(),
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=733:sample_rate=48000:duration=1",
                "-c:a",
                "pcm_s16le",
                "-shortest",
                "-c:v",
                "qtrle",
                "-pix_fmt",
                "argb",
            ];
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
            let region_rate = if mode == "clock" { "5" } else { rate };
            run(
                &ffmpeg,
                &[
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
                    region_rate,
                    "-i",
                    region_raw.to_str().unwrap(),
                    "-c:v",
                    "ffv1",
                    "-pix_fmt",
                    "gray",
                    encoded.to_str().unwrap(),
                ],
            );
            let original_movie = std::fs::read(&movie).unwrap();
            let movie_hash = hash(&original_movie);
            let region_hash = hash(&std::fs::read(&encoded).unwrap());
            let mut edits: Vec<FrameCorrection> =
                serde_json::from_value(request["corrections"].clone()).unwrap();
            for edit in &mut edits {
                for stroke in &mut edit.correction.strokes {
                    let region = stroke.region.as_mut().unwrap();
                    region.source_hash = movie_hash.clone();
                    region.mask_file_hash = region_hash.clone();
                }
            }
            if mode == "plane" {
                edits[0].correction.strokes[0]
                    .region
                    .as_mut()
                    .unwrap()
                    .mask_hash = "0".repeat(64);
            }
            let mut snapshot = request["regions"][0]["attachment"].clone();
            snapshot["sourceHash"] = json!(movie_hash);
            snapshot["maskFileHash"] = json!(region_hash);
            snapshot["width"] = json!(if mode == "shape" { width + 1 } else { width });
            snapshot["height"] = json!(height);
            snapshot["frames"] = json!(4);
            snapshot["frameRate"] = json!({"numerator":if mode=="clock"{5}else if mode=="fractional"{30000}else{4},"denominator":if mode=="fractional"{1001}else{1}});
            snapshot["seedFrame"] = json!(1);
            snapshot["trackedFrames"] = json!(4);
            snapshot["forwardLostAt"] = Value::Null;
            snapshot["backwardLostAt"] = Value::Null;
            if mode == "stream" {
                snapshot["maskHash"] = json!("0".repeat(64));
            }
            let attachment: Attachment = serde_json::from_value(snapshot).unwrap();
            if mode == "occupied" {
                std::fs::write(&output, b"foreign").unwrap();
            }
            if mode == "occupied-mask" {
                std::fs::write(&mask_output, b"foreign").unwrap();
            }
            let executable = if mode == "executable" {
                let path = root.path().join("vision");
                std::fs::copy(&binary, &path).unwrap();
                path
            } else {
                binary.clone()
            };
            let cancel = CancellationToken::new();
            let trigger = cancel.clone();
            let sp = movie.clone();
            let rp = encoded.clone();
            let mp = masks.clone();
            let ep = executable.clone();
            let op = output.clone();
            let mop = mask_output.clone();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<f64>();
            let observer = tokio::spawn(async move {
                let mut max = 0f64;
                let mut acted = false;
                while let Some(value) = rx.recv().await {
                    assert!(value >= max, "{geometry}/{mode}: {value} < {max}");
                    max = value;
                    if value > 20. && !acted {
                        acted = true;
                        match mode {
                            "cancel" => trigger.cancel(),
                            "source" => std::fs::write(&sp, b"changed original movie").unwrap(),
                            "region" => {
                                std::fs::write(&rp, b"changed encoded region after restore")
                                    .unwrap()
                            }
                            "mask" => std::fs::write(&mp, b"changed automatic masks").unwrap(),
                            "executable" => {
                                let temp = ep.with_extension("replacement");
                                std::fs::copy(&ep, &temp).unwrap();
                                use std::io::Write;
                                std::fs::OpenOptions::new()
                                    .append(true)
                                    .open(&temp)
                                    .unwrap()
                                    .write_all(b"changed")
                                    .unwrap();
                                std::fs::rename(temp, &ep).unwrap();
                            }
                            "late-output" => std::fs::write(&op, b"foreign").unwrap(),
                            "late-mask" => std::fs::write(&mop, b"foreign").unwrap(),
                            _ => {}
                        }
                    }
                }
                (max, acted)
            });
            let source_hash = if mode == "stale" {
                "0".repeat(64)
            } else {
                movie_hash.clone()
            };
            let streams = [EncodedRegion {
                video: &encoded,
                attachment: &attachment,
            }];
            let result = local_person_video_regions::correct(
                &ProcessRuntime::local_default(),
                Tools {
                    ffmpeg: &ffmpeg,
                    ffprobe: &ffprobe,
                },
                &executable,
                CorrectionInput {
                    video: Input {
                        source: &movie,
                        source_hash: &source_hash,
                        staging_bytes: if mode == "budget" {
                            1
                        } else {
                            64 * 1024 * 1024 * 1024
                        },
                        mask_destination: Some(&mask_output),
                    },
                    masks: &masks,
                    mask_hash: &hash(&auto),
                    corrections: &edits,
                },
                &streams,
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
            let (max, acted) = observer.await.unwrap();
            if ["whole", "fractional", "rotation", "delay"].contains(&mode) {
                let metadata = result.unwrap_or_else(|e| panic!("{geometry}/{mode}: {e:#}"));
                assert_eq!(
                    (metadata.width, metadata.height, metadata.frames),
                    (width, height, 4)
                );
                assert_eq!(metadata.frame_rate, attachment.frame_rate);
                assert!(metadata.audio_copied);
                assert_eq!(metadata.audio_tracks, 1);
                assert_eq!(
                    metadata
                        .correction
                        .as_ref()
                        .unwrap()
                        .region_hashes
                        .get("tracked"),
                    Some(&hash(&regions))
                );
                assert_eq!(max, 100.);
                assert_eq!(std::fs::read(&movie).unwrap(), original_movie);
                assert_eq!(
                    std::fs::read(&mask_output).unwrap(),
                    std::fs::read(fixture.join("expected.gray")).unwrap()
                );
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
                        "pipe:1",
                    ],
                );
                assert_eq!(
                    decoded,
                    std::fs::read(fixture.join("expected.rgba")).unwrap(),
                    "{geometry}/{mode}"
                );
                let audio = |path: &Path| {
                    run(
                        &ffmpeg,
                        &[
                            "-nostdin",
                            "-v",
                            "error",
                            "-i",
                            path.to_str().unwrap(),
                            "-map",
                            "0:a:0",
                            "-vn",
                            "-f",
                            "s16le",
                            "-acodec",
                            "pcm_s16le",
                            "pipe:1",
                        ],
                    )
                };
                assert_eq!(audio(&movie), audio(&output));
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    assert_eq!(
                        std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                        0o600
                    );
                    assert_eq!(
                        std::fs::metadata(&mask_output)
                            .unwrap()
                            .permissions()
                            .mode()
                            & 0o777,
                        0o600
                    );
                }
            } else {
                assert!(result.is_err(), "{geometry}/{mode}");
                if [
                    "cancel",
                    "source",
                    "region",
                    "mask",
                    "executable",
                    "late-output",
                    "late-mask",
                ]
                .contains(&mode)
                {
                    assert!(acted, "{mode}");
                }
                if ["occupied", "late-output"].contains(&mode) {
                    assert_eq!(std::fs::read(&output).unwrap(), b"foreign");
                } else {
                    assert!(!output.exists(), "{mode}");
                }
                if ["occupied-mask", "late-mask"].contains(&mode) {
                    assert_eq!(std::fs::read(&mask_output).unwrap(), b"foreign");
                } else {
                    assert!(!mask_output.exists(), "{mode}");
                }
            }
            assert!(
                std::fs::read_dir(root.path()).unwrap().all(|e| !e
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".person-")),
                "{mode}"
            );
        }
    }
}
