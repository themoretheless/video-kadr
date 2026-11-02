//! Local encrypted narration requests and bounded content identity for durable inference.
use crate::{services::local_tts::TtsConfig, youtube::TokenCipher};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use tokio::io::AsyncWriteExt;
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub async fn cipher(storage: &Path) -> Result<TokenCipher> {
    let path = storage.join(".tts-job-key");
    if !path.try_exists()? {
        use ring::rand::SecureRandom;
        let mut key = [0u8; 32];
        ring::rand::SystemRandom::new()
            .fill(&mut key)
            .map_err(|_| anyhow::anyhow!("generate narration key"))?;
        let temporary_path = storage.join(format!(".tts-key-{}", uuid::Uuid::new_v4()));
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .await?;
        let temporary = Temp(temporary_path);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .await?;
        }
        file.write_all(&key).await?;
        file.sync_all().await?;
        drop(file);
        match tokio::fs::hard_link(&temporary.0, &path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    load_cipher(storage).await
}

pub async fn load_cipher(storage: &Path) -> Result<TokenCipher> {
    let path = storage.join(".tts-job-key");
    let metadata = tokio::fs::symlink_metadata(&path).await?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() == 32,
        "invalid narration encryption key file"
    );
    let key = tokio::fs::read(path).await?;
    ensure!(key.len() == 32, "invalid narration encryption key");
    TokenCipher::new(key.try_into().expect("key length"))
}
pub fn context(owner: &str, id: &str, identity: &str) -> String {
    format!("tts-job\0{owner}\0{id}\0{identity}")
}
/// Snapshot the model bundle and configured executable, without persisting host paths.
/// Shared runtime libraries and accelerator/OS behavior are outside this identity.
pub async fn fingerprint(config: &TtsConfig) -> Result<String> {
    let root = config
        .model_dir
        .as_ref()
        .context("Kokoro model is unavailable")?;
    ensure!(
        root.is_absolute() && root.is_dir(),
        "invalid Kokoro directory"
    );
    let executable = if config.executable.is_absolute() {
        config.executable.clone()
    } else if config.executable.components().count() > 1 {
        tokio::fs::canonicalize(&config.executable).await?
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|p| p.join(&config.executable))
            .find(|p| p.is_file())
            .context("Kokoro executable is unavailable")?
    };
    let mut files = BTreeSet::new();
    let mut directories = vec![root.clone()];
    let mut entries = 0usize;
    while let Some(dir) = directories.pop() {
        let mut scan = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = scan.next_entry().await? {
            entries += 1;
            ensure!(entries <= 2048, "Kokoro bundle has too many entries");
            let kind = entry.file_type().await?;
            ensure!(!kind.is_symlink(), "Kokoro bundle symlinks are unsupported");
            if kind.is_dir() {
                directories.push(entry.path())
            } else {
                ensure!(kind.is_file(), "unsupported Kokoro entry");
                files.insert(entry.path());
            }
        }
    }
    ensure!(!files.is_empty(), "empty Kokoro bundle");
    let mut digest = Sha256::new();
    let mut total = 0u64;
    for path in files {
        let relative = path
            .strip_prefix(root)?
            .to_str()
            .context("non UTF8 Kokoro path")?;
        let (size, hash) = super::local_speech::fingerprint_model(&path).await?;
        total = total.checked_add(size).context("Kokoro bundle overflow")?;
        ensure!(total <= 2_000_000_000, "Kokoro bundle exceeds budget");
        digest.update((relative.len() as u64).to_le_bytes());
        digest.update(relative.as_bytes());
        digest.update(size.to_le_bytes());
        digest.update(hash.as_bytes());
    }
    let (size, hash) = super::local_speech::fingerprint_model(&executable).await?;
    digest.update(b"executable\0");
    digest.update(size.to_le_bytes());
    digest.update(hash.as_bytes());
    Ok(digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
