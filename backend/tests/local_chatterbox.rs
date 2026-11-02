use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::local_chatterbox::{self, Config, Input},
};
#[tokio::test]
async fn invalid_and_cancelled_inputs_refuse_before_model_access() {
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("output.wav");
    let config = Config {
        executable: "/missing-cli".into(),
        model_dir: "/missing-model".into(),
        runtime_library: "/missing-library".into(),
    };
    let reference = PathBuf::from("/missing-reference");
    let runtime = ProcessRuntime::local_default();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let cancel = CancellationToken::new();
    let error = local_chatterbox::synthesize(
        &runtime,
        &config,
        Input {
            text: "",
            reference: &reference,
        },
        &destination,
        &cancel,
        Duration::from_secs(1),
        &tx,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("invalid Chatterbox narration"));
    cancel.cancel();
    let error = local_chatterbox::synthesize(
        &runtime,
        &config,
        Input {
            text: "Hello",
            reference: &reference,
        },
        &destination,
        &cancel,
        Duration::from_secs(1),
        &tx,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("cancelled"));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
#[tokio::test]
#[ignore = "requires actual Chatterbox CLI, author model/runtime and public WAV"]
async fn actual_runtime_generation_progress_and_cancellation() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_BIN")
            .unwrap()
            .into(),
        model_dir: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_MODEL")
            .unwrap()
            .into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let reference = PathBuf::from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_REFERENCE").unwrap());
    let runtime = ProcessRuntime::local_default();
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("completed.wav");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let result = local_chatterbox::synthesize(
        &runtime,
        &config,
        Input {
            text: "Welcome to our video project.",
            reference: &reference,
        },
        &destination,
        &CancellationToken::new(),
        Duration::from_secs(180),
        &tx,
    )
    .await
    .unwrap();
    assert_eq!(result.voice_id, 2099);
    assert_eq!(result.engine, "chatterbox-onnx");
    assert_eq!(result.sample_rate, 24000);
    assert!(result.frames >= 24000);
    assert_eq!(
        result.model_hash,
        "ec9945df36cb5d131d46688f2609fd715fbfcb0b8ee9681af5c84118de2d55a2"
    );
    let mut values = Vec::new();
    while let Ok(value) = rx.try_recv() {
        values.push(value);
    }
    assert_eq!(values.last(), Some(&100.));
    assert!(values.iter().any(|n| *n > 0. && *n < 95.));
    assert!(values.windows(2).all(|p| p[0] <= p[1]));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&destination)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let before = std::fs::read(&destination).unwrap();
    let error = local_chatterbox::synthesize(
        &runtime,
        &config,
        Input {
            text: "Do not overwrite",
            reference: &reference,
        },
        &destination,
        &CancellationToken::new(),
        Duration::from_secs(10),
        &tx,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("already exists"));
    assert_eq!(std::fs::read(&destination).unwrap(), before);
    std::fs::copy(
        &destination,
        std::env::var("VIDEO_KADR_TEST_CHATTERBOX_ADAPTER_OUTPUT").unwrap(),
    )
    .unwrap();
    let cancelled = dir.path().join("cancelled.wav");
    let cancel = CancellationToken::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let future = local_chatterbox::synthesize(
        &runtime,
        &config,
        Input {
            text: "Welcome to our video project.",
            reference: &reference,
        },
        &cancelled,
        &cancel,
        Duration::from_secs(180),
        &tx,
    );
    tokio::pin!(future);
    loop {
        tokio::select! {result=&mut future=>panic!("process finished before actual token progress: {result:?}"),value=rx.recv()=>{if value.is_some_and(|v|v>0.){cancel.cancel();break;}}}
    }
    let started = std::time::Instant::now();
    assert!(future.await.is_err());
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(!cancelled.exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
