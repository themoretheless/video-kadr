use serde_json::json;
use std::{collections::BTreeMap, path::Path, sync::atomic::AtomicBool, time::Duration};
use video_kadr_backend::domain::{
    composition::SourceId,
    concat_media_slots,
    concat_project_export::export_with_paths,
    concat_project_import::{import, ImportRequest},
    nested_composition::NestedCompositionProject,
};
fn filled_project(kind: &str, speed: f64, start: f64) -> NestedCompositionProject {
    let original = json!({"version":1,"media":[{"id":"slot","kind":kind,"path":"/creator","name":"Creator","placeholder":true}],"timelines":[{"id":"main","video":{"width":2,"height":2,"rateNum":4,"rateDen":1},"tracks":[{"id":"track"}],"clips":[{"id":"clip","trackId":"track","mediaId":"slot","kind":kind,"start":start,"duration":2.,"sourceStart":7.,"speed":speed}]}]});
    let filled=concat_media_slots::fill(&original,"slot",&json!({"path":"library:new","name":"Replacement","kind":kind,"duration":1.,"width":if kind=="video"{Some(2)}else{None},"height":if kind=="video"{Some(2)}else{None},"hasAudio":kind=="audio"})).unwrap();
    let request:ImportRequest=serde_json::from_value(json!({"schemaVersion":1,"document":filled,"mediaBindings":{"slot":{"id":"new","kind":kind,"durationTicks":1000000,"width":if kind=="video"{2}else{0},"height":if kind=="video"{2}else{0},"hasAudio":kind=="audio"}}})).unwrap();
    let report = import(request).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    report.project.unwrap()
}
#[test]
fn short_source_admission_preserves_virtual_source_and_slot_windows_and_refuses_legacy_export() {
    for kind in ["video", "audio"] {
        for speed in [1., 2.] {
            let project = filled_project(kind, speed, 0.);
            let content = &project.compositions[&project.root].content;
            assert!(content.validate_native_surface().is_ok());
            assert!(content.validate().is_err());
            let native = serde_json::to_value(content).unwrap();
            let placement = &native["tracks"][0]["clips"][0]["placement"];
            assert_eq!(placement["concatSourceEof"], true);
            assert_eq!(placement["sourceInTick"], 0);
            assert_eq!(placement["sourceOutTick"], (2_000_000. * speed) as u64);
            assert_eq!(content.native_duration_ticks().unwrap(), 2_000_000);
            let output = export_with_paths(
                &project,
                &BTreeMap::from([("new".into(), "/never/open".into())]),
            )
            .unwrap();
            assert_eq!(output["timelines"][0]["clips"][0]["duration"], 2.);
            assert_eq!(output["timelines"][0]["clips"][0]["speed"], speed);
            if kind == "video" {
                assert!(
                    video_kadr_backend::tools::composition_native_video::needs_native_video(
                        content
                    )
                );
            } else {
                assert!(
                    video_kadr_backend::tools::composition_native_audio::needs_native_audio(
                        content
                    )
                );
            }
            let mut disabled = native.clone();
            disabled["tracks"][0]["clips"][0]["enabled"] = json!(false);
            let disabled: video_kadr_backend::domain::composition::Composition =
                serde_json::from_value(disabled).unwrap();
            if kind == "video" {
                assert!(
                    video_kadr_backend::tools::composition_native_video::needs_native_video(
                        &disabled
                    )
                );
            } else {
                assert!(
                    video_kadr_backend::tools::composition_native_audio::needs_native_audio(
                        &disabled
                    )
                );
            }
            let mut strict = native.clone();
            strict["tracks"][0]["clips"][0]["placement"]
                .as_object_mut()
                .unwrap()
                .remove("concatSourceEof");
            let strict: video_kadr_backend::domain::composition::Composition =
                serde_json::from_value(strict).unwrap();
            assert!(strict.validate_native_surface().is_err());
        }
    }
}
#[test]
#[ignore = "requires actual FFmpeg/ffprobe; generated lossless four-frame clip"]
fn short_video_holds_last_pixels_until_the_slot_end() {
    use std::process::Command;
    let temp = tempfile::tempdir().unwrap();
    let raw = temp.path().join("frames.rgba");
    let movie = temp.path().join("short.mkv");
    let colors: [[u8; 4]; 4] = [
        [16, 32, 48, 255],
        [64, 80, 96, 255],
        [112, 128, 144, 255],
        [160, 176, 192, 255],
    ];
    std::fs::write(
        &raw,
        colors
            .iter()
            .flat_map(|color| color.repeat(4))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let status = Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", "2x2", "-r", "4", "-i",
        ])
        .arg(&raw)
        .args(["-c:v", "ffv1"])
        .arg(&movie)
        .status()
        .unwrap();
    assert!(status.success());
    let project = filled_project("video", 2., 1.);
    let sources = BTreeMap::from([(SourceId::parse("new").unwrap(), movie)]);
    for (time, index) in [
        (1_000_000, 0),
        (1_125_000, 1),
        (1_375_000, 3),
        (1_500_000, 3),
        (2_500_000, 3),
        (2_999_999, 3),
    ] {
        let frame = video_kadr_backend::domain::group_media::render_project_frame_sources(
            &project,
            &project.root,
            Path::new("ffmpeg"),
            &BTreeMap::new(),
            &sources,
            time,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(frame.rgba, colors[index].repeat(4), "slot time {time}");
    }
}
#[tokio::test]
#[ignore = "requires actual FFmpeg; generated one-second constant mono PCM"]
async fn short_audio_is_silent_after_eof_without_shortening_the_slot_at_1x_and_2x() {
    use std::process::Command;
    use video_kadr_backend::{
        process_control::ProcessRuntime,
        tools::native_audio_export::{mix_project_audio_tracks, PcmSource},
    };
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("short.wav");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "aevalsrc=0.25:s=48000:d=1",
            "-c:a",
            "pcm_f32le",
        ])
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());
    let strict = video_kadr_backend::tools::pcm_conform::conform_pcm(
        &ProcessRuntime::local_default(),
        Path::new("ffmpeg"),
        &source,
        temp.path(),
        video_kadr_backend::tools::pcm_conform::PcmConformSpec {
            start_sample: 72_000,
            end_sample: 96_000,
            channels: 1,
            staging_bytes: 96_000,
        },
        Duration::from_secs(10),
        &tokio_util::sync::CancellationToken::new(),
    )
    .await;
    assert!(
        strict
            .err()
            .unwrap()
            .to_string()
            .contains("PCM conform produced invalid"),
        "ordinary conform must retain strict empty-output refusal"
    );
    for speed in [1., 2.] {
        let project = filled_project("audio", speed, 0.);
        let sources = BTreeMap::from([(
            SourceId::parse("new").unwrap(),
            PcmSource {
                path: source.clone(),
                channels: 1,
                probe: None,
            },
        )]);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mixed = mix_project_audio_tracks(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            temp.path(),
            &project,
            &project.root,
            0..2_000_000,
            96_000,
            &sources,
            4 * 1024 * 1024,
            Duration::from_secs(15),
            &tokio_util::sync::CancellationToken::new(),
            &tx,
        )
        .await
        .unwrap();
        assert_eq!(mixed.frames, 96_000);
        let bytes = std::fs::read(mixed.path()).unwrap();
        assert_eq!(bytes.len(), 96_000 * 8);
        let samples: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        assert!(
            samples[9600].abs() > 0.1,
            "speed {speed}: source was not audible"
        );
        assert!(
            samples[144_000..].iter().all(|value| value.abs() < 1e-7),
            "speed {speed}: source tail repeated beyond EOF"
        );
        let tail = mix_project_audio_tracks(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            temp.path(),
            &project,
            &project.root,
            1_500_000..2_000_000,
            24_000,
            &sources,
            4 * 1024 * 1024,
            Duration::from_secs(15),
            &tokio_util::sync::CancellationToken::new(),
            &tx,
        )
        .await
        .unwrap();
        assert_eq!(tail.frames, 24_000);
        let bytes = std::fs::read(tail.path()).unwrap();
        assert_eq!(bytes.len(), 24_000 * 8);
        assert!(
            bytes.iter().all(|byte| *byte == 0),
            "selected EOF-only range must be silent"
        );
    }
}

#[tokio::test]
#[ignore = "requires actual FFmpeg/ffprobe; exports and decodes a selected-range movie"]
async fn selected_range_movie_export_retains_the_long_slot_and_last_frame() {
    use std::process::Command;
    let temp = tempfile::tempdir().unwrap();
    let raw = temp.path().join("movie-frames.rgba");
    let source = temp.path().join("movie-source.mkv");
    let colors: [[u8; 4]; 4] = [
        [16, 32, 48, 255],
        [64, 80, 96, 255],
        [112, 128, 144, 255],
        [160, 176, 192, 255],
    ];
    std::fs::write(
        &raw,
        colors
            .iter()
            .flat_map(|color| color.repeat(4))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", "2x2", "-r", "4", "-i"
        ])
        .arg(&raw)
        .args(["-c:v", "ffv1"])
        .arg(&source)
        .status()
        .unwrap()
        .success());
    let project = filled_project("video", 2., 1.);
    let content = &project.compositions[&project.root].content;
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let prepared = video_kadr_backend::tools::composition_native_video::prepare_native_video(
        &video_kadr_backend::process_control::ProcessRuntime::local_default(),
        temp.path(),
        content,
        &BTreeMap::from([(SourceId::parse("new").unwrap(), source)]),
        1_000_000..3_000_000,
        Duration::from_secs(30),
        &tokio_util::sync::CancellationToken::new(),
        &tx,
    )
    .await
    .unwrap();
    let decoded = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(prepared.path())
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
        .output()
        .unwrap();
    assert!(
        decoded.status.success(),
        "{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    assert_eq!(decoded.stdout.len(), 8 * 16);
    for (index, frame) in decoded.stdout.chunks_exact(16).enumerate() {
        assert_eq!(
            frame,
            colors[if index == 0 {
                0
            } else if index == 1 {
                2
            } else {
                3
            }]
            .repeat(4),
            "encoded frame {index}"
        );
    }
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "json",
        ])
        .arg(prepared.path())
        .output()
        .unwrap();
    assert!(probe.status.success());
    let metadata: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    assert_eq!(
        metadata["format"]["duration"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap(),
        2.
    );
}

#[tokio::test]
#[ignore = "requires actual FFmpeg; generated tone through fixed and packet DSP"]
async fn short_source_dsp_preserves_effect_tails_and_packet_eof_silence() {
    use std::process::Command;
    use video_kadr_backend::{
        process_control::ProcessRuntime,
        tools::native_audio_export::{mix_project_audio_tracks, PcmSource},
    };
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("dsp-short.wav");
    assert!(Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "aevalsrc=0.25*sin(2*PI*440*t):s=48000:d=1",
            "-c:a",
            "pcm_f32le"
        ])
        .arg(&source)
        .status()
        .unwrap()
        .success());
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(&source)
        .output()
        .unwrap();
    assert!(output.status.success());
    let probe = video_kadr_backend::domain::media_probe::ProbeResult::from_ffprobe_json(
        &serde_json::from_slice(&output.stdout).unwrap(),
    )
    .unwrap();
    for (id, effects, tail) in [
        (
            "bass",
            json!([{"id":"concat.bass","params":{"gain":4.}}]),
            false,
        ),
        (
            "echo",
            json!([{"id":"concat.echo","params":{"delay":0.25,"decay":0.4}}]),
            true,
        ),
        (
            "pitch",
            json!([{"id":"concat.pitch","params":{"semitones":7.}}]),
            false,
        ),
        (
            "bass+echo",
            json!([{"id":"concat.bass","params":{"gain":4.}},{"id":"concat.echo","params":{"delay":0.25,"decay":0.4}}]),
            true,
        ),
    ] {
        let initial = filled_project("audio", 1., 0.);
        let mut wire = serde_json::to_value(&initial).unwrap();
        wire["compositions"][&initial.root]["content"]["tracks"][0]["clips"][0]["dspEffects"] =
            effects;
        let project: NestedCompositionProject = serde_json::from_value(wire).unwrap();
        project.dependency_order().unwrap();
        let sources = BTreeMap::from([(
            SourceId::parse("new").unwrap(),
            PcmSource::from_probe(source.clone(), probe.clone()).unwrap(),
        )]);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mixed = mix_project_audio_tracks(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            temp.path(),
            &project,
            &project.root,
            0..2_000_000,
            96_000,
            &sources,
            256 * 1024 * 1024,
            Duration::from_secs(20),
            &tokio_util::sync::CancellationToken::new(),
            &tx,
        )
        .await
        .unwrap_or_else(|error| panic!("{id}: {error:#}"));
        let bytes = std::fs::read(mixed.path()).unwrap();
        assert_eq!(bytes.len(), 96_000 * 8);
        let samples: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        assert!(samples.iter().all(|v| v.is_finite()));
        let selected = mix_project_audio_tracks(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            temp.path(),
            &project,
            &project.root,
            950_000..1_250_000,
            14_400,
            &sources,
            256 * 1024 * 1024,
            Duration::from_secs(20),
            &tokio_util::sync::CancellationToken::new(),
            &tx,
        )
        .await
        .unwrap_or_else(|error| panic!("{id} selected tail: {error:#}"));
        let selected_bytes = std::fs::read(selected.path()).unwrap();
        assert_eq!(
            selected_bytes,
            &bytes[45_600 * 8..60_000 * 8],
            "{id}: selected export lost source/DSP history near EOF"
        );

        assert!(
            samples[9600..19200].iter().any(|v| v.abs() > 0.01),
            "{id}: source was not audible"
        );
        if tail {
            assert!(
                samples[105600..115200].iter().any(|v| v.abs() > 0.001),
                "{id}: effect tail was cut at source EOF"
            );
        }
        assert!(
            samples[144000..].iter().all(|v| v.abs() < 1e-6),
            "{id}: stale source/effect output after EOF tail"
        );
    }
}
