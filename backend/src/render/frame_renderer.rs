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

const FRAME_SCHEMA_VERSION: u32 = 2;
const PREVIEW_TIME_BASE_MAX: u32 = 1_000_000_000;
const PREVIEW_DIMENSION_MAX: u32 = 16_384;
const PREVIEW_FPS_MILLI_MAX: u32 = 240_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewFrameFormat {
    Jpeg,
    Webp,
}

impl PreviewFrameFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }

    fn mime_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
        }
    }
}

/// Canonical preview-only settings. Timeline position is represented as an
/// integer tick plus its project clock, never as a lossy floating-point time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewRenderSettings {
    pub timeline_tick: u64,
    pub timeline_time_base: u32,
    pub width: u32,
    pub height: u32,
    pub max_fps_milli: u32,
    pub quality: u8,
    pub format: PreviewFrameFormat,
}

impl PreviewRenderSettings {
    pub fn validate(&self) -> Result<()> {
        if self.timeline_time_base == 0 || self.timeline_time_base > PREVIEW_TIME_BASE_MAX {
            return Err(anyhow!("preview timeline time base is invalid"));
        }
        if self.width == 0
            || self.height == 0
            || self.width > PREVIEW_DIMENSION_MAX
            || self.height > PREVIEW_DIMENSION_MAX
        {
            return Err(anyhow!("preview dimensions are invalid"));
        }
        if self.max_fps_milli == 0 || self.max_fps_milli > PREVIEW_FPS_MILLI_MAX {
            return Err(anyhow!("preview frame rate is invalid"));
        }
        if self.quality == 0 || self.quality > 100 {
            return Err(anyhow!("preview quality is invalid"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameRequest {
    pub source_fingerprint: Fingerprint,
    pub graph_version: Fingerprint,
    pub renderer_compatibility: String,
    /// Unique physical publication namespace. It is deliberately excluded from
    /// `key()`: retries share semantic identity but can never overwrite bytes
    /// produced by an older lease.
    pub publication_id: String,
    pub settings: PreviewRenderSettings,
}

impl FrameRequest {
    pub fn key(&self) -> Fingerprint {
        let canonical_settings =
            serde_json::to_vec(&self.settings).expect("preview settings serialization cannot fail");
        Fingerprint::combine([
            b"preview-frame-v2".as_slice(),
            self.source_fingerprint.as_str().as_bytes(),
            self.graph_version.as_str().as_bytes(),
            self.renderer_compatibility.as_bytes(),
            canonical_settings.as_slice(),
        ])
    }

    pub fn validate(&self) -> Result<()> {
        if self.renderer_compatibility.is_empty()
            || self.renderer_compatibility.len() > 128
            || !self.renderer_compatibility.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':')
            })
        {
            return Err(anyhow!("preview renderer compatibility is invalid"));
        }
        if self.publication_id.is_empty()
            || self.publication_id.len() > 64
            || !self
                .publication_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(anyhow!("preview publication id is invalid"));
        }
        self.settings.validate()
    }

    fn relative_frame_path(&self) -> PathBuf {
        let key = self.key();
        PathBuf::from("frames")
            .join(&key.as_str()[..2])
            .join(key.as_str())
            .join(format!(
                "{}.{}",
                self.publication_id,
                self.settings.format.extension()
            ))
    }

    pub fn artifact_locator(&self) -> Result<String> {
        self.validate()?;
        crate::artifacts::path_token(&self.relative_frame_path())
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
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub file: ArtifactFile,
}

#[axum::async_trait]
pub trait FrameRenderer: Send + Sync {
    fn compatibility(&self) -> &str;

    /// Write one complete frame to `staging_path`. Implementations must produce
    /// byte-identical output for the same request and renderer/tool version.
    async fn render_frame(
        &self,
        request: &FrameRequest,
        staging_path: &Path,
        cancellation: &CancellationToken,
    ) -> Result<()>;

    /// Decode/probe the staged image before publication. Metadata in the
    /// manifest is never accepted solely because it was requested.
    async fn validate_frame(&self, request: &FrameRequest, staging_path: &Path) -> Result<()>;
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
        request.validate()?;
        if request.renderer_compatibility != self.renderer.compatibility() {
            return Err(anyhow!("preview renderer compatibility mismatch"));
        }
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

    pub async fn validated(
        &self,
        request: &FrameRequest,
        cancellation: CancellationToken,
    ) -> Result<Option<FrameArtifact>> {
        request.validate()?;
        if request.renderer_compatibility != self.renderer.compatibility() {
            return Err(anyhow!("preview renderer compatibility mismatch"));
        }
        let manifest = self.root.join(request.relative_manifest_path());
        let existing =
            match read_json_bounded::<FrameArtifact>(&manifest, DEFAULT_MANIFEST_LIMIT).await {
                Ok(value) => value,
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
                {
                    return Ok(None)
                }
                Err(_) => return Ok(None),
            };
        let expected_token = crate::artifacts::path_token(&request.relative_frame_path())?;
        if existing.schema_version != FRAME_SCHEMA_VERSION
            || existing.key != request.key()
            || existing.request != *request
            || existing.request.validate().is_err()
            || existing.mime_type != request.settings.format.mime_type()
            || existing.width != request.settings.width
            || existing.height != request.settings.height
            || existing.file.path != expected_token
        {
            return Ok(None);
        }
        match existing
            .file
            .verify(&self.root, &self.pool, cancellation)
            .await
        {
            Ok(_) => Ok(Some(existing)),
            Err(error) if crate::artifacts::is_cpu_execution_error(&error) => Err(error),
            Err(_) => Ok(None),
        }
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
            && existing.request.validate().is_ok()
            && existing.mime_type == request.settings.format.mime_type()
            && existing.width == request.settings.width
            && existing.height == request.settings.height
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
    let staging = parent.join(format!(
        ".{expected_key}.{}.tmp.{}",
        Uuid::new_v4(),
        request.settings.format.extension()
    ));
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
    if let Err(error) = renderer.validate_frame(&request, &staging).await {
        let _ = tokio::fs::remove_file(&staging).await;
        return Err(error);
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
        mime_type: request.settings.format.mime_type().to_owned(),
        width: request.settings.width,
        height: request.settings.height,
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
        || artifact.request.validate().is_err()
        || artifact.key != artifact.request.key()
        || artifact.mime_type != artifact.request.settings.format.mime_type()
        || artifact.width != artifact.request.settings.width
        || artifact.height != artifact.request.settings.height
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
        fn compatibility(&self) -> &'static str {
            "fake-preview-v2"
        }

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

        async fn validate_frame(&self, request: &FrameRequest, staging_path: &Path) -> Result<()> {
            let bytes = tokio::fs::read(staging_path).await?;
            if bytes != request.key().as_str().as_bytes() {
                return Err(anyhow!("invalid fake frame"));
            }
            Ok(())
        }
    }

    fn request(timeline_tick: u64) -> FrameRequest {
        FrameRequest {
            source_fingerprint: Fingerprint::digest(b"source"),
            graph_version: Fingerprint::digest(b"graph"),
            renderer_compatibility: "fake-preview-v2".into(),
            publication_id: "test-publication".into(),
            settings: PreviewRenderSettings {
                timeline_tick,
                timeline_time_base: 1_000_000,
                width: 640,
                height: 360,
                max_fps_milli: 30_000,
                quality: 80,
                format: PreviewFrameFormat::Webp,
            },
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
    fn frame_key_is_deterministic_and_semantically_sensitive() {
        assert_eq!(request(7).key(), request(7).key());
        assert_ne!(request(7).key(), request(8).key());
        let baseline = request(7).key();
        let mut changed = request(7);
        changed.source_fingerprint = Fingerprint::digest(b"other-source");
        assert_ne!(baseline, changed.key());
        let mut changed = request(7);
        changed.graph_version = Fingerprint::digest(b"other-graph");
        assert_ne!(baseline, changed.key());
        let mut changed = request(7);
        changed.renderer_compatibility = "fake-preview-v3".into();
        assert_ne!(baseline, changed.key());
        let mut changed = request(7);
        changed.settings.quality = 79;
        assert_ne!(baseline, changed.key());
        let mut changed = request(7);
        changed.settings.timeline_time_base = 90_000;
        assert_ne!(baseline, changed.key());
        let mut retry = request(7);
        retry.publication_id = "another-lease".into();
        assert_eq!(baseline, retry.key());
    }

    #[test]
    fn invalid_preview_settings_and_renderer_are_rejected() {
        let mut invalid = request(0);
        invalid.settings.width = 0;
        assert!(invalid.validate().is_err());
        invalid = request(0);
        invalid.settings.timeline_time_base = 0;
        assert!(invalid.validate().is_err());
        invalid = request(0);
        invalid.renderer_compatibility = "../../renderer".into();
        assert!(invalid.validate().is_err());
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
    async fn manifest_with_wrong_media_metadata_is_rendered_again() {
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
        let mut invalid = artifact.clone();
        invalid.mime_type = "image/jpeg".into();
        write_json_atomic(
            &directory
                .path()
                .join(artifact.request.relative_manifest_path()),
            &invalid,
        )
        .await
        .unwrap();
        service
            .ensure(request(7), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(renderer.calls.load(Ordering::SeqCst), 2);
    }

    struct CancellingRenderer;

    #[axum::async_trait]
    impl FrameRenderer for CancellingRenderer {
        fn compatibility(&self) -> &'static str {
            "cancel-preview-v2"
        }

        async fn render_frame(
            &self,
            _request: &FrameRequest,
            staging_path: &Path,
            cancellation: &CancellationToken,
        ) -> Result<()> {
            tokio::fs::write(staging_path, b"partial").await?;
            cancellation.cancel();
            Ok(())
        }

        async fn validate_frame(
            &self,
            _request: &FrameRequest,
            _staging_path: &Path,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn cancellation_never_publishes_frame_or_manifest() {
        let directory = tempfile::tempdir().unwrap();
        let mut cancelled_request = request(7);
        cancelled_request.renderer_compatibility = "cancel-preview-v2".into();
        let frame = directory
            .path()
            .join(cancelled_request.relative_frame_path());
        let manifest = directory
            .path()
            .join(cancelled_request.relative_manifest_path());
        let service = FrameRenderService::new(
            Arc::new(CancellingRenderer),
            directory.path().to_path_buf(),
            pool(),
        );
        assert!(service
            .ensure(cancelled_request, CancellationToken::new())
            .await
            .is_err());
        assert!(!frame.exists());
        assert!(!manifest.exists());
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
            mime_type: "image/webp".into(),
            width: 640,
            height: 360,
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
