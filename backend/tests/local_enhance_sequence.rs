use std::time::Duration;
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::{
        local_enhance::{self, Config},
        local_enhance_sequence::{self, Input},
    },
};
#[tokio::test]
#[ignore = "requires native learned enhancer, public model and ONNX Runtime"]
async fn actual_sequence_adapter_identity_progress_cancel_mutation_and_no_replace() {
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
    let source = root.path().join("frames.rgba");
    let frames: Vec<Vec<u8>> = (0..2u32)
        .map(|ordinal| {
            (0..9u32)
                .flat_map(|y| {
                    (0..257u32).flat_map(move |x| {
                        [
                            ((x * 13 + y * 7 + ordinal * 71) % 256) as u8,
                            ((x * 9 + y * 17 + ordinal * 37) % 256) as u8,
                            (80 + ordinal * 50) as u8,
                            255,
                        ]
                    })
                })
                .collect()
        })
        .collect();
    let original = frames.concat();
    std::fs::write(&source, &original).unwrap();
    let hash = video_kadr_backend::services::local_speech::fingerprint_model(&source)
        .await
        .unwrap()
        .1;
    let runtime = ProcessRuntime::local_default();
    let output = root.path().join("enhanced.rgba");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let result = local_enhance_sequence::enhance(
        &runtime,
        &config,
        Input {
            source: &source,
            source_hash: &hash,
            factor: 2,
            width: 257,
            height: 9,
            frames: 2,
        },
        &output,
        &CancellationToken::new(),
        Duration::from_secs(60),
        &tx,
    )
    .await
    .unwrap();
    assert_eq!(
        (result.width, result.height, result.frames, result.bytes),
        (514, 18, 2, 514 * 18 * 4 * 2)
    );
    let mut progress = vec![];
    while let Ok(v) = rx.try_recv() {
        progress.push(v);
    }
    assert_eq!(progress.last(), Some(&100.));
    assert!(progress.iter().any(|v| *v > 0. && *v < 100.));
    assert!(progress.windows(2).all(|v| v[0] <= v[1]));
    let actual = std::fs::read(&output).unwrap();
    let mut expected = vec![];
    for (ordinal, rgba) in frames.into_iter().enumerate() {
        let input = root.path().join(format!("frame-{ordinal}.png"));
        let target = root.path().join(format!("frame-{ordinal}-enhanced.png"));
        image::RgbaImage::from_raw(257, 9, rgba)
            .unwrap()
            .save(&input)
            .unwrap();
        let hash = video_kadr_backend::services::local_speech::fingerprint_model(&input)
            .await
            .unwrap()
            .1;
        local_enhance::enhance(
            &runtime,
            &config,
            local_enhance::Input {
                source: &input,
                source_hash: &hash,
                factor: 2,
            },
            &target,
            &CancellationToken::new(),
            Duration::from_secs(60),
            &tx,
        )
        .await
        .unwrap();
        expected.extend(image::open(target).unwrap().into_rgba8().into_raw());
    }
    assert_eq!(actual, expected);
    assert!(local_enhance_sequence::enhance(
        &runtime,
        &config,
        Input {
            source: &source,
            source_hash: &hash,
            factor: 2,
            width: 257,
            height: 9,
            frames: 2
        },
        &output,
        &CancellationToken::new(),
        Duration::from_secs(60),
        &tx
    )
    .await
    .is_err());
    assert_eq!(std::fs::read(&output).unwrap(), actual);
    for mode in ["cancel", "changed", "deadline", "wrong_hash", "wrong_count"] {
        let destination = root.path().join(format!("{mode}.rgba"));
        let cancel = CancellationToken::new();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let token = cancel.clone();
        let path = source.clone();
        let bytes = original.clone();
        let observer = tokio::spawn(async move {
            while let Some(value) = rx.recv().await {
                if value > 0. {
                    if mode == "cancel" {
                        token.cancel();
                    }
                    if mode == "changed" {
                        let mut changed = bytes.clone();
                        changed[0] ^= 1;
                        std::fs::write(&path, changed).unwrap();
                    }
                    break;
                }
            }
        });
        let result = local_enhance_sequence::enhance(
            &runtime,
            &config,
            Input {
                source: &source,
                source_hash: if mode == "wrong_hash" {
                    "invalid"
                } else {
                    &hash
                },
                factor: 2,
                width: 257,
                height: 9,
                frames: if mode == "wrong_count" { 3 } else { 2 },
            },
            &destination,
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
        assert!(!destination.exists(), "{mode} must not publish");
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
    assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".enhance-sequence-")));
}
