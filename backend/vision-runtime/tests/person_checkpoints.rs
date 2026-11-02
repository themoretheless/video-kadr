//! Opt-in real-model qualification for native completed-frame receipts.
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn private_dir(path: &Path) {
    std::fs::create_dir(path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
}
fn command(request: &Path, family: Option<&str>) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_video-kadr-vision-runtime"));
    cmd.arg(if family.is_some() {
        "--cutout-sequence"
    } else {
        "--person-sequence"
    })
    .arg(request);
    cmd
}
fn request(
    root: &Path,
    name: &str,
    model: &str,
    family: Option<&str>,
    pixels: &[u8],
    frames: usize,
    checkpoint: bool,
) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let directory = root.join(name);
    private_dir(&directory);
    let input = directory.join("input.rgba");
    std::fs::write(&input, pixels).unwrap();
    let output = directory.join("output.rgba");
    let masks = directory.join("masks.raw");
    let checkpoints = directory.join("checkpoints");
    let mut value = serde_json::json!({"schemaVersion":if family.is_some(){10}else{7},"model":model,"runtimeLibrary":std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap(),"input":input,"output":output,"maskOutput":masks,"sourceSha256":hash(pixels),"width":37,"height":23,"frames":frames});
    if let Some(family) = family {
        value["family"] = serde_json::json!(family);
    }
    if checkpoint {
        private_dir(&checkpoints);
        value["checkpointOutput"] = serde_json::json!(checkpoints);
    }
    let request = directory.join("request.json");
    std::fs::write(&request, serde_json::to_vec(&value).unwrap()).unwrap();
    (request, output, masks, checkpoints)
}
fn verify(root: &Path, pixels: &[u8], ordinal: usize, total: usize) -> (Vec<u8>, Vec<u8>) {
    let directory = root.join(ordinal.to_string());
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("receipt.json")).unwrap()).unwrap();
    let rgba = std::fs::read(directory.join("frame.rgba")).unwrap();
    let mask = std::fs::read(directory.join("mask.raw")).unwrap();
    assert_eq!(receipt["schemaVersion"], 1);
    assert_eq!(receipt["ordinal"], ordinal);
    assert_eq!(receipt["context"]["total"], total);
    assert_eq!(receipt["context"]["sourceHash"], hash(pixels));
    let size = 37 * 23 * 4;
    assert_eq!(
        receipt["context"]["frameHash"],
        hash(&pixels[ordinal * size..(ordinal + 1) * size])
    );
    assert_eq!(receipt["outputHash"], hash(&rgba));
    assert_eq!(receipt["maskHash"], hash(&mask));
    assert_eq!(receipt["blank"], mask.iter().all(|b| *b < 10));
    assert!(!directory.join("receipt.pending").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for file in ["frame.rgba", "mask.raw", "receipt.json"] {
            assert_eq!(
                std::fs::metadata(directory.join(file))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    (rgba, mask)
}
#[test]
#[ignore = "requires pinned RVM/Selfie/ISNet models and local ONNX runtime"]
fn actual_checkpoints_match_complete_sequences_and_survive_killed_recurrent_run() {
    let root = Root(std::env::temp_dir().join(format!(
            "video-checkpoints-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    private_dir(&root.0);
    let frame_bytes = 37 * 23 * 4;
    let pixels: Vec<u8> = (0..4)
        .flat_map(|frame| {
            (0..37 * 23).flat_map(move |p| {
                [
                    ((p * 17 + frame * 31) % 256) as u8,
                    ((p * 37) % 256) as u8,
                    ((p + frame * 89) % 256) as u8,
                    (p % 256) as u8,
                ]
            })
        })
        .collect();
    let mut first_person = None;
    for (name, family, variable) in [
        ("person", None, "VIDEO_KADR_TEST_PERSON_MODEL"),
        ("selfie", Some("selfie"), "VIDEO_KADR_TEST_SELFIE_MODEL"),
        ("object", Some("object"), "VIDEO_KADR_TEST_OBJECT_MODEL"),
    ] {
        let model = std::env::var(variable).unwrap();
        let frames = if family.is_none() { 4 } else { 2 };
        let source = &pixels[..frames * frame_bytes];
        let (req, output, masks, checkpoints) =
            request(&root.0, name, &model, family, source, frames, true);
        let result = command(&req, family).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let metadata: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        let mut all_rgba = vec![];
        let mut all_masks = vec![];
        for ordinal in 0..frames {
            let (rgba, mask) = verify(&checkpoints, source, ordinal, frames);
            let receipt: serde_json::Value = serde_json::from_slice(
                &std::fs::read(checkpoints.join(ordinal.to_string()).join("receipt.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(receipt["context"]["modelHash"], metadata["modelHash"]);
            assert_eq!(receipt["context"]["runtimeHash"], metadata["runtimeHash"]);
            all_rgba.extend(rgba.clone());
            all_masks.extend(mask.clone());
            if family.is_none() && ordinal == 0 {
                first_person = Some((rgba, mask));
            }
        }
        assert_eq!(all_rgba, std::fs::read(output).unwrap());
        assert_eq!(all_masks, std::fs::read(masks).unwrap());
        // Checkpoint I/O must not alter recurrent inference state or frame ordering.
        if family.is_none() {
            let (req, output, masks, _) = request(
                &root.0,
                "person-without-checkpoints",
                &model,
                None,
                source,
                frames,
                false,
            );
            let result = command(&req, None).output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(all_rgba, std::fs::read(output).unwrap());
            assert_eq!(all_masks, std::fs::read(masks).unwrap());
        }
    }
    let frames = 256;
    let source = pixels[..frame_bytes].repeat(frames);
    let model = std::env::var("VIDEO_KADR_TEST_PERSON_MODEL").unwrap();
    let (req, output, masks, checkpoints) = request(
        &root.0,
        "killed-person",
        &model,
        None,
        &source,
        frames,
        true,
    );
    let mut child = command(&req, None)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else { break };
            if serde_json::from_str::<serde_json::Value>(&line)
                .ok()
                .is_some_and(|v| v["event"] == "checkpoint")
            {
                let _ = tx.send(());
            }
        }
    });
    let checkpoint = rx.recv_timeout(Duration::from_secs(60));
    let killed = child.kill();
    let status = child.wait().unwrap();
    reader.join().unwrap();
    checkpoint.unwrap();
    killed.unwrap();
    assert!(!status.success());
    assert!(!output.exists());
    assert!(!masks.exists());
    let prefix = verify(&checkpoints, &source, 0, frames);
    assert_eq!(Some(prefix), first_person);
    let completed = (0..frames)
        .take_while(|ordinal| {
            checkpoints
                .join(ordinal.to_string())
                .join("receipt.json")
                .is_file()
        })
        .count();
    assert!(completed > 0 && completed < frames);
    for ordinal in 0..completed {
        verify(&checkpoints, &source, ordinal, frames);
    }
    eprintln!("killed recurrent run retained {completed} verified frame checkpoints");
}
