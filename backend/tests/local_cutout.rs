use std::time::Duration;
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::local_cutout::{self, Config, Input},
};
#[test]
fn runtime_configuration_requires_complete_absolute_paths() {
    assert!(Config::from_lookup(&mut |_| None).unwrap().is_none());
    assert!(Config::from_lookup(&mut |key| if key == "CUTOUT_BIN" {
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
        model: std::env::var("VIDEO_KADR_TEST_OBJECT_MODEL")
            .unwrap()
            .into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("synthetic.png");
    image::RgbaImage::from_fn(37, 23, |x, y| {
        let n = y * 37 + x;
        image::Rgba([
            (n % 151) as u8,
            ((n * 17) % 180) as u8,
            ((n * 31) % 199) as u8,
            (n % 256) as u8,
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
    let result = local_cutout::cutout(
        &runtime,
        &config,
        Input {
            mask_destination: None,
            source: &source,
            source_hash: &hash,
        },
        &output,
        &CancellationToken::new(),
        Duration::from_secs(60),
        &tx,
    )
    .await
    .unwrap();
    assert_eq!((result.width, result.height), (37, 23));
    assert_eq!(result.source_hash, hash);
    let pixels = image::open(&output).unwrap();
    assert_eq!((pixels.width(), pixels.height()), (37, 23));
    assert_eq!((result.mask_width, result.mask_height), (512, 512));
    assert!(!result.blank_mask);
    let original_pixels = image::open(&source).unwrap().into_rgba8();
    let output_pixels = pixels.into_rgba8();
    assert!(original_pixels
        .pixels()
        .zip(output_pixels.pixels())
        .all(|(a, b)| a.0[..3] == b.0[..3] && b.0[3] <= a.0[3]));
    assert!(original_pixels
        .pixels()
        .zip(output_pixels.pixels())
        .any(|(a, b)| b.0[3] < a.0[3]));

    let mut progress = vec![];
    while let Ok(value) = rx.try_recv() {
        progress.push(value);
    }
    assert_eq!(progress.last(), Some(&100.));
    assert!(progress.iter().any(|p| *p > 0. && *p < 100.));
    assert!(progress.windows(2).all(|p| p[0] <= p[1]));
    let bytes = std::fs::read(&output).unwrap();
    assert!(local_cutout::cutout(
        &runtime,
        &config,
        Input {
            mask_destination: None,
            source: &source,
            source_hash: &hash,
        },
        &output,
        &CancellationToken::new(),
        Duration::from_secs(60),
        &tx
    )
    .await
    .is_err());
    assert_eq!(std::fs::read(&output).unwrap(), bytes);
    for mode in ["cancel", "changed", "deadline", "wrong_hash", "occupied"] {
        let output = root.path().join(format!("{mode}.png"));
        let mask = root.path().join(format!("{mode}.mask.png"));
        let occupied = output.clone();
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let path = source.clone();
        let observer = tokio::spawn(async move {
            if rx.recv().await.is_some() {
                if mode == "cancel" {
                    signal.cancel()
                } else if mode == "occupied" {
                    tokio::fs::write(occupied, b"existing destination")
                        .await
                        .unwrap();
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
        let result = local_cutout::cutout(
            &runtime,
            &config,
            Input {
                mask_destination: Some(&mask),
                source: &source,
                source_hash: expected,
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
        if mode == "occupied" {
            assert_eq!(std::fs::read(&output).unwrap(), b"existing destination");
        } else {
            assert!(!output.exists(), "{mode} must not publish");
        }
        assert!(!mask.exists(), "{mode} must not leave a mask");
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
        .starts_with(".cutout-")));
}
