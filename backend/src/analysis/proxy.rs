//! Proxy media is a disposable derivative of an immutable source identity.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::artifacts::{
    fingerprint_file, path_token, read_json_bounded, write_json_atomic, ArtifactFile,
    DEFAULT_MANIFEST_LIMIT,
};
use crate::domain::artifact_graph::Fingerprint;
use crate::runtime::cpu_pool::CpuPool;
use crate::runtime::TaskSupervisor;

const PROXY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyCodec {
    H264,
    ProresProxy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyProfile {
    pub max_width: u32,
    pub codec: ProxyCodec,
    pub quality: u8,
    pub include_audio: bool,
}

impl Default for ProxyProfile {
    fn default() -> Self {
        Self {
            max_width: 960,
            codec: ProxyCodec::H264,
            quality: 28,
            include_audio: true,
        }
    }
}

impl ProxyProfile {
    pub fn validate(&self) -> Result<()> {
        if !(160..=3840).contains(&self.max_width) {
            return Err(anyhow!("proxy width must be within 160..=3840"));
        }
        if self.quality > 63 {
            return Err(anyhow!("proxy quality must be within 0..=63"));
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::digest(
            &serde_json::to_vec(self).expect("ProxyProfile serialization cannot fail"),
        )
    }

    pub fn extension(&self) -> &'static str {
        match self.codec {
            ProxyCodec::H264 => "mp4",
            ProxyCodec::ProresProxy => "mov",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceMedia {
    pub id: String,
    pub original_path: PathBuf,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceIdentity {
    pub id: String,
    pub original_path: PathBuf,
    pub duration_seconds: f64,
    pub fingerprint: Fingerprint,
}

impl SourceIdentity {
    /// Relink changes location, never source identity. Inspect the replacement
    /// first when content equality is uncertain.
    fn relink(&self, original_path: PathBuf) -> Self {
        Self {
            original_path,
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyArtifact {
    pub schema_version: u32,
    pub key: Fingerprint,
    pub source_id: String,
    pub source_fingerprint: Fingerprint,
    pub profile: ProxyProfile,
    pub file: ArtifactFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaIntent {
    Preview,
    Export,
}

#[axum::async_trait]
pub trait ProxyEncoder: Send + Sync {
    async fn generate(
        &self,
        source: &SourceIdentity,
        profile: &ProxyProfile,
        staging_path: &Path,
        cancellation: &CancellationToken,
    ) -> Result<()>;
}

pub struct ProxyService<E: ?Sized> {
    root: PathBuf,
    cpu_pool: CpuPool,
    encoder: Arc<E>,
    key_locks: Arc<Mutex<HashMap<Fingerprint, Arc<Mutex<()>>>>>,
}

impl<E: ?Sized> Clone for ProxyService<E> {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            cpu_pool: self.cpu_pool.clone(),
            encoder: self.encoder.clone(),
            key_locks: self.key_locks.clone(),
        }
    }
}

impl<E: ProxyEncoder + ?Sized + 'static> ProxyService<E> {
    pub fn new(root: PathBuf, cpu_pool: CpuPool, encoder: Arc<E>) -> Self {
        Self {
            root,
            cpu_pool,
            encoder,
            key_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn inspect_source(
        &self,
        source: SourceMedia,
        cancellation: CancellationToken,
    ) -> Result<SourceIdentity> {
        if source.id.is_empty()
            || !source.duration_seconds.is_finite()
            || source.duration_seconds <= 0.0
        {
            return Err(anyhow!("proxy source metadata is invalid"));
        }
        let identity =
            fingerprint_file(&self.cpu_pool, source.original_path.clone(), cancellation).await?;
        Ok(SourceIdentity {
            id: source.id,
            original_path: source.original_path,
            duration_seconds: source.duration_seconds,
            fingerprint: identity.sha256,
        })
    }

    pub async fn relink_verified(
        &self,
        source: &SourceIdentity,
        new_path: PathBuf,
        cancellation: CancellationToken,
    ) -> Result<SourceIdentity> {
        let identity = fingerprint_file(&self.cpu_pool, new_path.clone(), cancellation).await?;
        if identity.sha256 != source.fingerprint {
            return Err(anyhow!("relinked source checksum does not match"));
        }
        Ok(source.relink(new_path))
    }

    pub async fn ensure(
        &self,
        source: SourceIdentity,
        profile: ProxyProfile,
        cancellation: CancellationToken,
    ) -> Result<ProxyArtifact> {
        profile.validate()?;
        let key = proxy_key(&source, &profile);
        let lock = self.key_lock(&key).await;
        let _guard = lock.lock().await;
        let result = self
            .ensure_locked(source, profile, key.clone(), cancellation)
            .await;
        drop(_guard);
        self.release_key_lock(&key, &lock).await;
        result
    }

    async fn ensure_locked(
        &self,
        source: SourceIdentity,
        profile: ProxyProfile,
        key: Fingerprint,
        cancellation: CancellationToken,
    ) -> Result<ProxyArtifact> {
        if let Some(artifact) = self
            .load_ready(&source, &profile, &key, cancellation.child_token())
            .await?
        {
            return Ok(artifact);
        }
        if cancellation.is_cancelled() {
            return Err(anyhow!("proxy generation cancelled"));
        }
        let staging_dir = self.root.join("staging").join("proxies");
        tokio::fs::create_dir_all(&staging_dir).await?;
        let staging = proxy_staging_path(&staging_dir, &key, &profile);
        if let Err(error) = self
            .encoder
            .generate(&source, &profile, &staging, &cancellation)
            .await
        {
            let _ = tokio::fs::remove_file(&staging).await;
            return Err(error);
        }
        if cancellation.is_cancelled() {
            let _ = tokio::fs::remove_file(&staging).await;
            return Err(anyhow!("proxy generation cancelled"));
        }
        let published = self
            .publish(&source, profile, key, &staging, cancellation)
            .await;
        if published.is_err() {
            let _ = tokio::fs::remove_file(&staging).await;
        }
        published
    }

    pub fn schedule(
        &self,
        supervisor: &TaskSupervisor,
        source: SourceIdentity,
        profile: ProxyProfile,
    ) -> JoinHandle<Result<ProxyArtifact>> {
        let service = self.clone();
        let cancellation = supervisor.child_token();
        supervisor.spawn(async move { service.ensure(source, profile, cancellation).await })
    }

    pub fn resolve(
        &self,
        source: &SourceIdentity,
        proxy: Option<&ProxyArtifact>,
        intent: MediaIntent,
    ) -> PathBuf {
        if intent == MediaIntent::Preview {
            if let Some(proxy) = proxy.filter(|proxy| {
                let expected = proxy_relative_path(&proxy.key, &proxy.profile);
                proxy.source_id == source.id
                    && proxy.source_fingerprint == source.fingerprint
                    && proxy.schema_version == PROXY_SCHEMA_VERSION
                    && path_token(&expected).ok().as_deref() == Some(proxy.file.path.as_str())
            }) {
                return self.root.join(&proxy.file.path);
            }
        }
        source.original_path.clone()
    }

    pub async fn remove(&self, artifact: &ProxyArtifact) -> Result<()> {
        let expected_file = proxy_relative_path(&artifact.key, &artifact.profile);
        if artifact.file.path != path_token(&expected_file)? {
            return Err(anyhow!("proxy manifest points outside its artifact key"));
        }
        remove_if_exists(&self.root.join(expected_file)).await?;
        remove_if_exists(&self.root.join(proxy_manifest_path(&artifact.key))).await?;
        Ok(())
    }

    async fn load_ready(
        &self,
        source: &SourceIdentity,
        profile: &ProxyProfile,
        key: &Fingerprint,
        cancellation: CancellationToken,
    ) -> Result<Option<ProxyArtifact>> {
        let path = self.root.join(proxy_manifest_path(key));
        let artifact = match read_json_bounded::<ProxyArtifact>(&path, DEFAULT_MANIFEST_LIMIT).await
        {
            Ok(artifact) => artifact,
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                return Ok(None);
            }
            Err(error) if error.downcast_ref::<std::io::Error>().is_some() => return Err(error),
            Err(_) => {
                remove_if_exists(&path).await?;
                remove_if_exists(&self.root.join(proxy_relative_path(key, profile))).await?;
                return Ok(None);
            }
        };
        let metadata_valid = artifact.schema_version == PROXY_SCHEMA_VERSION
            && artifact.key == *key
            && artifact.source_id == source.id
            && artifact.source_fingerprint == source.fingerprint
            && artifact.profile == *profile;
        let file_valid = if metadata_valid {
            match artifact
                .file
                .verify(&self.root, &self.cpu_pool, cancellation)
                .await
            {
                Ok(_) => true,
                Err(error) if crate::artifacts::is_cpu_execution_error(&error) => {
                    return Err(error);
                }
                Err(_) => false,
            }
        } else {
            false
        };
        if !file_valid {
            remove_if_exists(&path).await?;
            remove_if_exists(&self.root.join(proxy_relative_path(key, profile))).await?;
            return Ok(None);
        }
        Ok(Some(artifact))
    }

    async fn publish(
        &self,
        source: &SourceIdentity,
        profile: ProxyProfile,
        key: Fingerprint,
        staging: &Path,
        cancellation: CancellationToken,
    ) -> Result<ProxyArtifact> {
        let identity =
            fingerprint_file(&self.cpu_pool, staging.to_path_buf(), cancellation).await?;
        let relative = proxy_relative_path(&key, &profile);
        let final_path = self.root.join(&relative);
        let parent = final_path
            .parent()
            .ok_or_else(|| anyhow!("proxy path needs a parent"))?;
        tokio::fs::create_dir_all(parent).await?;
        tokio::fs::rename(staging, &final_path).await?;
        let artifact = ProxyArtifact {
            schema_version: PROXY_SCHEMA_VERSION,
            key: key.clone(),
            source_id: source.id.clone(),
            source_fingerprint: source.fingerprint.clone(),
            profile,
            file: ArtifactFile {
                path: path_token(&relative)?,
                size: identity.size,
                sha256: identity.sha256,
            },
        };
        if let Err(error) =
            write_json_atomic(&self.root.join(proxy_manifest_path(&key)), &artifact).await
        {
            let _ = tokio::fs::remove_file(&final_path).await;
            return Err(error);
        }
        Ok(artifact)
    }

    async fn key_lock(&self, key: &Fingerprint) -> Arc<Mutex<()>> {
        let mut locks = self.key_locks.lock().await;
        locks
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    async fn release_key_lock(&self, key: &Fingerprint, lock: &Arc<Mutex<()>>) {
        let mut locks = self.key_locks.lock().await;
        if Arc::strong_count(lock) == 2
            && locks
                .get(key)
                .is_some_and(|registered| Arc::ptr_eq(registered, lock))
        {
            locks.remove(key);
        }
    }
}

fn proxy_key(source: &SourceIdentity, profile: &ProxyProfile) -> Fingerprint {
    Fingerprint::combine([
        b"proxy-v1".as_slice(),
        source.fingerprint.as_str().as_bytes(),
        profile.fingerprint().as_str().as_bytes(),
    ])
}

fn proxy_relative_path(key: &Fingerprint, profile: &ProxyProfile) -> PathBuf {
    PathBuf::from("proxies").join(format!("{key}.{}", profile.extension()))
}

fn proxy_manifest_path(key: &Fingerprint) -> PathBuf {
    PathBuf::from("proxies").join(format!("{key}.json"))
}

fn proxy_staging_path(directory: &Path, key: &Fingerprint, profile: &ProxyProfile) -> PathBuf {
    // FFmpeg infers the muxer from the final suffix, so keep the media
    // extension after the temporary marker.
    directory.join(format!(
        "{key}.{}.tmp.{}",
        Uuid::new_v4(),
        profile.extension()
    ))
}

async fn remove_if_exists(path: &Path) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::runtime::cpu_pool::CpuPoolConfig;

    use super::*;

    struct FakeEncoder {
        calls: AtomicUsize,
    }

    #[axum::async_trait]
    impl ProxyEncoder for FakeEncoder {
        async fn generate(
            &self,
            source: &SourceIdentity,
            profile: &ProxyProfile,
            staging_path: &Path,
            _cancellation: &CancellationToken,
        ) -> Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::fs::write(
                staging_path,
                format!("{}:{}", source.fingerprint, profile.max_width),
            )
            .await?;
            Ok(())
        }
    }

    fn pool() -> CpuPool {
        CpuPool::new(CpuPoolConfig {
            threads: 1,
            queue_capacity: 2,
        })
        .unwrap()
    }

    #[tokio::test]
    async fn proxy_is_content_addressed_reused_and_never_used_for_export() {
        let root = tempfile::tempdir().unwrap();
        let source_path = root.path().join("source.mp4");
        tokio::fs::write(&source_path, b"full resolution")
            .await
            .unwrap();
        let encoder = Arc::new(FakeEncoder {
            calls: AtomicUsize::new(0),
        });
        let service = ProxyService::new(root.path().to_path_buf(), pool(), encoder.clone());
        let source = service
            .inspect_source(
                SourceMedia {
                    id: "source".into(),
                    original_path: source_path.clone(),
                    duration_seconds: 10.0,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let first = service
            .ensure(
                source.clone(),
                ProxyProfile::default(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let second = service
            .ensure(
                source.clone(),
                ProxyProfile::default(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(encoder.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            service.resolve(&source, Some(&first), MediaIntent::Export),
            source_path
        );
        assert_ne!(
            service.resolve(&source, Some(&first), MediaIntent::Preview),
            source_path
        );
        assert_eq!(
            proxy_staging_path(root.path(), &first.key, &first.profile)
                .extension()
                .and_then(|value| value.to_str()),
            Some("mp4")
        );
    }

    #[tokio::test]
    async fn proxy_delete_and_relink_leave_original_source_untouched() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("source.mp4");
        let relinked = root.path().join("moved.mp4");
        tokio::fs::write(&original, b"full resolution")
            .await
            .unwrap();
        let service = ProxyService::new(
            root.path().to_path_buf(),
            pool(),
            Arc::new(FakeEncoder {
                calls: AtomicUsize::new(0),
            }),
        );
        let source = service
            .inspect_source(
                SourceMedia {
                    id: "source".into(),
                    original_path: original.clone(),
                    duration_seconds: 10.0,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let artifact = service
            .ensure(
                source.clone(),
                ProxyProfile::default(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        tokio::fs::rename(&original, &relinked).await.unwrap();
        let source = service
            .relink_verified(&source, relinked.clone(), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            service.resolve(&source, Some(&artifact), MediaIntent::Export),
            relinked
        );
        service.remove(&artifact).await.unwrap();
        assert!(relinked.exists());
        assert!(!root.path().join(&artifact.file.path).exists());
    }

    #[tokio::test]
    async fn concurrent_ensure_runs_one_encoder_and_relink_checks_content() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("source.mp4");
        let wrong = root.path().join("wrong.mp4");
        tokio::fs::write(&original, b"same source").await.unwrap();
        tokio::fs::write(&wrong, b"different source").await.unwrap();
        let encoder = Arc::new(FakeEncoder {
            calls: AtomicUsize::new(0),
        });
        let service = ProxyService::new(root.path().to_path_buf(), pool(), encoder.clone());
        let source = service
            .inspect_source(
                SourceMedia {
                    id: "source".into(),
                    original_path: original,
                    duration_seconds: 5.0,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let first = service.ensure(
            source.clone(),
            ProxyProfile::default(),
            CancellationToken::new(),
        );
        let second = service.ensure(
            source.clone(),
            ProxyProfile::default(),
            CancellationToken::new(),
        );
        let (first, second) = tokio::join!(first, second);
        assert_eq!(first.unwrap(), second.unwrap());
        assert_eq!(encoder.calls.load(Ordering::SeqCst), 1);
        assert!(service.key_locks.lock().await.is_empty());
        assert!(service
            .relink_verified(&source, wrong, CancellationToken::new())
            .await
            .is_err());
    }
}
