use std::time::Duration;
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::{
        local_kokoro::{self, Config, Input},
        local_tts::SynthesisRequest,
    },
};

#[tokio::test]
#[ignore = "requires native Kokoro executable, official model and shared libraries"]
async fn actual_native_pauses_publication_progress_cancel_and_library_identity() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_NATIVE_KOKORO_BIN")
            .unwrap()
            .into(),
        model_dir: std::env::var("VIDEO_KADR_TEST_TTS_MODEL").unwrap().into(),
        runtime_library: std::env::var("VIDEO_KADR_SHERPA_LIBRARY").unwrap().into(),
        ort_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
        threads: 2,
    };
    let dir = tempfile::tempdir().unwrap();
    let runtime = ProcessRuntime::local_default();
    let request=SynthesisRequest{text:"Welcome to our video project. We edit pictures and add clear narration. Thank you for listening.".into(),voice_id:3,speed:1.,pauses:None,num_steps:None};
    let mut frames = vec![];
    for pauses in [0., 1.] {
        let output = dir.path().join(format!("pause-{pauses}.wav"));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let audio = local_kokoro::synthesize(
            &runtime,
            &config,
            Input {
                request: &request,
                pauses,
            },
            &output,
            &CancellationToken::new(),
            Duration::from_secs(120),
            &tx,
        )
        .await
        .unwrap();
        assert_eq!(audio.voice_id, 3);
        assert_eq!(audio.engine, "kokoro-sherpa-onnx");
        frames.push(audio.frames);
        let mut progress = vec![];
        while let Ok(value) = rx.try_recv() {
            progress.push(value)
        }
        assert_eq!(progress.last(), Some(&100.));
        assert!(progress.windows(2).all(|p| p[0] <= p[1]));
        let original = std::fs::read(&output).unwrap();
        assert!(local_kokoro::synthesize(
            &runtime,
            &config,
            Input {
                request: &request,
                pauses
            },
            &output,
            &CancellationToken::new(),
            Duration::from_secs(120),
            &tx
        )
        .await
        .is_err());
        assert_eq!(std::fs::read(output).unwrap(), original);
    }
    assert!(frames[1] > frames[0]);
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let observer = tokio::spawn(async move {
        if rx.recv().await.is_some() {
            signal.cancel()
        }
    });
    let output = dir.path().join("cancelled.wav");
    assert!(local_kokoro::synthesize(
        &runtime,
        &config,
        Input {
            request: &request,
            pauses: 0.2
        },
        &output,
        &cancel,
        Duration::from_secs(120),
        &tx
    )
    .await
    .is_err());
    drop(tx);
    observer.await.unwrap();
    assert!(!output.exists());
    let mut wrong = config.clone();
    wrong.runtime_library = config.ort_library.clone();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    assert!(local_kokoro::synthesize(
        &runtime,
        &wrong,
        Input {
            request: &request,
            pauses: 0.2
        },
        &dir.path().join("wrong.wav"),
        &CancellationToken::new(),
        Duration::from_secs(120),
        &tx
    )
    .await
    .is_err());
    assert!(std::fs::read_dir(dir.path()).unwrap().all(|p| !p
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".kokoro-")));
}
