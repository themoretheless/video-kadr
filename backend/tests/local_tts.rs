use std::{path::Path, time::Duration};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::local_tts::{synthesize, voices, SynthesisRequest, TtsConfig},
};
#[test]
fn pocket_quality_is_bounded_family_scoped_and_persisted() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("lm_main.int8.onnx"), b"marker").unwrap();
    let config = TtsConfig {
        model_dir: Some(root.path().into()),
        ..Default::default()
    };
    let mut request: SynthesisRequest =
        serde_json::from_str(r#"{"text":"Hello","voiceId":1000,"speed":1}"#).unwrap();
    assert_eq!(request.num_steps, None);
    for steps in [3, 5, 8] {
        request.num_steps = Some(steps);
        request.validate_for_model(&config).unwrap();
        let restored: SynthesisRequest =
            serde_json::from_slice(&serde_json::to_vec(&request).unwrap()).unwrap();
        assert_eq!(restored.num_steps, Some(steps));
        assert!(request.validate_for_model(&TtsConfig::default()).is_err());
    }
    for steps in [0, 1, 4, 6, 9, u32::MAX] {
        request.num_steps = Some(steps);
        assert!(request.validate().is_err());
    }
}

#[tokio::test]
#[ignore = "requires official Pocket bundle and sherpa executable"]
async fn actual_pocket_quality_steps_generate_valid_distinct_audio() {
    let config = TtsConfig {
        executable: std::env::var("VIDEO_KADR_TEST_TTS_BIN").unwrap().into(),
        model_dir: Some(std::env::var("VIDEO_KADR_TEST_TTS_MODEL").unwrap().into()),
        threads: 2,
    };
    let dir = tempfile::tempdir().unwrap();
    let runtime = ProcessRuntime::local_default();
    let mut outputs = Vec::new();
    for steps in [3, 8] {
        let output = dir.path().join(format!("quality-{steps}.wav"));
        let audio = synthesize(
            &runtime,
            &config,
            &SynthesisRequest {
                text: "Welcome to our video project.".into(),
                voice_id: 1000,
                speed: 1.,
                num_steps: Some(steps),
                pauses: None,
            },
            &output,
            &CancellationToken::new(),
            Duration::from_secs(120),
        )
        .await
        .unwrap();
        assert_eq!(audio.engine, "pocket-sherpa-onnx");
        assert!(audio.frames > 24000);
        outputs.push(std::fs::read(output).unwrap());
    }
    assert_ne!(outputs[0], outputs[1]);
}
#[test]
fn request_and_config_admission() {
    assert_eq!(voices().len(), 36);
    assert!(voices().iter().all(|v| v.id <= 27 || v.id >= 45));
    let valid = SynthesisRequest {
        pauses: None,
        num_steps: None,
        text: "Hello, world".into(),
        voice_id: 3,
        speed: 1.,
    };
    valid.validate().unwrap();
    for text in [
        "".to_string(),
        " \n ".into(),
        "x\0y".into(),
        "x".repeat(4097),
    ] {
        assert!(SynthesisRequest {
            text,
            ..valid.clone()
        }
        .validate()
        .is_err());
    }
    for id in [28, 44, 53, u32::MAX] {
        assert!(SynthesisRequest {
            voice_id: id,
            ..valid.clone()
        }
        .validate()
        .is_err());
    }
    for speed in [f32::NAN, f32::INFINITY, 0., 2.1] {
        assert!(SynthesisRequest {
            speed,
            ..valid.clone()
        }
        .validate()
        .is_err());
    }
    assert!(TtsConfig::from_lookup(&mut |key| match key {
        "KOKORO_MODEL_DIR" => Some("relative/path".into()),
        _ => None,
    })
    .is_err());
    assert!(TtsConfig::from_lookup(&mut |key| match key {
        "KOKORO_THREADS" => Some("0".into()),
        _ => None,
    })
    .is_err());
}
#[cfg(unix)]
fn fixture(root: &Path, hang: bool) -> TtsConfig {
    use std::os::unix::fs::PermissionsExt;
    let model = root.join("model");
    std::fs::create_dir(&model).unwrap();
    std::fs::create_dir(model.join("espeak-ng-data")).unwrap();
    std::fs::write(model.join("model.onnx"), vec![0u8; 1_000_000]).unwrap();
    for name in [
        "voices.bin",
        "tokens.txt",
        "lexicon-us-en.txt",
        "lexicon-zh.txt",
    ] {
        std::fs::write(model.join(name), "fixture").unwrap();
    }
    let mut wav = vec![];
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&996u32.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&24000u32.to_le_bytes());
    wav.extend_from_slice(&48000u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&960u32.to_le_bytes());
    for _ in 0..480 {
        wav.extend_from_slice(&1000i16.to_le_bytes());
    }
    std::fs::write(root.join("fixture.wav"), wav).unwrap();
    let executable = root.join("engine");
    let script=format!("#!/bin/sh\nfor arg in \"$@\"; do case \"$arg\" in --) break;; --output-filename=*) output=${{arg#--output-filename=}};; esac; done\ncp '{}' \"$output\"\n{}\n",root.join("fixture.wav").display(),if hang{"sleep 20"}else{"exit 0"});
    std::fs::write(&executable, script).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    TtsConfig {
        executable,
        model_dir: Some(model),
        threads: 2,
    }
}
#[cfg(unix)]
#[tokio::test]
async fn publication_no_replace_cancel_timeout_and_staging_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let config = fixture(dir.path(), false);
    let request = SynthesisRequest {
        pauses: None,
        num_steps: None,
        text: "--output-filename=untrusted.wav".into(),
        voice_id: 3,
        speed: 1.,
    };
    let output = dir.path().join("narration.wav");
    let runtime = ProcessRuntime::local_default();
    let audio = synthesize(
        &runtime,
        &config,
        &request,
        &output,
        &CancellationToken::new(),
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert_eq!(audio.sample_rate, 24000);
    assert_eq!(audio.frames, 480);
    assert_eq!(audio.duration_us, 20000);
    let original = std::fs::read(&output).unwrap();
    assert!(synthesize(
        &runtime,
        &config,
        &request,
        &output,
        &CancellationToken::new(),
        Duration::from_secs(5)
    )
    .await
    .is_err());
    assert_eq!(std::fs::read(&output).unwrap(), original);
    let hanging = tempfile::tempdir().unwrap();
    let config = fixture(hanging.path(), true);
    let output = hanging.path().join("never.wav");
    assert!(synthesize(
        &runtime,
        &config,
        &request,
        &output,
        &CancellationToken::new(),
        Duration::from_millis(200)
    )
    .await
    .is_err());
    assert!(!output.exists());
    let cancel = CancellationToken::new();
    let worker_cancel = cancel.clone();
    let task = tokio::spawn(async move {
        synthesize(
            &ProcessRuntime::local_default(),
            &config,
            &request,
            &output,
            &worker_cancel,
            Duration::from_secs(10),
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel.cancel();
    assert!(tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    assert!(std::fs::read_dir(hanging.path()).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".tts-")));
}
#[tokio::test]
#[ignore = "requires official local Kokoro bundle and sherpa-onnx executable"]
async fn actual_kokoro_speaks_with_selected_voice_and_speed() {
    let config = TtsConfig {
        executable: std::env::var("VIDEO_KADR_TEST_TTS_BIN").unwrap().into(),
        model_dir: Some(std::env::var("VIDEO_KADR_TEST_TTS_MODEL").unwrap().into()),
        threads: 2,
    };
    let dir = tempfile::tempdir().unwrap();
    let runtime = ProcessRuntime::local_default();
    let request = SynthesisRequest {
        pauses: None,
        num_steps: None,
        text: "The video editor creates a new audio track. Welcome to our project.".into(),
        voice_id: 3,
        speed: 1.,
    };
    let normal = synthesize(
        &runtime,
        &config,
        &request,
        &dir.path().join("normal.wav"),
        &CancellationToken::new(),
        Duration::from_secs(120),
    )
    .await
    .unwrap();
    assert_eq!(
        normal.model_hash,
        "4b86207ef680e394d8343bee22dfc4c512e5c707c6d9578e3f35ab09bffd6b36"
    );
    assert_eq!(normal.sample_rate, 24000);
    assert!((2_000_000..30_000_000).contains(&normal.duration_us));
    let fast = synthesize(
        &runtime,
        &config,
        &SynthesisRequest {
            speed: 1.5,
            ..request.clone()
        },
        &dir.path().join("fast.wav"),
        &CancellationToken::new(),
        Duration::from_secs(120),
    )
    .await
    .unwrap();
    assert!(fast.frames < normal.frames);
    assert_eq!(fast.model_hash, normal.model_hash);
    let alternate = synthesize(
        &runtime,
        &config,
        &SynthesisRequest {
            voice_id: 11,
            ..request
        },
        &dir.path().join("alternate.wav"),
        &CancellationToken::new(),
        Duration::from_secs(120),
    )
    .await
    .unwrap();
    assert_ne!(
        std::fs::read(dir.path().join("normal.wav")).unwrap(),
        std::fs::read(dir.path().join("alternate.wav")).unwrap()
    );
    assert_eq!(alternate.voice_id, 11);
    let whisper = video_kadr_backend::services::local_speech::SpeechConfig {
        executable: "/opt/homebrew/bin/whisper-cli".into(),
        model: Some(
            std::env::var("VIDEO_KADR_TEST_WHISPER_MODEL")
                .expect("explicit Whisper model for synthesized speech qualification")
                .into(),
        ),
        threads: 2,
    };
    let transcript = video_kadr_backend::services::local_speech::transcribe(
        &runtime,
        &whisper,
        &dir.path().join("normal.wav"),
        0,
        normal.duration_us,
        &dir.path().join("recognition-staging"),
        "en",
    )
    .await
    .unwrap();
    let recognized = transcript
        .segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    assert!(
        recognized.contains("video") && recognized.contains("project"),
        "synthesized narration must be intelligible: {recognized}"
    );
}

#[tokio::test]
#[ignore = "requires official Pocket bundle, sherpa executable and public Whisper model"]
async fn actual_pocket_stock_references_produce_intelligible_distinct_voices() {
    let config = TtsConfig {
        executable: std::env::var("VIDEO_KADR_TEST_TTS_BIN").unwrap().into(),
        model_dir: Some(std::env::var("VIDEO_KADR_TEST_TTS_MODEL").unwrap().into()),
        threads: 2,
    };
    let dir = tempfile::tempdir().unwrap();
    let runtime = ProcessRuntime::local_default();
    let mut outputs = vec![];
    for voice_id in [1000, 1001] {
        let output = dir.path().join(format!("{voice_id}.wav"));
        let audio = synthesize(
            &runtime,
            &config,
            &SynthesisRequest {
                pauses: None,
                num_steps: None,
                text: "The video editor creates a new audio track. Welcome to our project.".into(),
                voice_id,
                speed: 1.,
            },
            &output,
            &CancellationToken::new(),
            Duration::from_secs(120),
        )
        .await
        .unwrap();
        assert_eq!(audio.engine, "pocket-sherpa-onnx");
        assert_eq!(audio.voice_id, voice_id);
        assert_eq!(audio.sample_rate, 24000);
        assert!(audio.frames > 24000 && audio.duration_us < 30_000_000);
        assert_eq!(
            audio.model_hash,
            "bfc0c7e7e3d72864fa3bb2ee499f62f21ddc1474b885f5f3ca570f8be73e787e"
        );
        outputs.push((output, audio));
    }
    assert_ne!(
        std::fs::read(&outputs[0].0).unwrap(),
        std::fs::read(&outputs[1].0).unwrap()
    );
    let fast = synthesize(
        &runtime,
        &config,
        &SynthesisRequest {
            pauses: None,
            num_steps: None,
            text: "The video editor creates a new audio track. Welcome to our project.".into(),
            voice_id: 1000,
            speed: 2.,
        },
        &dir.path().join("fast.wav"),
        &CancellationToken::new(),
        Duration::from_secs(120),
    )
    .await
    .unwrap();
    assert!(
        fast.frames < outputs[0].1.frames,
        "Pocket speed must affect generated audio duration"
    );
    let whisper = video_kadr_backend::services::local_speech::SpeechConfig {
        executable: "/opt/homebrew/bin/whisper-cli".into(),
        model: Some(
            std::env::var("VIDEO_KADR_TEST_WHISPER_MODEL")
                .unwrap()
                .into(),
        ),
        threads: 2,
    };
    let transcript = video_kadr_backend::services::local_speech::transcribe(
        &runtime,
        &whisper,
        &outputs[0].0,
        0,
        outputs[0].1.duration_us,
        &dir.path().join("recognition-staging"),
        "en",
    )
    .await
    .unwrap();
    let recognized = transcript
        .segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    assert!(
        recognized.contains("video") && recognized.contains("project"),
        "Pocket narration must be intelligible: {recognized}"
    );
}

#[test]
fn chatterbox_family_refuses_cross_family_voices_and_non_native_speed() {
    use video_kadr_backend::services::local_tts::{family, model_voices, TtsFamily};
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("onnx")).unwrap();
    std::fs::write(
        root.path().join("onnx/language_model_quantized.onnx"),
        b"layout marker",
    )
    .unwrap();
    let config = TtsConfig {
        executable: "/host/chatterbox".into(),
        model_dir: Some(root.path().to_owned()),
        threads: 2,
    };
    assert_eq!(family(&config).unwrap(), TtsFamily::Chatterbox);
    assert_eq!(
        model_voices(&config)
            .unwrap()
            .iter()
            .map(|v| v.id)
            .collect::<Vec<_>>(),
        vec![2000, 2099]
    );
    let mut request = SynthesisRequest {
        pauses: None,
        num_steps: None,
        text: "Welcome".into(),
        voice_id: 2099,
        speed: 1.,
    };
    request.validate_for_model(&config).unwrap();
    request.speed = 2.;
    assert!(request.validate_for_model(&config).is_err());
    request.speed = 1.;
    request.voice_id = 1099;
    assert!(request.validate_for_model(&config).is_err());
    std::fs::write(root.path().join("model.onnx"), b"mixed").unwrap();
    assert!(family(&config).is_err());
}
