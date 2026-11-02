use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    domain::{composition::FrameRateRatio, video_region::Attachment},
    process_control::ProcessRuntime,
    services::{
        local_person_correct::{self, FrameCorrection, Input},
        local_person_regions::{Context, RegionInput},
    },
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn fixture(
    root: &std::path::Path,
    geometry: &str,
) -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<FrameCorrection>, Attachment) {
    let folder = root.join(geometry);
    let request: Value =
        serde_json::from_slice(&std::fs::read(folder.join("valid.json")).unwrap()).unwrap();
    let source = std::fs::read(request["input"].as_str().unwrap()).unwrap();
    let auto = std::fs::read(request["mask"].as_str().unwrap()).unwrap();
    let regions = std::fs::read(request["regions"][0]["input"].as_str().unwrap()).unwrap();
    let mut attachment = request["regions"][0]["attachment"].clone();
    for (key, value) in [
        ("width", request["width"].clone()),
        ("height", request["height"].clone()),
        ("frames", json!(4)),
        ("frameRate", json!({"numerator":4,"denominator":1})),
        ("seedFrame", json!(1)),
        ("trackedFrames", json!(4)),
        ("forwardLostAt", Value::Null),
        ("backwardLostAt", Value::Null),
    ] {
        attachment[key] = value;
    }
    (
        source,
        auto,
        regions,
        serde_json::from_value(request["corrections"].clone()).unwrap(),
        serde_json::from_value(attachment).unwrap(),
    )
}
#[tokio::test]
#[ignore = "requires native vision executable and retained independent scalar region replay oracles; no models/ORT"]
async fn actual_tracked_replay_hash_clock_cancel_mutation_and_atomic_publication() {
    let binary = PathBuf::from(std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap());
    let oracle = PathBuf::from(std::env::var("VIDEO_KADR_TEST_PERSON_REGIONS_ORACLE_DIR").unwrap());
    for geometry in ["37x23", "23x37", "80x80"] {
        let (source, auto, regions, edits, attachment) = fixture(&oracle, geometry);
        let modes = if geometry == "37x23" {
            vec![
                "valid",
                "cancel",
                "source",
                "auto",
                "region",
                "executable",
                "occupied",
                "late-output",
                "late-mask",
                "deadline",
                "plane",
                "provenance",
                "clock",
                "encoded-source",
                "duplicate",
                "unused",
                "count",
            ]
        } else {
            vec!["valid"]
        };
        for mode in modes {
            let root = tempfile::tempdir().unwrap();
            let input = root.path().join("source.rgba");
            let masks = root.path().join("auto.gray");
            let raw = root.path().join("region.gray");
            let output = root.path().join("output.rgba");
            let mask_output = root.path().join("output.gray");
            std::fs::write(&input, &source).unwrap();
            std::fs::write(&masks, &auto).unwrap();
            std::fs::write(&raw, &regions).unwrap();
            let mut bound = attachment.clone();
            let mut edits = edits.clone();
            if mode == "plane" {
                edits[0].correction.strokes[0]
                    .region
                    .as_mut()
                    .unwrap()
                    .mask_hash = "0".repeat(64);
            }
            if mode == "provenance" {
                bound.runtime_hash = "d".repeat(64);
            }
            if mode == "unused" {
                edits.clear();
            }
            if mode == "occupied" {
                std::fs::write(&output, b"foreign").unwrap();
            }
            let mut streams = vec![RegionInput {
                path: &raw,
                attachment: &bound,
            }];
            if mode == "duplicate" {
                streams.push(RegionInput {
                    path: &raw,
                    attachment: &bound,
                });
            }
            let encoded = if mode == "encoded-source" {
                "0".repeat(64)
            } else {
                attachment.source_hash.clone()
            };
            let context = Context {
                encoded_source_hash: &encoded,
                frame_rate: FrameRateRatio {
                    numerator: if mode == "clock" { 5 } else { 4 },
                    denominator: 1,
                },
                regions: &streams,
            };
            let executable = if mode == "executable" {
                let path = root.path().join("vision");
                std::fs::copy(&binary, &path).unwrap();
                path
            } else {
                binary.clone()
            };
            let cancel = CancellationToken::new();
            let trigger = cancel.clone();
            let sp = input.clone();
            let mp = masks.clone();
            let rp = raw.clone();
            let ep = executable.clone();
            let op = output.clone();
            let mop = mask_output.clone();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<f64>();
            let observer = tokio::spawn(async move {
                let mut acted = false;
                let mut maximum = 0f64;
                while let Some(value) = rx.recv().await {
                    assert!(value >= maximum);
                    maximum = value;
                    if value > 0. && !acted {
                        acted = true;
                        match mode {
                            "cancel" => trigger.cancel(),
                            "source" => std::fs::write(&sp, b"changed").unwrap(),
                            "auto" => std::fs::write(&mp, b"changed").unwrap(),
                            "region" => std::fs::write(&rp, b"changed").unwrap(),
                            "executable" => {
                                let replacement = ep.with_extension("new");
                                std::fs::copy(&ep, &replacement).unwrap();
                                use std::io::Write;
                                std::fs::OpenOptions::new()
                                    .append(true)
                                    .open(&replacement)
                                    .unwrap()
                                    .write_all(b"changed")
                                    .unwrap();
                                std::fs::rename(replacement, &ep).unwrap();
                            }
                            "late-output" => std::fs::write(&op, b"foreign").unwrap(),
                            "late-mask" => std::fs::write(&mop, b"foreign").unwrap(),
                            _ => {}
                        }
                    }
                }
                maximum
            });
            let result = local_person_correct::correct_with_regions(
                &ProcessRuntime::local_default(),
                &executable,
                Input {
                    source: &input,
                    source_hash: &hash(&source),
                    mask: &masks,
                    mask_hash: &hash(&auto),
                    mask_destination: Some(&mask_output),
                    width: attachment.width,
                    height: attachment.height,
                    frames: if mode == "count" { u64::MAX } else { 4 },
                    corrections: &edits,
                },
                &context,
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
            let maximum = observer.await.unwrap();
            if mode == "valid" {
                let meta = result.unwrap();
                assert_eq!(
                    std::fs::read(&output).unwrap(),
                    std::fs::read(oracle.join(geometry).join("expected.rgba")).unwrap()
                );
                assert_eq!(
                    std::fs::read(&mask_output).unwrap(),
                    std::fs::read(oracle.join(geometry).join("expected.gray")).unwrap()
                );
                assert_eq!(
                    meta.region_hashes.get("tracked"),
                    Some(&attachment.mask_hash)
                );
                assert_eq!(meta.region_source_hash, Some(encoded));
                assert_eq!(maximum, 100.);
            } else {
                assert!(result.is_err(), "{geometry}/{mode}");
                if ["occupied", "late-output"].contains(&mode) {
                    assert_eq!(std::fs::read(&output).unwrap(), b"foreign");
                } else {
                    assert!(!output.exists(), "{mode}");
                }
                if mode == "late-mask" {
                    assert_eq!(std::fs::read(&mask_output).unwrap(), b"foreign");
                } else {
                    assert!(!mask_output.exists(), "{mode}");
                }
            }
            assert!(
                std::fs::read_dir(root.path()).unwrap().all(|e| !e
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".person-correct-")),
                "{mode}"
            );
        }
    }
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires retained scalar region oracles; controlled child contract, no model/ORT"]
async fn strict_child_metadata_and_kill_reap_before_staging_cleanup() {
    use std::os::unix::fs::PermissionsExt;
    let oracle = PathBuf::from(std::env::var("VIDEO_KADR_TEST_PERSON_REGIONS_ORACLE_DIR").unwrap());
    let (source, auto, regions, edits, attachment) = fixture(&oracle, "37x23");
    for mode in [
        "valid",
        "missing",
        "hash",
        "source",
        "duplicate",
        "unknown",
        "unknown-then-valid",
        "cancel",
        "timeout",
        "legacy",
    ] {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("source");
        let base = root.path().join("auto");
        let raw = root.path().join("regions");
        let executable = root.path().join("child");
        let config = root.path().join("config.json");
        let pid_path = root.path().join("pid");
        let output = root.path().join("out");
        let masks = root.path().join("out.masks");
        std::fs::write(&input, &source).unwrap();
        std::fs::write(&base, &auto).unwrap();
        std::fs::write(&raw, &regions).unwrap();
        std::fs::write(
            &config,
            serde_json::to_vec(&json!({"mode":mode,"oracle":oracle.join("37x23"),"pid":pid_path}))
                .unwrap(),
        )
        .unwrap();
        let script=r#"#!/usr/bin/python3
import json,pathlib,sys,os,time
c=json.loads(pathlib.Path(CONFIG).read_text());q=json.loads(pathlib.Path(sys.argv[2]).read_text());mode=c['mode']
if mode in ['cancel','timeout']:
 pathlib.Path(c['pid']).write_text(str(os.getpid()))
 print(json.dumps({'event':'progress','completed':1,'total':4}),file=sys.stderr,flush=True)
 time.sleep(60)
else:
 folder=pathlib.Path(c['oracle']);meta=json.loads((folder/'metadata.json').read_text())
 for key,name in [('output','expected.rgba'),('maskOutput','expected.gray')]:
  with open(q[key],'xb') as f:f.write((folder/name).read_bytes())
  os.chmod(q[key],0o600)
 if mode=='missing':meta.pop('regionHashes');meta.pop('regionSourceHash')
 if mode=='hash':meta['regionHashes']={'tracked':'0'*64}
 if mode=='source':meta['regionSourceHash']='0'*64
 if mode in ['unknown','unknown-then-valid']:print(json.dumps(dict(meta,extra=True)),flush=True)
 if mode!='unknown':print(json.dumps(meta),flush=True)
 if mode=='duplicate':print(json.dumps(meta),flush=True)
"#.replace("CONFIG",&serde_json::to_string(config.to_str().unwrap()).unwrap());
        std::fs::write(&executable, script).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let streams = [RegionInput {
            path: &raw,
            attachment: &attachment,
        }];
        let context = Context {
            encoded_source_hash: &attachment.source_hash,
            frame_rate: FrameRateRatio {
                numerator: 4,
                denominator: 1,
            },
            regions: &streams,
        };
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<f64>();
        let observer = tokio::spawn(async move {
            while let Some(value) = rx.recv().await {
                if mode == "cancel" && value > 0. {
                    trigger.cancel();
                }
            }
        });
        let source_hash = hash(&source);
        let mask_hash = hash(&auto);
        let input = Input {
            source: &input,
            source_hash: &source_hash,
            mask: &base,
            mask_hash: &mask_hash,
            mask_destination: Some(&masks),
            width: 37,
            height: 23,
            frames: 4,
            corrections: &edits,
        };
        let started = std::time::Instant::now();
        let result = if mode == "legacy" {
            local_person_correct::correct(
                &ProcessRuntime::local_default(),
                &executable,
                input,
                &output,
                &cancel,
                Duration::from_secs(2),
                &tx,
            )
            .await
        } else {
            local_person_correct::correct_with_regions(
                &ProcessRuntime::local_default(),
                &executable,
                input,
                &context,
                &output,
                &cancel,
                Duration::from_secs(2),
                &tx,
            )
            .await
        };
        drop(tx);
        observer.await.unwrap();
        if mode == "valid" {
            result.unwrap();
            assert_eq!(
                std::fs::read(&output).unwrap(),
                std::fs::read(oracle.join("37x23/expected.rgba")).unwrap()
            );
        } else {
            assert!(result.is_err(), "{mode}");
            assert!(!output.exists(), "{mode}");
            assert!(!masks.exists(), "{mode}");
        }
        if ["cancel", "timeout"].contains(&mode) {
            assert!(started.elapsed() < Duration::from_secs(5));
            let pid = std::fs::read_to_string(&pid_path).unwrap();
            assert!(
                !std::process::Command::new("/bin/kill")
                    .args(["-0", &pid])
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "owned child remains live after {mode}"
            );
        }
        assert!(
            std::fs::read_dir(root.path()).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".person-correct-")),
            "{mode}"
        );
    }
}
