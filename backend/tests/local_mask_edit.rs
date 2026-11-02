use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::{
        local_cutout::Metadata,
        local_mask_edit::{self, Correction, Input, Stroke, Tool},
    },
};
#[test]
fn authored_corrections_are_bounded_and_strict() {
    let mut value = Correction {
        custom: true,
        strokes: vec![Stroke {
            tool: Tool::Brush,
            size: 0.2,
            points: vec![[0.5, 0.5]],
            at: None,
            region: None,
        }],
        feather: 0.01,
    };
    value.validate().unwrap();
    let old = value.hash().unwrap();
    value.strokes[0].tool = Tool::Eraser;
    assert_ne!(old, value.hash().unwrap());
    value.strokes[0].points[0][0] = f64::NAN;
    assert!(value.validate().is_err());
    assert!(serde_json::from_value::<Correction>(
        serde_json::json!({"custom":true,"strokes":[],"feather":0.,"model":"untrusted"})
    )
    .is_err());
}
#[tokio::test]
#[ignore = "requires native vision executable only; no model or ORT library"]
async fn actual_cached_correction_restores_original_alpha_and_refuses_mutation() {
    let executable =
        std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap());
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.png");
    let mask = root.path().join("mask.png");
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
    image::GrayImage::from_pixel(512, 512, image::Luma([128]))
        .save(&mask)
        .unwrap();
    let source_bytes = std::fs::read(&source).unwrap();
    let mask_bytes = std::fs::read(&mask).unwrap();
    let original = Metadata {
        schema_version: 3,
        width: 37,
        height: 23,
        mask_width: 512,
        mask_height: 512,
        blank_mask: false,
        source_hash: format!("{:x}", Sha256::digest(&source_bytes)),
        mask_file_hash: Some(format!("{:x}", Sha256::digest(&mask_bytes))),
        mask_hash: format!("{:x}", Sha256::digest(vec![128; 512 * 512])),
        model_hash: String::new(),
        runtime_hash: String::new(),
        output_hash: String::new(),
    };
    let runtime = ProcessRuntime::local_default();
    let output = root.path().join("restored.png");
    let correction = Correction {
        custom: true,
        feather: 0.,
        strokes: (0..5)
            .map(|i| Stroke {
                tool: Tool::Brush,
                size: 0.5,
                points: vec![[0., i as f64 / 4.], [1., i as f64 / 4.]],
                at: Some(0.),
                region: None,
            })
            .collect(),
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let result = local_mask_edit::correct(
        &runtime,
        &executable,
        Input {
            source: &source,
            mask: &mask,
            original: &original,
            correction: &correction,
        },
        &output,
        &CancellationToken::new(),
        Duration::from_secs(30),
        &tx,
    )
    .await
    .unwrap();
    assert_eq!(result.correction_hash, correction.hash().unwrap());
    assert_eq!(
        image::open(&output).unwrap().to_rgba8(),
        image::open(&source).unwrap().to_rgba8()
    );
    let mut progress = vec![];
    while let Ok(p) = rx.try_recv() {
        progress.push(p);
    }
    assert_eq!(progress.last(), Some(&100.));
    assert!(progress.windows(2).all(|p| p[0] <= p[1]));
    let before = std::fs::read(&output).unwrap();
    assert!(local_mask_edit::correct(
        &runtime,
        &executable,
        Input {
            source: &source,
            mask: &mask,
            original: &original,
            correction: &correction
        },
        &output,
        &CancellationToken::new(),
        Duration::from_secs(30),
        &tx
    )
    .await
    .is_err());
    assert_eq!(std::fs::read(&output).unwrap(), before);
    let slow = Correction {
        custom: true,
        strokes: vec![Stroke {
            tool: Tool::Brush,
            size: 0.5,
            points: vec![[0.5, 0.5]; 500],
            at: None,
            region: None,
        }],
        feather: 0.,
    };
    for mode in ["cancel", "source", "mask", "deadline", "occupied", "stale"] {
        let output = root.path().join(format!("{mode}-result.png"));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        let source_path = source.clone();
        let mask_path = mask.clone();
        let occupied = output.clone();
        let watcher = tokio::spawn(async move {
            if rx.recv().await.is_some() {
                match mode {
                    "cancel" => signal.cancel(),
                    "source" => tokio::fs::write(source_path, b"changed").await.unwrap(),
                    "mask" => tokio::fs::write(mask_path, b"changed").await.unwrap(),
                    "occupied" => tokio::fs::write(occupied, b"existing").await.unwrap(),
                    _ => {}
                }
            }
        });
        let mut expected = original.clone();
        if mode == "stale" {
            expected.source_hash = "0".repeat(64);
        }
        let result = local_mask_edit::correct(
            &runtime,
            &executable,
            Input {
                source: &source,
                mask: &mask,
                original: &expected,
                correction: &slow,
            },
            &output,
            &cancel,
            if mode == "deadline" {
                Duration::from_millis(1)
            } else {
                Duration::from_secs(30)
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
        std::fs::write(&mask, &mask_bytes).unwrap();
    }
    assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".mask-edit-")));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(root.path().join("restored.png"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
