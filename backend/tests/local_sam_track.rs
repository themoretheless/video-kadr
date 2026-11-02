use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::{
        local_sam::Config,
        local_sam_track::{self, Input},
    },
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[tokio::test]
#[ignore = "requires native SAM executable, pinned models/ORT and original sequence oracles"]
async fn actual_raw_tracking_host_identity_loss_cancel_and_publication() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        encoder: std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap().into(),
        decoder: std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap().into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let oracles = PathBuf::from(std::env::var("VIDEO_KADR_TEST_SAM_TRACK_ORACLE_DIR").unwrap());
    for case in ["37x23", "23x37", "80x80"] {
        let oracle = oracles.join(case);
        let expected: serde_json::Value =
            serde_json::from_slice(&std::fs::read(oracle.join("reference.json")).unwrap()).unwrap();
        let original = std::fs::read(oracle.join("source.rgba")).unwrap();
        let seed_bytes = std::fs::read(oracle.join("seed.gray")).unwrap();
        let modes: Vec<_> = if case == "37x23" {
            vec![
                "whole",
                "empty",
                "cancel",
                "source_changed",
                "seed_changed",
                "engine_changed",
                "occupied",
                "late_occupied",
                "deadline",
                "stale_source",
                "stale_seed",
            ]
        } else {
            vec!["whole"]
        };
        for mode in modes {
            let root = tempfile::tempdir().unwrap();
            let source = root.path().join("source.rgba");
            let seed = root.path().join("seed.gray");
            let target = root.path().join("tracked.masks");
            std::fs::write(&source, &original).unwrap();
            let seed_data = if mode == "empty" {
                vec![0; 65536]
            } else {
                seed_bytes.clone()
            };
            std::fs::write(&seed, &seed_data).unwrap();
            let source_hash = hash(&original);
            let seed_hash = hash(&seed_data);
            let bad = "0".repeat(64);
            let cancel = CancellationToken::new();
            let mut config = config.clone();
            if mode == "engine_changed" {
                let executable = root.path().join("native-vision");
                std::fs::copy(&config.executable, &executable).unwrap();
                config.executable = executable;
            }
            if mode == "occupied" {
                std::fs::write(&target, b"foreign output").unwrap();
            }
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let sp = source.clone();
            let mp = seed.clone();
            let tp = target.clone();
            let ep = config.executable.clone();
            let trigger = cancel.clone();
            let observer = tokio::spawn(async move {
                let mut maximum = 0.;
                let mut acted = false;
                while let Some(value) = rx.recv().await {
                    assert!(value >= maximum);
                    maximum = value;
                    if value > 0. && !acted {
                        acted = true;
                        match mode {
                            "cancel" => trigger.cancel(),
                            "source_changed" => std::fs::write(&sp, b"changed source").unwrap(),
                            "seed_changed" => std::fs::write(&mp, b"changed seed").unwrap(),
                            "engine_changed" => {
                                let replacement = ep.with_extension("replacement");
                                std::fs::write(&replacement, b"changed executable identity")
                                    .unwrap();
                                std::fs::rename(replacement, &ep).unwrap();
                            }
                            "late_occupied" => std::fs::write(&tp, b"foreign output").unwrap(),
                            _ => {}
                        }
                    }
                }
                (maximum, acted)
            });
            let result = local_sam_track::track(
                &ProcessRuntime::local_default(),
                &config,
                Input {
                    source: &source,
                    source_hash: if mode == "stale_source" {
                        &bad
                    } else {
                        &source_hash
                    },
                    seed: &seed,
                    seed_hash: if mode == "stale_seed" {
                        &bad
                    } else {
                        &seed_hash
                    },
                    width: expected["width"].as_u64().unwrap() as u32,
                    height: expected["height"].as_u64().unwrap() as u32,
                    frames: 4,
                    seed_frame: 1,
                },
                &target,
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
            let (maximum, acted) = observer.await.unwrap();
            if mode == "whole" || mode == "empty" {
                let result = result.unwrap();
                assert_eq!(maximum, 100.);
                let actual = std::fs::read(&target).unwrap();
                let reference = if mode == "empty" {
                    vec![0; 4 * 65536]
                } else {
                    std::fs::read(oracle.join("reference.masks")).unwrap()
                };
                assert_eq!(actual, reference, "{case}/{mode}");
                assert_eq!(result.output_hash, hash(&actual));
                assert_eq!(
                    result.tracked_frames,
                    if mode == "empty" {
                        1
                    } else {
                        expected["trackedFrames"].as_u64().unwrap()
                    }
                );
                assert_eq!(
                    result.forward_lost_at,
                    if mode == "empty" {
                        Some(2)
                    } else {
                        expected["forwardLostAt"].as_u64()
                    }
                );
                assert_eq!(
                    result.backward_lost_at,
                    if mode == "empty" {
                        Some(0)
                    } else {
                        expected["backwardLostAt"].as_u64()
                    }
                );
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    assert_eq!(
                        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                        0o600
                    );
                }
            } else {
                assert!(result.is_err(), "{case}/{mode}");
                if [
                    "cancel",
                    "source_changed",
                    "seed_changed",
                    "engine_changed",
                    "late_occupied",
                ]
                .contains(&mode)
                {
                    assert!(acted, "{mode}");
                }
                if mode == "occupied" || mode == "late_occupied" {
                    assert_eq!(std::fs::read(&target).unwrap(), b"foreign output");
                } else {
                    assert!(!target.exists(), "{mode}");
                }
            }
            assert!(
                !std::fs::read_dir(root.path()).unwrap().any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".sam-track-")),
                "{case}/{mode}: owned staging leak"
            );
        }
    }
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires pinned SAM model/runtime inventory; uses an adversarial child contract"]
async fn rejects_forged_child_metadata_and_inconsistent_region_streams() {
    use std::os::unix::fs::PermissionsExt;
    for mode in [
        "valid",
        "seed_plane",
        "overlap",
        "backward_overlap",
        "loss_tail",
        "counter",
        "loss_bounds",
        "shape",
        "trailing",
        "hash",
        "duplicate",
        "unknown",
        "unknown_then_valid",
        "missing",
    ] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.rgba");
        let seed = root.path().join("seed.gray");
        let output = root.path().join("output.masks");
        let executable = root.path().join("child.py");
        let rgba = vec![255u8; 2 * 2 * 4 * 4];
        let seed_bytes = vec![255u8; 65536];
        std::fs::write(&source, &rgba).unwrap();
        std::fs::write(&seed, &seed_bytes).unwrap();
        let script=r#"#!/usr/bin/python3
import hashlib,json,os,sys
from pathlib import Path
mode='%MODE%'
q=json.loads(Path(sys.argv[2]).read_text())
def sha(path):
    h=hashlib.sha256()
    with open(path,'rb') as f:
        for b in iter(lambda:f.read(65536),b''):h.update(b)
    return h.hexdigest()
data=bytearray(Path(q['seed']).read_bytes()*q['frames'])
m=dict(schemaVersion=9,width=q['width'],height=q['height'],frames=q['frames'],seedFrame=q['seedFrame'],maskWidth=256,maskHeight=256,maskBytes=len(data),trackedFrames=q['frames'],forwardLostAt=None,backwardLostAt=None,sourceHash=q['sourceSha256'],seedHash=q['seedSha256'],encoderHash=sha(q['encoder']),decoderHash=sha(q['decoder']),runtimeHash=sha(q['runtimeLibrary']))
if mode=='seed_plane':data[65536]=0
if mode=='overlap':data[2*65536:3*65536]=bytes(65536)
if mode=='backward_overlap':data[:65536]=bytes(65536)
if mode=='loss_tail':m.update(forwardLostAt=2,trackedFrames=2)
if mode=='counter':m['trackedFrames']=3
if mode=='loss_bounds':m['backwardLostAt']=1
if mode=='shape':m['maskWidth']=255
if mode=='trailing':data.append(0)
m['outputHash']=hashlib.sha256(data).hexdigest()
if mode=='hash':m['outputHash']='0'*64
if mode=='unknown':m['untrusted']=True
fd=os.open(q['output'],os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
with os.fdopen(fd,'wb') as f:f.write(data)
if mode=='unknown_then_valid':print(json.dumps(dict(m,untrusted=True)))
if mode!='missing':print(json.dumps(m))
if mode=='duplicate':print(json.dumps(m))
"#.replace("%MODE%",mode);
        std::fs::write(&executable, script).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = Config {
            executable,
            encoder: std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap().into(),
            decoder: std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap().into(),
            runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
                .unwrap()
                .into(),
        };
        let source_hash = hash(&rgba);
        let seed_hash = hash(&seed_bytes);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let result = local_sam_track::track(
            &ProcessRuntime::local_default(),
            &config,
            Input {
                source: &source,
                source_hash: &source_hash,
                seed: &seed,
                seed_hash: &seed_hash,
                width: 2,
                height: 2,
                frames: 4,
                seed_frame: 1,
            },
            &output,
            &CancellationToken::new(),
            Duration::from_secs(30),
            &tx,
        )
        .await;
        drop(tx);
        if mode == "valid" {
            result.unwrap();
            assert_eq!(std::fs::read(&output).unwrap(), vec![255; 4 * 65536]);
        } else {
            assert!(result.is_err(), "{mode}");
            assert!(!output.exists(), "{mode}");
            while let Some(value) = rx.recv().await {
                assert!(value < 100., "{mode}: reported success");
            }
        }
        assert!(
            !std::fs::read_dir(root.path()).unwrap().any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".sam-track-")),
            "{mode}: owned staging leak"
        );
    }
}
