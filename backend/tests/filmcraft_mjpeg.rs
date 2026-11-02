#[path = "source_oracles/filmcraft_mjpeg.rs"]
mod original;
mod support;
use filmcraft_isobmff::TrackKind;
use std::{path::Path, time::Duration};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    domain::{
        composition::FrameRateRatio,
        filmcraft_export_movie::{AudioCodec, AudioSettings},
        group_surface::GroupSurface,
    },
    tools::native_rgba_export::{
        encode_frames_with_native_audio, RasterAudio, RasterEncoding, RasterVideoSpec,
    },
};
fn frame(i: u64) -> GroupSurface {
    let mut rgba = Vec::new();
    for y in 0..64 {
        for x in 0..96 {
            rgba.extend([
                (x * 3 + i * 17) as u8,
                (y * 4 + i * 19) as u8,
                (x + y + i * 23) as u8,
                (x * 2 + y) as u8,
            ]);
        }
    }
    GroupSurface {
        width: 96,
        height: 64,
        rgba,
    }
}
fn spec(rate: FrameRateRatio) -> RasterVideoSpec {
    RasterVideoSpec {
        width: 96,
        height: 64,
        frames: 3,
        frame_rate: rate,
        staging_bytes: 64 * 1024 * 1024,
    }
}
#[tokio::test]
async fn mov_jpeg_packets_match_the_pinned_source_encoder_at_quality_boundaries() {
    let (state, dir) = support::make_state(false, false).await;
    let rate = FrameRateRatio {
        numerator: 30000,
        denominator: 1001,
    };
    let spec = spec(rate);
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    for quality in [0, 1, 50, 90, 255] {
        let output = dir.path().join(format!("outputs/mjpeg-{quality}.mov"));
        encode_frames_with_native_audio(
            &state.process_runtime,
            Path::new("/never-use-ffmpeg"),
            &output,
            spec,
            Duration::from_secs(30),
            &CancellationToken::new(),
            &tx,
            None,
            RasterEncoding::Mjpeg { quality },
            Some((8, 9)),
            &[],
            Some(state.cpu_pool.clone()),
            None,
            |i, _| async move { Ok(frame(i)) },
        )
        .await
        .unwrap();
        let bytes = std::fs::read(&output).unwrap();
        let movie = filmcraft_isobmff::open(&bytes).unwrap();
        let video = movie
            .tracks
            .iter()
            .position(|track| track.kind == TrackKind::Video)
            .unwrap();
        let track = &movie.tracks[video];
        assert_eq!(track.timescale, 30000);
        assert_eq!(track.duration, 3003);
        assert_eq!(track.samples.len(), 3);
        assert_eq!(track.codec().unwrap().name(), "mjpeg");
        let desc = track.video().unwrap();
        assert!(desc.color.is_none());
        assert!(desc.field_info.is_none());
        assert!(desc.pixel_aspect.is_none());
        for i in 0..3 {
            let sample = &track.samples[i];
            assert!(sample.is_sync);
            assert_eq!(sample.duration, 1001);
            assert_eq!(sample.dts, i as i64 * 1001);
            assert_eq!(sample.pts, sample.dts);
            let expected = original::packet(
                frame(i as u64).rgba,
                96,
                64,
                quality,
                filmcraft_time::FrameRate::new(30000, 1001),
            );
            assert_eq!(
                movie.read_sample(&bytes, video, i).unwrap(),
                expected,
                "quality {quality}, frame {i}"
            );
        }
    }
    state.begin_shutdown();
    assert!(state.wait_for_tasks(Duration::from_secs(5)).await);
}
#[tokio::test]
async fn mjpeg_mov_keeps_pcm_surround_and_original_aac_packets_and_audio_duration() {
    let (state, dir) = support::make_state(false, false).await;
    let spec = spec(FrameRateRatio {
        numerator: 25,
        denominator: 1,
    });
    let count = 5292u64;
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    for (codec, channels) in [(AudioCodec::Pcm, 6), (AudioCodec::Aac, 2)] {
        let mut raw = Vec::new();
        let mut expected_pcm: Vec<u8> = Vec::new();
        let mut planar = vec![Vec::new(); channels as usize];
        for _ in 0..count {
            for (c, (value, integer)) in [
                (0.25f32, 2097152i32),
                (-0.25, -2097152),
                (0.5, 4194304),
                (-0.5, -4194304),
                (1., 8388607),
                (-1., -8388607),
            ]
            .into_iter()
            .take(channels as usize)
            .enumerate()
            {
                raw.extend(value.to_le_bytes());
                expected_pcm.extend(&integer.to_le_bytes()[..3]);
                planar[c].push(value);
            }
        }
        let audio = dir.path().join(format!("{channels}.f32le"));
        std::fs::write(&audio, raw).unwrap();
        let output = dir.path().join(format!("outputs/audio-{channels}.mov"));
        let settings = AudioSettings {
            codec,
            channels,
            bits: 24,
            sample_rate: Some(44100),
            bitrate_kbps: 128,
        };
        encode_frames_with_native_audio(
            &state.process_runtime,
            Path::new("/never-use-ffmpeg"),
            &output,
            spec,
            Duration::from_secs(30),
            &CancellationToken::new(),
            &tx,
            Some(RasterAudio {
                path: &audio,
                frames: count,
            }),
            RasterEncoding::Mjpeg { quality: 90 },
            None,
            &[("title".into(), "Native Motion JPEG".into())],
            Some(state.cpu_pool.clone()),
            Some(settings),
            |i, _| async move { Ok(frame(i)) },
        )
        .await
        .unwrap();
        let bytes = std::fs::read(&output).unwrap();
        let movie = filmcraft_isobmff::open(&bytes).unwrap();
        let sound = movie
            .tracks
            .iter()
            .position(|track| track.kind == TrackKind::Audio)
            .unwrap();
        let track = &movie.tracks[sound];
        assert_eq!(track.timescale, 44100);
        let packets = (0..track.samples.len())
            .map(|i| movie.read_sample(&bytes, sound, i).unwrap())
            .collect::<Vec<_>>();
        if codec == AudioCodec::Pcm {
            assert_eq!(track.duration, count);
            assert_eq!(packets.concat(), expected_pcm);
        } else {
            let mut encoder =
                filmcraft_aac::Encoder::new(filmcraft_aac::EncoderConfig::cbr(44100, 2, 128000))
                    .unwrap();
            let mut expected =
                encoder.encode(&planar.iter().map(Vec::as_slice).collect::<Vec<_>>());
            expected.extend(encoder.flush());
            assert_eq!(packets, expected);
            assert_eq!(track.edits[0].segment_duration, count);
            assert_eq!(track.edits[0].media_time, 1024);
        }
        let probe = tokio::process::Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_streams",
                "-show_format",
                "-of",
                "json",
            ])
            .arg(&output)
            .output()
            .await
            .unwrap();
        assert!(probe.status.success());
        let probe: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
        let streams = probe["streams"].as_array().unwrap();
        let video = streams
            .iter()
            .find(|stream| stream["codec_type"] == "video")
            .unwrap();
        let audio = streams
            .iter()
            .find(|stream| stream["codec_type"] == "audio")
            .unwrap();
        assert_eq!(video["codec_name"], "mjpeg");
        assert_eq!(video["nb_frames"], "3");
        assert_eq!(audio["channels"], channels);
        assert_eq!(audio["sample_rate"], "44100");
        assert_eq!(probe["format"]["tags"]["title"], "Native Motion JPEG");
    }
    state.begin_shutdown();
    assert!(state.wait_for_tasks(Duration::from_secs(5)).await);
}
#[tokio::test]
async fn cancelled_mjpeg_never_publishes_or_overwrites_peer_files() {
    let (state, dir) = support::make_state(false, false).await;
    let output = dir.path().join("outputs/cancel.mov");
    let peer = dir.path().join("outputs/peer.keep");
    std::fs::write(&peer, b"peer").unwrap();
    let spec = spec(FrameRateRatio {
        numerator: 25,
        denominator: 1,
    });
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let (pool, scope) = state.cpu_pool.scoped();
    let result = encode_frames_with_native_audio(
        &state.process_runtime,
        Path::new("/never-use-ffmpeg"),
        &output,
        spec,
        Duration::from_secs(30),
        &cancel,
        &tx,
        None,
        RasterEncoding::Mjpeg { quality: 90 },
        None,
        &[],
        Some(pool),
        None,
        move |i, _| {
            if i == 1 {
                trigger.cancel();
            }
            async move { Ok(frame(i)) }
        },
    )
    .await;
    assert!(result.is_err());
    scope.wait_idle().await;
    assert!(!output.exists());
    assert_eq!(std::fs::read(&peer).unwrap(), b"peer");
    assert!(!std::fs::read_dir(dir.path().join("outputs"))
        .unwrap()
        .any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".native-mjpeg-")));
    std::fs::write(&output, b"existing").unwrap();
    let result = encode_frames_with_native_audio(
        &state.process_runtime,
        Path::new("/never-use-ffmpeg"),
        &output,
        spec,
        Duration::from_secs(30),
        &CancellationToken::new(),
        &tx,
        None,
        RasterEncoding::Mjpeg { quality: 90 },
        None,
        &[],
        Some(state.cpu_pool.clone()),
        None,
        |i, _| async move { Ok(frame(i)) },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(std::fs::read(&output).unwrap(), b"existing");
    state.begin_shutdown();
    assert!(state.wait_for_tasks(Duration::from_secs(5)).await);
}

#[tokio::test]
async fn saved_mjpeg_quality_and_flat_overrides_reach_real_command_job_packets() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;
    async fn call(app: &axum::Router, token: &str, command: &str, params: Value) -> Value {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/compositions/filmcraft/command")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"command":command,"params":params}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(status, 200, "{body}");
        body
    }
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("mjpeg-preset-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    call(
        &app,
        &owner.token,
        "export.presets.save",
        json!({"name":"MJPEG review","format":"mjpeg","quality":37,"audio":false}),
    )
    .await;
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    let sequence = &mut document["project"]["items"]["6"]["kind"]["Sequence"];
    sequence["settings"]["frame_rate"] = json!({"num":24,"den":1});
    sequence["settings"]["width"] = json!(64);
    sequence["settings"]["height"] = json!(48);
    for tracks in [
        "video_tracks",
        "audio_tracks",
        "submix_tracks",
        "caption_tracks",
    ] {
        sequence[tracks] = json!([]);
    }
    let base = json!({"document":serde_json::to_string(&document).unwrap(),"sequence":6,"sources":{},"preset":"MJPEG review","range":{"startFrame":0,"endFrame":2},"wait":true});
    for quality in [37, 88] {
        let mut params = base.clone();
        if quality == 88 {
            params["quality"] = json!(quality);
        }
        let exported = call(&app, &owner.token, "export.quick", params).await;
        assert_eq!(exported["status"], "done");
        assert_eq!(exported["result"]["videoCodec"], "mjpeg");
        let bytes = std::fs::read(
            state
                .outputs_dir()
                .join(exported["result"]["filename"].as_str().unwrap()),
        )
        .unwrap();
        let movie = filmcraft_isobmff::open(&bytes).unwrap();
        assert_eq!(movie.tracks.len(), 1);
        let video = movie.track_of_kind(TrackKind::Video).unwrap();
        assert_eq!(movie.tracks[video].samples.len(), 2);
        let expected = original::packet(
            [0, 0, 0, 255].repeat(64 * 48),
            64,
            48,
            quality,
            filmcraft_time::FrameRate::new(24, 1),
        );
        for i in 0..2 {
            assert_eq!(
                movie.read_sample(&bytes, video, i).unwrap(),
                expected,
                "command quality {quality}"
            );
        }
    }
    assert_eq!(
        call(
            &app,
            &owner.token,
            "export.presets.get",
            json!({"name":"MJPEG review"})
        )
        .await["settings"]["quality"],
        37
    );
    state.begin_shutdown();
    assert!(state.wait_for_tasks(Duration::from_secs(5)).await);
}
