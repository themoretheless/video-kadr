//! Shared filesystem primitives for derived artifacts.
//!
//! Manifests use safe relative paths, bounded JSON, atomic publication, and
//! checksums computed on the dedicated CPU pool.

use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::domain::artifact_graph::Fingerprint;
use crate::runtime::cpu_pool::{CpuPool, CpuTaskError};

pub const DEFAULT_MANIFEST_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactFile {
    pub path: String,
    pub size: u64,
    pub sha256: Fingerprint,
}

impl ArtifactFile {
    pub async fn inspect(
        root: &Path,
        relative: &Path,
        pool: &CpuPool,
        cancellation: CancellationToken,
    ) -> Result<Self> {
        let path = safe_relative_path(relative)?;
        let absolute = contained_existing_path(root, &path).await?;
        let identity = fingerprint_file(pool, absolute, cancellation).await?;
        Ok(Self {
            path: path_token(&path)?,
            size: identity.size,
            sha256: identity.sha256,
        })
    }

    pub async fn verify(
        &self,
        root: &Path,
        pool: &CpuPool,
        cancellation: CancellationToken,
    ) -> Result<PathBuf> {
        let relative = safe_relative_token(&self.path)?;
        let absolute = contained_existing_path(root, &relative).await?;
        let identity = fingerprint_file(pool, absolute.clone(), cancellation).await?;
        if identity.size != self.size || identity.sha256 != self.sha256 {
            return Err(anyhow!("artifact checksum mismatch: {}", self.path));
        }
        Ok(absolute)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIdentity {
    pub size: u64,
    pub sha256: Fingerprint,
}

pub async fn fingerprint_file(
    pool: &CpuPool,
    path: PathBuf,
    cancellation: CancellationToken,
) -> Result<FileIdentity> {
    pool.execute(cancellation, move |token| {
        let metadata = std::fs::symlink_metadata(&path)
            .with_context(|| format!("read artifact metadata: {}", path.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow!(
                "artifact is not a regular file: {}",
                path.display()
            ));
        }
        let mut reader = BufReader::new(
            std::fs::File::open(&path)
                .with_context(|| format!("open artifact: {}", path.display()))?,
        );
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 128 * 1024];
        loop {
            if token.is_cancelled() {
                return Err(anyhow!("artifact hashing cancelled"));
            }
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(FileIdentity {
            size: metadata.len(),
            sha256: Fingerprint::parse(format!("{:x}", hasher.finalize()))?,
        })
    })
    .await
    .map_err(cpu_error)
}

pub async fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("artifact manifest needs a parent directory"))?;
    tokio::fs::create_dir_all(parent).await?;
    let bytes = serde_json::to_vec_pretty(value)?;
    if bytes.len() > DEFAULT_MANIFEST_LIMIT {
        return Err(anyhow!("artifact manifest is too large"));
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("artifact manifest needs a UTF-8 filename"))?;
    let temporary = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    let result = async {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .await?;
        file.write_all(&bytes).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(&temporary, path).await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    result
}

pub async fn read_json_bounded<T: DeserializeOwned>(path: &Path, limit: usize) -> Result<T> {
    let metadata = tokio::fs::symlink_metadata(path).await?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(anyhow!("manifest is not a regular file"));
    }
    if metadata.len() > limit as u64 {
        return Err(anyhow!("artifact manifest exceeds {limit} bytes"));
    }
    let file = tokio::fs::File::open(path).await?;
    let read_limit = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    let mut bytes = Vec::new();
    file.take(read_limit).read_to_end(&mut bytes).await?;
    if bytes.len() > limit {
        return Err(anyhow!("artifact manifest exceeds {limit} bytes"));
    }
    Ok(serde_json::from_slice(&bytes)?)
}

async fn contained_existing_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    let canonical_root = tokio::fs::canonicalize(root)
        .await
        .with_context(|| format!("canonicalize artifact root: {}", root.display()))?;
    let candidate = root.join(relative);
    let canonical_candidate = tokio::fs::canonicalize(&candidate)
        .await
        .with_context(|| format!("canonicalize artifact: {}", candidate.display()))?;
    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(anyhow!("artifact path escapes its root"));
    }
    Ok(canonical_candidate)
}

pub fn safe_relative_token(token: &str) -> Result<PathBuf> {
    safe_relative_path(Path::new(token))
}

pub fn safe_relative_path(path: &Path) -> Result<PathBuf> {
    let mut components = 0_usize;
    for component in path.components() {
        match component {
            Component::Normal(value) if !value.is_empty() => components += 1,
            _ => return Err(anyhow!("artifact path must be relative and normalized")),
        }
    }
    if components == 0 || components > 16 {
        return Err(anyhow!("artifact path has an invalid component count"));
    }
    Ok(path.to_path_buf())
}

pub fn path_token(path: &Path) -> Result<String> {
    let safe = safe_relative_path(path)?;
    let token = safe
        .to_str()
        .ok_or_else(|| anyhow!("artifact path must be UTF-8"))?;
    Ok(token.replace(std::path::MAIN_SEPARATOR, "/"))
}

fn cpu_error(error: CpuTaskError) -> anyhow::Error {
    anyhow!(error)
}

pub fn is_cpu_execution_error(error: &anyhow::Error) -> bool {
    error.downcast_ref::<CpuTaskError>().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::cpu_pool::CpuPoolConfig;

    fn pool() -> CpuPool {
        CpuPool::new(CpuPoolConfig {
            threads: 1,
            queue_capacity: 2,
        })
        .unwrap()
    }

    #[tokio::test]
    async fn manifest_round_trip_is_atomic_and_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("manifest.json");
        let value = serde_json::json!({"schemaVersion": 1, "name": "ready"});
        write_json_atomic(&path, &value).await.unwrap();
        let loaded: serde_json::Value = read_json_bounded(&path, 1024).await.unwrap();
        assert_eq!(loaded, value);
        assert_eq!(directory.path().read_dir().unwrap().count(), 1);

        tokio::fs::write(&path, vec![b'x'; 1025]).await.unwrap();
        assert!(read_json_bounded::<serde_json::Value>(&path, 1024)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn artifact_verification_detects_content_changes() {
        let directory = tempfile::tempdir().unwrap();
        tokio::fs::write(directory.path().join("chunk.bin"), b"one")
            .await
            .unwrap();
        let file = ArtifactFile::inspect(
            directory.path(),
            Path::new("chunk.bin"),
            &pool(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        file.verify(directory.path(), &pool(), CancellationToken::new())
            .await
            .unwrap();
        tokio::fs::write(directory.path().join("chunk.bin"), b"two")
            .await
            .unwrap();
        assert!(file
            .verify(directory.path(), &pool(), CancellationToken::new())
            .await
            .is_err());
    }

    #[test]
    fn artifact_paths_reject_escape_and_absolute_tokens() {
        assert!(safe_relative_token("segments/0001.m4s").is_ok());
        assert!(safe_relative_token("../secret").is_err());
        assert!(safe_relative_token("/etc/passwd").is_err());
        assert!(safe_relative_token("").is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn hashing_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("source"), b"secret").unwrap();
        symlink(
            directory.path().join("source"),
            directory.path().join("derived"),
        )
        .unwrap();
        assert!(fingerprint_file(
            &pool(),
            directory.path().join("derived"),
            CancellationToken::new(),
        )
        .await
        .is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn artifact_paths_cannot_escape_through_a_parent_symlink() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("segment"), b"secret").unwrap();
        symlink(outside.path(), root.path().join("segments")).unwrap();

        assert!(ArtifactFile::inspect(
            root.path(),
            Path::new("segments/segment"),
            &pool(),
            CancellationToken::new(),
        )
        .await
        .is_err());
    }
}
