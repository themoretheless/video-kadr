//! Deterministic frame-level render boundary with idempotent publication.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::artifacts::{
    fingerprint_file, read_json_bounded, safe_relative_token, write_json_atomic, ArtifactFile,
    DEFAULT_MANIFEST_LIMIT,
};
use crate::domain::artifact_graph::Fingerprint;
use crate::runtime::cpu_pool::CpuPool;

const FRAME_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameRequest {
    pub frame_no: u64,
    pub plan_fingerprint: Fingerprint,
    pub source_fingerprint: Fingerprint,
    pub output_fingerprint: Fingerprint,
}

impl FrameRequest {
    pub fn key(&self) -> Fingerprint {
        let frame = self.frame_no.to_be_bytes();
        Fingerprint::combine([
            b"frame-v1".as_slice(),
            frame.as_slice(),
            self.plan_fingerprint.as_str().as_bytes(),
            self.source_fingerprint.as_str().as_bytes(),
            self.output_fingerprint.as_str().as_bytes(),
        ])
    }

    fn relative_frame_path(&self) -> PathBuf {
        let key = self.key();
        PathBuf::from("frames")
            .join(&key.as_str()[..2])
            .join(format!("{key}.frame"))
    }

    fn relative_manifest_path(&self) -> PathBuf {
        self.relative_frame_path().with_extension("json")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameArtifact {
    pub schema_version: u32,
    pub key: Fingerprint,
    pub request: FrameRequest,
    pub file: ArtifactFile,
}

#[axum::async_trait]
pub trait FrameRenderer: Send + Sync {
    /// Write one complete frame to `staging_path`. Implementations must produce
    /// byte-identical output for the same request and renderer/tool version.
    async fn render_frame(
        &self,
        request: &FrameRequest,
        staging_path: &Path,
        cancellation: &CancellationToken,
    ) -> Result<()>;
}

pub struct FrameRenderService<R: ?Sized> {
    renderer: Arc<R>,
    root: PathBuf,
    pool: CpuPool,
    key_locks: Arc<Mutex<HashMap<Fingerprint, Arc<Mutex<()>>>>>,
}

impl<R: ?Sized> Clone for FrameRenderService<R> {
    fn clone(&self) -> Self {
        Self {
            renderer: self.renderer.clone(),
            root: self.root.clone(),
            pool: self.pool.clone(),
            key_locks: self.key_locks.clone(),
        }
    }
}

impl<R: FrameRenderer + ?Sized> FrameRenderService<R> {
    pub fn new(renderer: Arc<R>, root: PathBuf, pool: CpuPool) -> Self {
        Self {
            renderer,
            root,
            pool,
            key_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn ensure(
        &self,
        request: FrameRequest,
        cancellation: CancellationToken,
    ) -> Result<FrameArtifact> {
        let key = request.key();
        let lock = {
            let mut locks = self.key_locks.lock().await;
            locks
                .entry(key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _guard = lock.lock().await;
        let result = ensure_frame(
            self.renderer.as_ref(),
            &self.root,
            request,
            &self.pool,
            cancellation,
        )
        .await;
        drop(_guard);
        self.release_key_lock(&key, &lock).await;
        result
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

async fn ensure_frame<R: FrameRenderer + ?Sized>(
    renderer: &R,
    root: &Path,
    request: FrameRequest,
    pool: &CpuPool,
    cancellation: CancellationToken,
) -> Result<FrameArtifact> {
    let expected_key = request.key();
    let frame_relative = request.relative_frame_path();
    let manifest_relative = request.relative_manifest_path();
    let frame_path = root.join(&frame_relative);
    let manifest_path = root.join(&manifest_relative);
    let expected_file_token = crate::artifacts::path_token(&frame_relative)?;

    if let Ok(existing) =
        read_json_bounded::<FrameArtifact>(&manifest_path, DEFAULT_MANIFEST_LIMIT).await
    {
        if existing.schema_version == FRAME_SCHEMA_VERSION
            && existing.key == expected_key
            && existing.request == request
            && existing.file.path == expected_file_token
        {
            match existing
                .file
                .verify(root, pool, cancellation.child_token())
                .await
            {
                Ok(_) => return Ok(existing),
                Err(error) if crate::artifacts::is_cpu_execution_error(&error) => {
                    return Err(error);
                }
                Err(_) => {}
            }
        }
    }

    if cancellation.is_cancelled() {
        return Err(anyhow!("frame render cancelled"));
    }
    let parent = frame_path
        .parent()
        .ok_or_else(|| anyhow!("frame path needs a parent"))?;
    tokio::fs::create_dir_all(parent).await?;
    remove_if_exists(&manifest_path).await?;
    remove_if_exists(&frame_path).await?;
    let staging = parent.join(format!(".{expected_key}.{}.tmp", Uuid::new_v4()));
    let render_result = renderer
        .render_frame(&request, &staging, &cancellation)
        .await;
    if let Err(error) = render_result {
        let _ = tokio::fs::remove_file(&staging).await;
        return Err(error);
    }
    if cancellation.is_cancelled() {
        let _ = tokio::fs::remove_file(&staging).await;
        return Err(anyhow!("frame render cancelled"));
    }
    let identity = match fingerprint_file(pool, staging.clone(), cancellation.child_token()).await {
        Ok(identity) => identity,
        Err(error) => {
            let _ = tokio::fs::remove_file(&staging).await;
            return Err(error);
        }
    };
    if let Err(error) = tokio::fs::rename(&staging, &frame_path).await {
        let _ = tokio::fs::remove_file(&staging).await;
        return Err(error.into());
    }
    let artifact = FrameArtifact {
        schema_version: FRAME_SCHEMA_VERSION,
        key: expected_key,
        request,
        file: ArtifactFile {
            path: crate::artifacts::path_token(&frame_relative)?,
            size: identity.size,
            sha256: identity.sha256,
        },
    };
    if let Err(error) = write_json_atomic(&manifest_path, &artifact).await {
        let _ = tokio::fs::remove_file(&frame_path).await;
        return Err(error);
    }
    Ok(artifact)
}

async fn remove_if_exists(path: &Path) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub fn resolve_frame_path(root: &Path, artifact: &FrameArtifact) -> Result<PathBuf> {
    if artifact.schema_version != FRAME_SCHEMA_VERSION
        || artifact.key != artifact.request.key()
        || artifact.file.path
            != crate::artifacts::path_token(&artifact.request.relative_frame_path())?
    {
        return Err(anyhow!("frame artifact metadata does not match its key"));
    }
    Ok(root.join(safe_relative_token(&artifact.file.path)?))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::runtime::cpu_pool::CpuPoolConfig;

    use super::*;

    struct FakeRenderer {
        calls: AtomicUsize,
    }

    #[axum::async_trait]
    impl FrameRenderer for FakeRenderer {
        async fn render_frame(
            &self,
            request: &FrameRequest,
            staging_path: &Path,
            _cancellation: &CancellationToken,
        ) -> Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::fs::write(staging_path, request.key().as_str()).await?;
            Ok(())
        }
    }

    fn request(frame_no: u64) -> FrameRequest {
        FrameRequest {
            frame_no,
            plan_fingerprint: Fingerprint::digest(b"plan"),
            source_fingerprint: Fingerprint::digest(b"source"),
            output_fingerprint: Fingerprint::digest(b"output"),
        }
    }

    fn pool() -> CpuPool {
        CpuPool::new(CpuPoolConfig {
            threads: 1,
            queue_capacity: 2,
        })
        .unwrap()
    }

    #[test]
    fn frame_key_is_deterministic_and_frame_sensitive() {
        assert_eq!(request(7).key(), request(7).key());
        assert_ne!(request(7).key(), request(8).key());
    }

    #[tokio::test]
    async fn verified_frame_is_reused_without_second_render() {
        let directory = tempfile::tempdir().unwrap();
        let renderer = Arc::new(FakeRenderer {
            calls: AtomicUsize::new(0),
        });
        let service =
            FrameRenderService::new(renderer.clone(), directory.path().to_path_buf(), pool());
        let first = service
            .ensure(request(7), CancellationToken::new())
            .await
            .unwrap();
        let second = service
            .ensure(request(7), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(renderer.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn corrupt_frame_is_rendered_again() {
        let directory = tempfile::tempdir().unwrap();
        let renderer = Arc::new(FakeRenderer {
            calls: AtomicUsize::new(0),
        });
        let service =
            FrameRenderService::new(renderer.clone(), directory.path().to_path_buf(), pool());
        let artifact = service
            .ensure(request(7), CancellationToken::new())
            .await
            .unwrap();
        tokio::fs::write(
            resolve_frame_path(directory.path(), &artifact).unwrap(),
            b"bad",
        )
        .await
        .unwrap();
        service
            .ensure(request(7), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(renderer.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn concurrent_requests_publish_one_frame() {
        let directory = tempfile::tempdir().unwrap();
        let renderer = Arc::new(FakeRenderer {
            calls: AtomicUsize::new(0),
        });
        let service =
            FrameRenderService::new(renderer.clone(), directory.path().to_path_buf(), pool());
        let first = service.ensure(request(7), CancellationToken::new());
        let second = service.ensure(request(7), CancellationToken::new());
        let (first, second) = tokio::join!(first, second);
        assert_eq!(first.unwrap(), second.unwrap());
        assert_eq!(renderer.calls.load(Ordering::SeqCst), 1);
        assert!(service.key_locks.lock().await.is_empty());
    }

    #[tokio::test]
    async fn artifact_cannot_resolve_a_different_internal_file() {
        let mut artifact = FrameArtifact {
            schema_version: FRAME_SCHEMA_VERSION,
            key: request(7).key(),
            request: request(7),
            file: ArtifactFile {
                path: "sources/other.mp4".into(),
                size: 1,
                sha256: Fingerprint::digest(b"x"),
            },
        };
        assert!(resolve_frame_path(Path::new("/storage"), &artifact).is_err());
        artifact.file.path =
            crate::artifacts::path_token(&artifact.request.relative_frame_path()).unwrap();
        assert!(resolve_frame_path(Path::new("/storage"), &artifact).is_ok());
    }
}
