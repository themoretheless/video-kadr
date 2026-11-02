use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::local_person_correct::{self, FrameCorrection, Input},
};
#[tokio::test]
#[ignore = "requires actual native executable and independent RVM sequence oracle; no model/ORT"]
async fn actual_model_free_replay_ordinal_alpha_cancel_mutation_and_no_replace() {
    let executable =
        std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap());
    let oracle =
        std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_PERSON_ORACLE_DIR").unwrap());
    let source = std::fs::read(oracle.join("input.rgba")).unwrap();
    let masks = std::fs::read(oracle.join("reference.masks")).unwrap();
    let reference = std::fs::read(oracle.join("reference.rgba")).unwrap();
    let hash = |b: &[u8]| format!("{:x}", Sha256::digest(b));
    let source_hash = hash(&source);
    let mask_hash = hash(&masks);
    let points: Vec<[f64; 2]> = (0..9)
        .flat_map(|y| (0..9).map(move |x| [x as f64 / 8., y as f64 / 8.]))
        .collect();
    let edit = |frame, tool| {
        serde_json::from_value::<FrameCorrection>(serde_json::json!({"frame":frame,"correction":{"custom":true,"strokes":[{"tool":tool,"size":0.5,"points":points}],"feather":0}})).unwrap()
    };
    let edits = vec![edit(1, "eraser"), edit(2, "brush")];
    for mode in [
        "identity",
        "corrected",
        "cancel",
        "source",
        "masks",
        "occupied-output",
        "occupied-mask",
        "deadline",
    ] {
        let root = tempfile::tempdir().unwrap();
        let input_path = root.path().join("source");
        let mask_path = root.path().join("auto");
        let output = root.path().join("out");
        let mask_output = root.path().join("out.masks");
        std::fs::write(&input_path, &source).unwrap();
        std::fs::write(&mask_path, &masks).unwrap();
        let cancel = CancellationToken::new();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let trigger = cancel.clone();
        let sp = input_path.clone();
        let mp = mask_path.clone();
        let op = output.clone();
        let om = mask_output.clone();
        let observer = tokio::spawn(async move {
            let mut acted = false;
            let mut max = 0f64;
            while let Some(value) = rx.recv().await {
                assert!(value >= max);
                max = value;
                if value > 0. && !acted {
                    acted = true;
                    match mode {
                        "cancel" => trigger.cancel(),
                        "source" => {
                            std::fs::write(&sp, b"changed").unwrap();
                        }
                        "masks" => {
                            std::fs::write(&mp, b"changed").unwrap();
                        }
                        "occupied-output" => {
                            std::fs::write(&op, b"occupied").unwrap();
                        }
                        "occupied-mask" => {
                            std::fs::write(&om, b"occupied").unwrap();
                        }
                        _ => {}
                    }
                }
            }
            max
        });
        let result = local_person_correct::correct(
            &ProcessRuntime::local_default(),
            &executable,
            Input {
                source: &input_path,
                source_hash: &source_hash,
                mask: &mask_path,
                mask_hash: &mask_hash,
                mask_destination: Some(&mask_output),
                width: 37,
                height: 23,
                frames: 4,
                corrections: if mode == "identity" { &[] } else { &edits },
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
        let max = observer.await.unwrap();
        if mode == "identity" || mode == "corrected" {
            let metadata = result.unwrap();
            assert_eq!(max, 100.);
            assert_eq!(metadata.source_mask_hash, mask_hash);
            assert_eq!(metadata.frames, 4);
            let rgba = std::fs::read(&output).unwrap();
            let got_masks = std::fs::read(&mask_output).unwrap();
            assert_eq!(metadata.output_hash, hash(&rgba));
            assert_eq!(metadata.mask_hash, hash(&got_masks));
            if mode == "identity" {
                assert_eq!(rgba, reference);
                assert_eq!(got_masks, masks);
            } else {
                let frame = 37 * 23 * 4;
                assert_eq!(&rgba[..frame], &reference[..frame]);
                assert_eq!(&rgba[3 * frame..], &reference[3 * frame..]);
                for i in (frame..2 * frame).step_by(4) {
                    assert_eq!(&rgba[i..i + 3], &source[i..i + 3]);
                    assert_eq!(rgba[i + 3], 0);
                }
                assert_eq!(&rgba[2 * frame..3 * frame], &source[2 * frame..3 * frame]);
            }
        } else {
            assert!(result.is_err(), "{mode}");
            if mode == "occupied-output" {
                assert_eq!(std::fs::read(&output).unwrap(), b"occupied");
            } else {
                assert!(!output.exists(), "{mode}");
            }
            if mode == "occupied-mask" {
                assert_eq!(std::fs::read(&mask_output).unwrap(), b"occupied");
            } else {
                assert!(!mask_output.exists(), "{mode}");
            }
        }
        assert!(!std::fs::read_dir(root.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".person-correct-")));
    }
}
