//! Host-controlled RVM runtime inventory; no caller model paths.
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
pub const MODEL_BYTES: u64 = 14_975_696;
pub const MODEL_HASH: &str = "88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub executable: PathBuf,
    pub model: PathBuf,
    pub runtime_library: PathBuf,
}
impl Config {
    pub fn from_lookup(lookup: &mut impl FnMut(&str) -> Option<String>) -> Result<Option<Self>> {
        let values: [Option<String>; 3] = ["PERSON_BIN", "PERSON_MODEL", "PERSON_ORT_LIBRARY"]
            .map(|key| lookup(key).filter(|v| !v.trim().is_empty()));
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        ensure!(
            values.iter().all(Option::is_some),
            "configure PERSON_BIN/PERSON_MODEL/PERSON_ORT_LIBRARY together"
        );
        let [executable, model, runtime_library] = values.map(|v| PathBuf::from(v.unwrap()));
        ensure!(
            [&executable, &model, &runtime_library]
                .iter()
                .all(|p| p.is_absolute()),
            "person host paths must be absolute"
        );
        Ok(Some(Self {
            executable,
            model,
            runtime_library,
        }))
    }
}
pub struct Identity {
    pub hash: String,
    pub runtime_hash: String,
}
pub async fn identity(config: &Config) -> Result<Identity> {
    let mut hash = Sha256::new();
    hash.update(b"rvm-mobilenetv3-fp32-ordered512-straight-alpha-v1");
    let mut runtime_hash = String::new();
    for (index, path) in [&config.executable, &config.model, &config.runtime_library]
        .iter()
        .enumerate()
    {
        ensure!(path.is_absolute(), "person host paths must be absolute");
        let meta = tokio::fs::symlink_metadata(path).await?;
        ensure!(
            meta.file_type().is_file() && meta.len() <= 256 * 1024 * 1024,
            "invalid person engine file"
        );
        let (size, digest) = super::local_speech::fingerprint_model(path).await?;
        if index == 1 {
            ensure!(
                size == MODEL_BYTES && digest == MODEL_HASH,
                "RVM model inventory mismatch"
            );
        }
        if index == 2 {
            runtime_hash = digest.clone();
        }
        hash.update(size.to_le_bytes());
        hash.update(digest.as_bytes());
    }
    Ok(Identity {
        hash: format!("{:x}", hash.finalize()),
        runtime_hash,
    })
}
