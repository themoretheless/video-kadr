use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::local_sam::{self, Config, Input},
};
#[test]
fn prompts_are_finite_bounded_and_nonempty() {
    for points in [
        vec![],
        vec![[f64::NAN, 0.]],
        vec![[f64::INFINITY, 0.]],
        vec![[1.6, 0.]],
        vec![[0., 0.]; 65537],
    ] {
        assert!(local_sam::validate_points(&points).is_err());
    }
    local_sam::validate_points(&[[-0.5, 1.5], [0.5, 0.5]]).unwrap();
}
#[tokio::test]
#[ignore = "requires native vision executable, pinned SlimSAM encoder/decoder and host ORT"]
async fn actual_region_is_private_verified_and_refuses_cancel_mutation_or_replacement() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        encoder: std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap().into(),
        decoder: std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap().into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let runtime = ProcessRuntime::local_default();
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.png");
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
    let source_bytes = std::fs::read(&source).unwrap();
    let hash = format!("{:x}", Sha256::digest(&source_bytes));
    let points = [[0.5, 0.5]];
    let output = root.path().join("region.png");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let result = local_sam::region(
        &runtime,
        &config,
        Input {
            source: &source,
            source_hash: &hash,
            points: &points,
        },
        &output,
        &CancellationToken::new(),
        Duration::from_secs(60),
        &tx,
    )
    .await
    .unwrap();
    assert_eq!(
        (
            result.width,
            result.height,
            result.mask_width,
            result.mask_height
        ),
        (37, 23, 256, 256)
    );
    assert_eq!(result.source_hash, hash);
    assert_eq!(result.encoder_hash, local_sam::ENCODER_HASH);
    assert_eq!(result.decoder_hash, local_sam::DECODER_HASH);
    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(result.output_hash, format!("{:x}", Sha256::digest(&bytes)));
    let mask = image::load_from_memory(&bytes).unwrap().to_luma8();
    assert_eq!(
        result.mask_hash,
        format!("{:x}", Sha256::digest(mask.as_raw()))
    );
    assert!(mask.as_raw().windows(2).any(|p| p[0] != p[1]));
    let mut progress = vec![];
    while let Ok(p) = rx.try_recv() {
        progress.push(p);
    }
    assert_eq!(progress.last(), Some(&100.));
    assert!(progress.windows(2).all(|p| p[0] <= p[1]));
    assert!(local_sam::region(
        &runtime,
        &config,
        Input {
            source: &source,
            source_hash: &hash,
            points: &points
        },
        &output,
        &CancellationToken::new(),
        Duration::from_secs(60),
        &tx
    )
    .await
    .is_err());
    assert_eq!(std::fs::read(&output).unwrap(), bytes);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    for mode in ["cancel", "source", "occupied", "deadline", "stale"] {
        let output = root.path().join(format!("{mode}-result.png"));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        let path = source.clone();
        let occupied = output.clone();
        let watcher = tokio::spawn(async move {
            if rx.recv().await.is_some() {
                match mode {
                    "cancel" => signal.cancel(),
                    "source" => tokio::fs::write(path, b"changed").await.unwrap(),
                    "occupied" => tokio::fs::write(occupied, b"existing").await.unwrap(),
                    _ => {}
                }
            }
        });
        let stale = "0".repeat(64);
        let expected = if mode == "stale" { &stale } else { &hash };
        let result = local_sam::region(
            &runtime,
            &config,
            Input {
                source: &source,
                source_hash: expected,
                points: &points,
            },
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
        watcher.await.unwrap();
        assert!(result.is_err(), "{mode}");
        if mode == "occupied" {
            assert_eq!(std::fs::read(&output).unwrap(), b"existing");
        } else {
            assert!(!output.exists(), "{mode}");
        }
        std::fs::write(&source, &source_bytes).unwrap();
    }
    let bad_model = root.path().join("bad.onnx");
    std::fs::write(&bad_model, b"invalid").unwrap();
    assert!(local_sam::identity(&Config {
        encoder: bad_model,
        ..config
    })
    .await
    .is_err());
    assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".sam-region-")));
}
