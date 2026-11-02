use std::time::Duration;
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::local_enhance::{self, Config, Input},
};
#[test]
fn runtime_configuration_requires_complete_absolute_paths() {
    assert!(Config::from_lookup(&mut |_| None).unwrap().is_none());
    assert!(Config::from_lookup(&mut |key| if key == "ENHANCE_BIN" {
        Some("/native".into())
    } else {
        None
    })
    .is_err());
    assert!(Config::from_lookup(&mut |_| Some("relative".into())).is_err());
    assert!(Config::from_lookup(&mut |_| Some("/host/file".into()))
        .unwrap()
        .is_some());
}
#[tokio::test]
#[ignore = "requires native enhancement executable, verified model and ONNX Runtime"]
async fn actual_process_progress_cancel_timeout_source_change_and_no_replace() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        model: std::env::var("VIDEO_KADR_TEST_ENHANCE_MODEL")
            .unwrap()
            .into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("synthetic.png");
    image::RgbaImage::from_fn(257, 9, |x, y| {
        image::Rgba([
            ((x * 13 + y * 7) % 256) as u8,
            ((x * 9 + y * 17) % 256) as u8,
            80,
            255,
        ])
    })
    .save(&source)
    .unwrap();
    let original = std::fs::read(&source).unwrap();
    let hash = video_kadr_backend::services::local_speech::fingerprint_model(&source)
        .await
        .unwrap()
        .1;
    let runtime = ProcessRuntime::local_default();
    let output = root.path().join("enhanced.png");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let result = local_enhance::enhance(
        &runtime,
        &config,
        Input {
            source: &source,
            source_hash: &hash,
            factor: 2,
        },
        &output,
        &CancellationToken::new(),
        Duration::from_secs(60),
        &tx,
    )
    .await
    .unwrap();
    assert_eq!((result.width, result.height), (514, 18));
    assert_eq!(result.source_hash, hash);
    let pixels = image::open(&output).unwrap();
    assert_eq!((pixels.width(), pixels.height()), (514, 18));
    let mut progress = vec![];
    while let Ok(value) = rx.try_recv() {
        progress.push(value);
    }
    assert_eq!(progress.last(), Some(&100.));
    assert!(progress.iter().any(|p| *p > 0. && *p < 100.));
    assert!(progress.windows(2).all(|p| p[0] <= p[1]));
    let bytes = std::fs::read(&output).unwrap();
    assert!(local_enhance::enhance(
        &runtime,
        &config,
        Input {
            source: &source,
            source_hash: &hash,
            factor: 2
        },
        &output,
        &CancellationToken::new(),
        Duration::from_secs(60),
        &tx
    )
    .await
    .is_err());
    assert_eq!(std::fs::read(&output).unwrap(), bytes);
    for mode in ["cancel", "changed", "deadline", "wrong_hash"] {
        let output = root.path().join(format!("{mode}.png"));
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let path = source.clone();
        let observer = tokio::spawn(async move {
            if rx.recv().await.is_some() {
                if mode == "cancel" {
                    signal.cancel()
                } else if mode == "changed" {
                    tokio::fs::write(path, b"changed during inference")
                        .await
                        .unwrap();
                }
            }
        });
        let limit = if mode == "deadline" {
            Duration::from_millis(1)
        } else {
            Duration::from_secs(60)
        };
        let expected = if mode == "wrong_hash" {
            "invalid"
        } else {
            &hash
        };
        let result = local_enhance::enhance(
            &runtime,
            &config,
            Input {
                source: &source,
                source_hash: expected,
                factor: 2,
            },
            &output,
            &cancel,
            limit,
            &tx,
        )
        .await;
        drop(tx);
        observer.await.unwrap();
        assert!(result.is_err(), "{mode} must refuse");
        assert!(!output.exists(), "{mode} must not publish");
        std::fs::write(&source, &original).unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert!(std::fs::read_dir(root.path()).unwrap().all(|item| !item
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".enhance-")));
}
