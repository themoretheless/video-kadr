use std::time::Duration;
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::{
        local_person::Config,
        local_person_sequence::{self, Input},
    },
};
#[tokio::test]
#[ignore = "requires native RVM executable/model/runtime and original sequence oracle"]
async fn actual_sequence_masks_rgba_cancel_mutation_and_atomic_publication() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        model: std::env::var("VIDEO_KADR_TEST_PERSON_MODEL")
            .unwrap()
            .into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let oracle =
        std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_PERSON_ORACLE_DIR").unwrap());
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("input.rgba");
    let original = std::fs::read(oracle.join("input.rgba")).unwrap();
    std::fs::write(&source, &original).unwrap();
    let hash = video_kadr_backend::services::local_speech::fingerprint_model(&source)
        .await
        .unwrap()
        .1;
    let runtime = ProcessRuntime::local_default();
    let output = root.path().join("person.rgba");
    let mask = root.path().join("person.masks");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let result = local_person_sequence::segment(
        &runtime,
        &config,
        Input {
            source: &source,
            source_hash: &hash,
            width: 37,
            height: 23,
            frames: 4,
            mask_destination: Some(&mask),
        },
        &output,
        &CancellationToken::new(),
        Duration::from_secs(60),
        &tx,
    )
    .await
    .unwrap();
    assert_eq!((result.width, result.height, result.frames), (37, 23, 4));
    assert_eq!((result.mask_width, result.mask_height), (512, 320));
    assert_eq!(
        std::fs::read(&output).unwrap(),
        std::fs::read(oracle.join("reference.rgba")).unwrap()
    );
    assert_eq!(
        std::fs::read(&mask).unwrap(),
        std::fs::read(oracle.join("reference.masks")).unwrap()
    );
    let mut progress = vec![];
    while let Ok(value) = rx.try_recv() {
        progress.push(value);
    }
    assert_eq!(progress.last(), Some(&100.));
    assert!(progress.windows(2).all(|v| v[0] <= v[1]));
    assert!(local_person_sequence::segment(
        &runtime,
        &config,
        Input {
            source: &source,
            source_hash: &hash,
            width: 37,
            height: 23,
            frames: 4,
            mask_destination: Some(&mask)
        },
        &output,
        &CancellationToken::new(),
        Duration::from_secs(60),
        &tx
    )
    .await
    .is_err());
    let no_mask = root.path().join("without-mask.rgba");
    local_person_sequence::segment(
        &runtime,
        &config,
        Input {
            source: &source,
            source_hash: &hash,
            width: 37,
            height: 23,
            frames: 4,
            mask_destination: None,
        },
        &no_mask,
        &CancellationToken::new(),
        Duration::from_secs(60),
        &tx,
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(&no_mask).unwrap(),
        std::fs::read(&output).unwrap()
    );
    for mode in [
        "cancel",
        "changed",
        "deadline",
        "wrong_hash",
        "wrong_count",
        "occupied",
        "mask_occupied",
    ] {
        let destination = root.path().join(format!("{mode}.rgba"));
        let masks = root.path().join(format!("{mode}.masks"));
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let path = source.clone();
        let bytes = original.clone();
        let occupied = destination.clone();
        let occupied_mask = masks.clone();
        let observer = tokio::spawn(async move {
            while let Some(value) = rx.recv().await {
                if value > 0. {
                    match mode {
                        "cancel" => token.cancel(),
                        "changed" => {
                            let mut value = bytes;
                            value[0] ^= 1;
                            std::fs::write(path, value).unwrap();
                        }
                        "occupied" => std::fs::write(occupied, b"occupied").unwrap(),
                        "mask_occupied" => std::fs::write(occupied_mask, b"occupied mask").unwrap(),
                        _ => {}
                    }
                    break;
                }
            }
        });
        let result = local_person_sequence::segment(
            &runtime,
            &config,
            Input {
                source: &source,
                source_hash: if mode == "wrong_hash" {
                    "invalid"
                } else {
                    &hash
                },
                width: 37,
                height: 23,
                frames: if mode == "wrong_count" { 3 } else { 4 },
                mask_destination: Some(&masks),
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
        if mode == "occupied" {
            assert_eq!(std::fs::read(&destination).unwrap(), b"occupied");
        } else {
            assert!(!destination.exists());
        }
        if mode == "mask_occupied" {
            assert_eq!(std::fs::read(&masks).unwrap(), b"occupied mask");
        } else {
            assert!(!masks.exists());
        }
        std::fs::write(&source, &original).unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [&output, &mask] {
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".person-sequence-")));
}

// Run the exact production module's private contract tests in this smaller
// integration target when macOS cannot map the monolithic lib-test binary.
pub use video_kadr_backend::services::local_person;
#[allow(dead_code)]
#[path = "../src/services/local_person_sequence_contract.rs"]
mod adapter_contract;
