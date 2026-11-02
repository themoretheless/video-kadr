//! Actual replay of a retained learned SAM region; only native executable and PNGs.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    domain::cutout::{Correction, RegionAttachment, Stroke, Tool},
    process_control::ProcessRuntime,
    services::{
        local_cutout::Metadata,
        local_mask_edit::{self, Input, RegionInput},
    },
};
fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
#[tokio::test]
#[ignore = "requires native executable plus retained SAM source/region/report; no models or ORT"]
async fn actual_saved_region_replay_binds_source_prompts_and_rechecks_files() {
    let executable = PathBuf::from(std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap());
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.png");
    let region = root.path().join("region.png");
    let automatic = root.path().join("automatic.png");
    std::fs::copy(
        std::env::var("VIDEO_KADR_TEST_SAM_SOURCE").unwrap(),
        &source,
    )
    .unwrap();
    std::fs::copy(
        std::env::var("VIDEO_KADR_TEST_SAM_REGION").unwrap(),
        &region,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&region, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let report: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("VIDEO_KADR_TEST_SAM_REPORT").unwrap()).unwrap(),
    )
    .unwrap();
    let provenance = &report["path"]["metadata"];
    let source_bytes = std::fs::read(&source).unwrap();
    let region_bytes = std::fs::read(&region).unwrap();
    let original = image::open(&source).unwrap().to_rgba8();
    let points: Vec<_> = (0..40).map(|i| [i as f64 / 39., 0.3]).collect();
    assert_eq!(hash(&source_bytes), provenance["sourceHash"]);
    assert_eq!(hash(&region_bytes), provenance["outputHash"]);
    assert_eq!(
        hash(serde_json::to_vec(&points).unwrap()),
        provenance["promptHash"]
    );
    image::GrayImage::from_pixel(512, 512, image::Luma([128]))
        .save(&automatic)
        .unwrap();
    let attachment = RegionAttachment {
        source_id: "original".into(),
        mask_source_id: "learned-region".into(),
        source_hash: hash(&source_bytes),
        mask_hash: provenance["maskHash"].as_str().unwrap().into(),
        mask_file_hash: hash(&region_bytes),
        encoder_hash: provenance["encoderHash"].as_str().unwrap().into(),
        decoder_hash: provenance["decoderHash"].as_str().unwrap().into(),
        runtime_hash: provenance["runtimeHash"].as_str().unwrap().into(),
        prompt_hash: provenance["promptHash"].as_str().unwrap().into(),
        points: points.clone(),
    };
    attachment.validate().unwrap();
    let metadata = Metadata {
        schema_version: 3,
        width: original.width(),
        height: original.height(),
        mask_width: 512,
        mask_height: 512,
        blank_mask: false,
        source_hash: hash(&source_bytes),
        mask_file_hash: Some(hash(std::fs::read(&automatic).unwrap())),
        mask_hash: hash(vec![128; 512 * 512]),
        model_hash: "a".repeat(64),
        runtime_hash: "b".repeat(64),
        output_hash: "c".repeat(64),
    };
    let correction = Correction {
        custom: true,
        strokes: vec![Stroke {
            tool: Tool::SmartBrush,
            size: 0.005,
            points,
            at: None,
            region: Some(attachment.clone()),
        }],
        feather: 0.,
    };
    let runtime = ProcessRuntime::local_default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let learned = root.path().join("learned.png");
    let result = local_mask_edit::correct_with_regions(
        &runtime,
        &executable,
        Input {
            source: &source,
            mask: &automatic,
            original: &metadata,
            correction: &correction,
        },
        &[Some(RegionInput {
            path: &region,
            attachment: &attachment,
        })],
        &learned,
        &CancellationToken::new(),
        Duration::from_secs(30),
        &tx,
    )
    .await
    .unwrap();
    assert_eq!(
        result.region_hashes,
        Some(vec![Some(attachment.mask_hash.clone())])
    );
    assert_eq!(result.correction_hash, correction.hash().unwrap());
    let mut progress = Vec::new();
    while let Ok(p) = rx.try_recv() {
        progress.push(p);
    }
    assert_eq!(progress.last(), Some(&100.));
    assert!(progress.windows(2).all(|p| p[0] <= p[1]));
    let actual = image::open(&learned).unwrap().to_rgba8();
    assert_eq!(actual.dimensions(), original.dimensions());
    for (a, b) in actual.pixels().zip(original.pixels()) {
        assert_eq!(&a.0[..3], &b.0[..3]);
        assert!(a[3] <= b[3]);
    }
    let missing = root.path().join("missing-region.png");
    assert!(local_mask_edit::correct(
        &runtime,
        &executable,
        Input {
            source: &source,
            mask: &automatic,
            original: &metadata,
            correction: &correction
        },
        &missing,
        &CancellationToken::new(),
        Duration::from_secs(30),
        &tx
    )
    .await
    .is_err());
    assert!(!missing.exists());
    let mut legacy = correction.clone();
    legacy.strokes[0].region = None;
    let fallback = root.path().join("fallback.png");
    let old = local_mask_edit::correct(
        &runtime,
        &executable,
        Input {
            source: &source,
            mask: &automatic,
            original: &metadata,
            correction: &legacy,
        },
        &fallback,
        &CancellationToken::new(),
        Duration::from_secs(30),
        &tx,
    )
    .await
    .unwrap();
    assert!(old.region_hashes.is_none());
    assert_ne!(old.output_hash, result.output_hash);
    let null = root.path().join("null.png");
    let empty = local_mask_edit::correct_with_regions(
        &runtime,
        &executable,
        Input {
            source: &source,
            mask: &automatic,
            original: &metadata,
            correction: &legacy,
        },
        &[None],
        &null,
        &CancellationToken::new(),
        Duration::from_secs(30),
        &tx,
    )
    .await
    .unwrap();
    assert_eq!(empty.region_hashes, Some(vec![None]));
    assert_eq!(empty.output_hash, old.output_hash);
    for mode in [
        "source", "points", "tool", "count", "cancel", "region", "occupied", "deadline",
    ] {
        let output = root.path().join(format!("{mode}-result.png"));
        let mut stored = attachment.clone();
        let mut edit = correction.clone();
        if mode == "source" {
            stored.source_hash = "0".repeat(64);
        }
        if mode == "points" {
            stored.points[0][0] = 0.2;
            stored.prompt_hash = hash(serde_json::to_vec(&stored.points).unwrap());
        }
        if mode == "tool" {
            edit.strokes[0].tool = Tool::Brush;
        }
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        let path = region.clone();
        let occupied = output.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watcher = tokio::spawn(async move {
            if rx.recv().await.is_some() {
                match mode {
                    "cancel" => signal.cancel(),
                    "region" => tokio::fs::write(path, b"changed").await.unwrap(),
                    "occupied" => tokio::fs::write(occupied, b"existing").await.unwrap(),
                    _ => {}
                }
            }
        });
        let mut regions = vec![Some(RegionInput {
            path: &region,
            attachment: &stored,
        })];
        if mode == "count" {
            regions.push(None);
        }
        let result = local_mask_edit::correct_with_regions(
            &runtime,
            &executable,
            Input {
                source: &source,
                mask: &automatic,
                original: &metadata,
                correction: &edit,
            },
            &regions,
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
        std::fs::write(&region, &region_bytes).unwrap();
    }
    assert!(std::fs::read_dir(root.path()).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".mask-edit-")));
}
