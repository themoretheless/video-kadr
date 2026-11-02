#![cfg(target_os = "macos")]
mod support;
use std::{path::Path, time::Duration};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    domain::{
        composition::FrameRateRatio,
        filmcraft_export_movie::{AudioCodec, AudioSettings, Multiplexer},
        group_surface::GroupSurface,
    },
    tools::native_rgba_export::{
        encode_frames_with_native_audio, RasterAudio, RasterEncoding, RasterVideoSpec,
    },
};
fn spec() -> RasterVideoSpec {
    RasterVideoSpec {
        width: 64,
        height: 48,
        frames: 3,
        frame_rate: FrameRateRatio {
            numerator: 25,
            denominator: 1,
        },
        staging_bytes: 64 * 1024 * 1024,
    }
}
fn frame() -> GroupSurface {
    GroupSurface {
        width: 64,
        height: 48,
        rgba: [255, 0, 0, 255].repeat(64 * 48),
    }
}
fn encoding(multiplexer: Multiplexer, audio: AudioSettings) -> RasterEncoding {
    RasterEncoding::NativeHevc {
        target_kbps: 1000,
        max_kbps: 2000,
        keyint: 2,
        cbr: false,
        multiplexer,
        audio,
    }
}
#[tokio::test]
async fn guarded_cpu_pool_hevc_movies_include_prepared_audio() {
    let (state, dir) = support::make_state(false, false).await;
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    for (mux, codec) in [
        (Multiplexer::Mov, AudioCodec::Pcm),
        (Multiplexer::Mp4, AudioCodec::Aac),
    ] {
        let settings = AudioSettings {
            codec,
            bits: 24,
            sample_rate: Some(44100),
            channels: 2,
            bitrate_kbps: 128,
        };
        let count = spec().audio_frames_at_rate(44100).unwrap();
        let audio = dir.path().join("sound.f32");
        let samples: Vec<u8> = (0..count * 2)
            .flat_map(|i| ((i as f32 * 0.02).sin() * 0.2).to_le_bytes())
            .collect();
        std::fs::write(&audio, &samples).unwrap();
        let output = dir.path().join(format!("outputs/hevc.{:?}", mux));
        encode_frames_with_native_audio(
            &state.process_runtime,
            Path::new("/never-ffmpeg"),
            &output,
            spec(),
            Duration::from_secs(30),
            &CancellationToken::new(),
            &tx,
            Some(RasterAudio {
                path: &audio,
                frames: count,
            }),
            encoding(mux, settings),
            None,
            &[],
            Some(state.cpu_pool.clone()),
            None,
            |_, _| async { Ok(frame()) },
        )
        .await
        .unwrap();
        let bytes = std::fs::read(&output).unwrap();
        let parsed = filmcraft_isobmff::open(&bytes).unwrap();
        assert_eq!(parsed.tracks.len(), 2);
        assert_eq!(parsed.tracks[0].samples.len(), 3);
        assert_eq!(parsed.tracks[0].codec().unwrap().name(), "hevc");
        assert_eq!(parsed.tracks[1].timescale, 44100);
        assert_eq!(parsed.tracks[1].tkhd_duration, count);
        assert_eq!(
            parsed.tracks[1].codec().unwrap().name(),
            if codec == AudioCodec::Pcm {
                "pcm"
            } else {
                "aac"
            }
        );
    }
}
#[tokio::test]
async fn cancellation_drains_workers_and_preserves_existing_destination() {
    let (state, dir) = support::make_state(false, false).await;
    let output = dir.path().join("outputs/cancel.mov");
    let peer = dir.path().join("outputs/peer.mov");
    std::fs::write(&peer, b"peer").unwrap();
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let (pool, scope) = state.cpu_pool.scoped();
    let result = encode_frames_with_native_audio(
        &state.process_runtime,
        Path::new("/never-ffmpeg"),
        &output,
        spec(),
        Duration::from_secs(30),
        &cancel,
        &tx,
        None,
        encoding(Multiplexer::Mov, AudioSettings::default()),
        None,
        &[],
        Some(pool),
        None,
        move |i, _| {
            if i == 1 {
                trigger.cancel();
            }
            async { Ok(frame()) }
        },
    )
    .await;
    assert!(result.is_err());
    scope.wait_idle().await;
    assert!(!output.exists());
    assert_eq!(std::fs::read(&peer).unwrap(), b"peer");
    assert!(!std::fs::read_dir(output.parent().unwrap())
        .unwrap()
        .any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".native-hevc-")));
    std::fs::write(&output, b"existing").unwrap();
    let result = encode_frames_with_native_audio(
        &state.process_runtime,
        Path::new("/never-ffmpeg"),
        &output,
        spec(),
        Duration::from_secs(30),
        &CancellationToken::new(),
        &tx,
        None,
        encoding(Multiplexer::Mov, AudioSettings::default()),
        None,
        &[],
        Some(state.cpu_pool.clone()),
        None,
        |_, _| async { Ok(frame()) },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(std::fs::read(&output).unwrap(), b"existing");
}
