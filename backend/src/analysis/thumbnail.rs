//! Private, content-addressed thumbnail derivatives.
//!
//! The cache contains only hashed owner/key paths and generated PNG bytes.
//! Original media remains authoritative and is never published from this tree.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::UNIX_EPOCH;

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::library::MediaEntry;

const POSTER_PIPELINE_VERSION: &str = "thumbnail-png-v1-320x180";
const FILMSTRIP_PIPELINE_VERSION: &str = "filmstrip-png-v1-8x160x90";
const MAX_CACHE_ENTRIES: usize = 4_096;
const MAX_POSTER_PNG_BYTES: u64 = 2 * 1024 * 1024;
const MAX_FILMSTRIP_PNG_BYTES: u64 = 4 * 1024 * 1024;
const MIN_PNG_BYTES: u64 = 24;

pub const FILMSTRIP_CELL_COUNT: u32 = 8;
pub const FILMSTRIP_CELL_WIDTH: u32 = 160;
pub const FILMSTRIP_CELL_HEIGHT: u32 = 90;
pub const FILMSTRIP_WIDTH: u32 = FILMSTRIP_CELL_COUNT * FILMSTRIP_CELL_WIDTH;
pub const FILMSTRIP_HEIGHT: u32 = FILMSTRIP_CELL_HEIGHT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailKind {
    Video,
    Image,
    Audio,
}

impl ThumbnailKind {
    fn cache_tag(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Image => "image",
            Self::Audio => "audio",
        }
    }
}

pub fn infer_thumbnail_kind(entry: &MediaEntry) -> Option<ThumbnailKind> {
    match entry.media_type.as_deref() {
        Some("video") => return Some(ThumbnailKind::Video),
        Some("image") => return Some(ThumbnailKind::Image),
        Some("audio") => return Some(ThumbnailKind::Audio),
        Some(_) => return None,
        None => {}
    }
    if entry.vcodec.as_deref().is_some_and(|codec| codec != "none") {
        return Some(ThumbnailKind::Video);
    }
    if entry.acodec.as_deref().is_some_and(|codec| codec != "none") {
        return Some(ThumbnailKind::Audio);
    }
    let extension = Path::new(&entry.filename)
        .extension()
        .and_then(|value| value.to_str())?
        .to_ascii_lowercase();
    if matches!(
        extension.as_str(),
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "tif" | "tiff" | "avif"
    ) {
        Some(ThumbnailKind::Image)
    } else if matches!(
        extension.as_str(),
        "mp3" | "wav" | "m4a" | "aac" | "flac" | "ogg" | "opus" | "aiff" | "aif"
    ) {
        Some(ThumbnailKind::Audio)
    } else if entry.kind == "output"
        || matches!(
            extension.as_str(),
            "mp4" | "mov" | "mkv" | "webm" | "avi" | "m4v" | "mpeg" | "mpg"
        )
    {
        Some(ThumbnailKind::Video)
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct ThumbnailSource {
    pub id: String,
    pub path: PathBuf,
    pub kind: ThumbnailKind,
    pub duration_seconds: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct ThumbnailArtifact {
    pub key: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub width: u32,
    pub height: u32,
}

#[axum::async_trait]
pub trait ThumbnailEncoder: Send + Sync + 'static {
    async fn encode(
        &self,
        source: &ThumbnailSource,
        output: &Path,
        cancellation: &CancellationToken,
    ) -> Result<()>;

    async fn encode_filmstrip(
        &self,
        _source: &ThumbnailSource,
        _output: &Path,
        _cancellation: &CancellationToken,
    ) -> Result<()> {
        Err(anyhow!("filmstrip encoding is unsupported"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Derivative {
    Poster,
    Filmstrip,
}

impl Derivative {
    fn pipeline_version(self) -> &'static str {
        match self {
            Self::Poster => POSTER_PIPELINE_VERSION,
            Self::Filmstrip => FILMSTRIP_PIPELINE_VERSION,
        }
    }

    fn filename(self, key: &str) -> String {
        match self {
            Self::Poster => format!("{key}.png"),
            Self::Filmstrip => format!("filmstrip-{key}.png"),
        }
    }

    fn png_spec(self) -> PngSpec {
        match self {
            Self::Poster => PngSpec {
                max_bytes: MAX_POSTER_PNG_BYTES,
                max_width: 320,
                max_height: 180,
                exact_size: None,
            },
            Self::Filmstrip => PngSpec {
                max_bytes: MAX_FILMSTRIP_PNG_BYTES,
                max_width: FILMSTRIP_WIDTH,
                max_height: FILMSTRIP_HEIGHT,
                exact_size: Some((FILMSTRIP_WIDTH, FILMSTRIP_HEIGHT)),
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PngSpec {
    max_bytes: u64,
    max_width: u32,
    max_height: u32,
    exact_size: Option<(u32, u32)>,
}

#[derive(Clone)]
pub struct ThumbnailService<E> {
    root: PathBuf,
    encoder: Arc<E>,
    render_semaphore: Arc<Semaphore>,
    locks: Arc<Mutex<HashMap<String, Weak<Mutex<()>>>>>,
}

impl<E: ThumbnailEncoder> ThumbnailService<E> {
    pub fn new(root: PathBuf, encoder: Arc<E>, render_semaphore: Arc<Semaphore>) -> Self {
        Self {
            root,
            encoder,
            render_semaphore,
            locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn get_or_create(
        &self,
        source: &ThumbnailSource,
        cancellation: &CancellationToken,
    ) -> Result<ThumbnailArtifact> {
        self.get_or_create_derivative(source, Derivative::Poster, cancellation)
            .await
    }

    pub async fn get_or_create_filmstrip(
        &self,
        source: &ThumbnailSource,
        cancellation: &CancellationToken,
    ) -> Result<ThumbnailArtifact> {
        ensure_filmstrip_source(source)?;
        self.get_or_create_derivative(source, Derivative::Filmstrip, cancellation)
            .await
    }

    async fn get_or_create_derivative(
        &self,
        source: &ThumbnailSource,
        derivative: Derivative,
        cancellation: &CancellationToken,
    ) -> Result<ThumbnailArtifact> {
        validate_source_id(&source.id)?;
        let identity = SourceIdentity::read(&source.path).await?;
        let key = cache_key(source, &identity, derivative);
        let owner = owner_key(&source.id);
        let owner_dir = self.root.join("cache").join(&owner);
        let final_path = owner_dir.join(derivative.filename(&key));
        let png_spec = derivative.png_spec();

        if let Some(artifact) = validate_png(&final_path, &key, png_spec).await? {
            return Ok(artifact);
        }

        // One owner-wide lock keeps poster/filmstrip publishing and deletion
        // linearizable while still allowing unrelated media to run in parallel.
        let lock = self.lock_for(&owner).await;
        let _guard = tokio::select! {
            guard = lock.lock() => guard,
            _ = cancellation.cancelled() => return Err(anyhow!("thumbnail generation cancelled")),
        };
        if let Some(artifact) = validate_png(&final_path, &key, png_spec).await? {
            return Ok(artifact);
        }

        let _permit = tokio::select! {
            permit = self.render_semaphore.clone().acquire_owned() => {
                permit.map_err(|_| anyhow!("thumbnail renderer is shutting down"))?
            }
            _ = cancellation.cancelled() => return Err(anyhow!("thumbnail generation cancelled")),
        };
        tokio::fs::create_dir_all(&owner_dir)
            .await
            .context("create thumbnail cache directory")?;
        let staging_dir = self.root.join("staging");
        tokio::fs::create_dir_all(&staging_dir)
            .await
            .context("create thumbnail staging directory")?;
        let staging_path = staging_dir.join(format!(
            "{}-{}.tmp.png",
            derivative.pipeline_version(),
            Uuid::new_v4()
        ));
        let mut staging = StagingFile::new(staging_path.clone());

        match derivative {
            Derivative::Poster => {
                self.encoder
                    .encode(source, &staging_path, cancellation)
                    .await?;
            }
            Derivative::Filmstrip => {
                self.encoder
                    .encode_filmstrip(source, &staging_path, cancellation)
                    .await?;
            }
        }
        if cancellation.is_cancelled() {
            return Err(anyhow!("thumbnail generation cancelled"));
        }
        let current_identity = SourceIdentity::read(&source.path).await?;
        if current_identity != identity {
            return Err(anyhow!("source changed while thumbnail was generated"));
        }
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .open(&staging_path)
            .await
            .context("open generated thumbnail for sync")?;
        file.sync_all().await.context("sync generated thumbnail")?;
        drop(file);
        let generated = validate_png(&staging_path, &key, png_spec)
            .await?
            .ok_or_else(|| anyhow!("thumbnail encoder produced an invalid PNG"))?;
        tokio::fs::rename(&staging_path, &final_path)
            .await
            .context("publish generated thumbnail")?;
        staging.published = true;
        remove_obsolete_owner_files(&owner_dir, source, &identity).await;

        Ok(ThumbnailArtifact {
            path: final_path,
            ..generated
        })
    }

    pub async fn current_key(&self, source: &ThumbnailSource) -> Result<String> {
        validate_source_id(&source.id)?;
        let identity = SourceIdentity::read(&source.path).await?;
        Ok(cache_key(source, &identity, Derivative::Poster))
    }

    pub async fn current_filmstrip_key(&self, source: &ThumbnailSource) -> Result<String> {
        validate_source_id(&source.id)?;
        ensure_filmstrip_source(source)?;
        let identity = SourceIdentity::read(&source.path).await?;
        Ok(cache_key(source, &identity, Derivative::Filmstrip))
    }

    pub async fn remove_source(&self, source_id: &str) -> Result<()> {
        validate_source_id(source_id)?;
        let owner = owner_key(source_id);
        let lock = self.lock_for(&owner).await;
        let _guard = lock.lock().await;
        let owner_dir = self.root.join("cache").join(owner);
        match tokio::fs::remove_dir_all(owner_dir).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("remove thumbnail cache"),
        }
    }

    /// Remove partial files and derivatives that no longer match a live
    /// library source. Directory traversal is bounded to avoid startup stalls.
    pub async fn cleanup_stale(&self, sources: &[ThumbnailSource]) -> Result<()> {
        let mut expected = HashMap::<String, HashSet<String>>::new();
        for source in sources.iter().take(MAX_CACHE_ENTRIES) {
            if validate_source_id(&source.id).is_err() {
                continue;
            }
            let Ok(identity) = SourceIdentity::read(&source.path).await else {
                continue;
            };
            expected
                .entry(owner_key(&source.id))
                .or_default()
                .extend(expected_cache_files(source, &identity));
        }

        let cache_root = self.root.join("cache");
        tokio::fs::create_dir_all(&cache_root).await?;
        let mut owners = tokio::fs::read_dir(&cache_root).await?;
        let mut owner_count = 0usize;
        while owner_count < MAX_CACHE_ENTRIES {
            let Some(owner) = owners.next_entry().await? else {
                break;
            };
            owner_count += 1;
            let name = owner.file_name().to_string_lossy().into_owned();
            let file_type = owner.file_type().await?;
            if !file_type.is_dir() || !is_sha256(&name) || !expected.contains_key(&name) {
                remove_cache_entry(owner.path(), file_type.is_dir()).await;
                continue;
            }
            let keep = &expected[&name];
            let mut files = tokio::fs::read_dir(owner.path()).await?;
            let mut file_count = 0usize;
            while file_count < MAX_CACHE_ENTRIES {
                let Some(file) = files.next_entry().await? else {
                    break;
                };
                file_count += 1;
                let file_name = file.file_name().to_string_lossy().into_owned();
                if !keep.contains(&file_name) {
                    remove_cache_entry(file.path(), file.file_type().await?.is_dir()).await;
                }
            }
        }

        let staging_root = self.root.join("staging");
        if let Ok(mut staging) = tokio::fs::read_dir(&staging_root).await {
            let mut count = 0usize;
            while count < MAX_CACHE_ENTRIES {
                let Some(entry) = staging.next_entry().await? else {
                    break;
                };
                count += 1;
                remove_cache_entry(entry.path(), entry.file_type().await?.is_dir()).await;
            }
        }
        Ok(())
    }

    async fn lock_for(&self, key: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(key).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(Mutex::new(()));
        locks.insert(key.to_owned(), Arc::downgrade(&lock));
        lock
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceIdentity {
    size: u64,
    modified_nanos: u128,
}

impl SourceIdentity {
    async fn read(path: &Path) -> Result<Self> {
        let metadata = tokio::fs::metadata(path)
            .await
            .context("read thumbnail source metadata")?;
        if !metadata.is_file() {
            return Err(anyhow!("thumbnail source is not a regular file"));
        }
        let modified_nanos = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        Ok(Self {
            size: metadata.len(),
            modified_nanos,
        })
    }
}

fn cache_key(
    source: &ThumbnailSource,
    identity: &SourceIdentity,
    derivative: Derivative,
) -> String {
    let mut hash = Sha256::new();
    hash.update(derivative.pipeline_version().as_bytes());
    hash.update([0]);
    hash.update(source.id.as_bytes());
    hash.update([0]);
    hash.update(source.kind.cache_tag().as_bytes());
    hash.update(identity.size.to_le_bytes());
    hash.update(identity.modified_nanos.to_le_bytes());
    hash.update(
        source
            .duration_seconds
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or_default()
            .to_bits()
            .to_le_bytes(),
    );
    format!("{:x}", hash.finalize())
}

fn expected_cache_files(source: &ThumbnailSource, identity: &SourceIdentity) -> HashSet<String> {
    let mut files = HashSet::from([Derivative::Poster.filename(&cache_key(
        source,
        identity,
        Derivative::Poster,
    ))]);
    if ensure_filmstrip_source(source).is_ok() {
        files.insert(Derivative::Filmstrip.filename(&cache_key(
            source,
            identity,
            Derivative::Filmstrip,
        )));
    }
    files
}

fn ensure_filmstrip_source(source: &ThumbnailSource) -> Result<()> {
    let duration = source
        .duration_seconds
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| anyhow!("filmstrip requires a finite positive duration"))?;
    if source.kind != ThumbnailKind::Video || duration > 24.0 * 60.0 * 60.0 {
        return Err(anyhow!("filmstrip requires a bounded video source"));
    }
    Ok(())
}

fn owner_key(source_id: &str) -> String {
    format!("{:x}", Sha256::digest(source_id.as_bytes()))
}

fn validate_source_id(source_id: &str) -> Result<()> {
    if source_id.is_empty()
        || source_id.len() > 128
        || !source_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || !source_id.bytes().any(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(anyhow!("invalid thumbnail source id"));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

async fn validate_png(path: &Path, key: &str, spec: PngSpec) -> Result<Option<ThumbnailArtifact>> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("read cached thumbnail metadata"),
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || !(MIN_PNG_BYTES..=spec.max_bytes).contains(&metadata.len())
    {
        return Ok(None);
    }
    let mut file = tokio::fs::File::open(path).await?;
    let mut header = [0u8; 24];
    file.read_exact(&mut header).await?;
    if header[..8] != [137, 80, 78, 71, 13, 10, 26, 10] || &header[12..16] != b"IHDR" {
        return Ok(None);
    }
    let width = u32::from_be_bytes(header[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(header[20..24].try_into().unwrap());
    if width == 0
        || height == 0
        || width > spec.max_width
        || height > spec.max_height
        || spec
            .exact_size
            .is_some_and(|expected| expected != (width, height))
    {
        return Ok(None);
    }
    Ok(Some(ThumbnailArtifact {
        key: key.to_owned(),
        path: path.to_path_buf(),
        size_bytes: metadata.len(),
        width,
        height,
    }))
}

async fn remove_obsolete_owner_files(
    owner_dir: &Path,
    source: &ThumbnailSource,
    identity: &SourceIdentity,
) {
    let Ok(mut entries) = tokio::fs::read_dir(owner_dir).await else {
        return;
    };
    let current = expected_cache_files(source, identity);
    let mut count = 0usize;
    while count < MAX_CACHE_ENTRIES {
        let Ok(Some(entry)) = entries.next_entry().await else {
            break;
        };
        count += 1;
        if !current.contains(entry.file_name().to_string_lossy().as_ref()) {
            if let Ok(file_type) = entry.file_type().await {
                remove_cache_entry(entry.path(), file_type.is_dir()).await;
            }
        }
    }
}

async fn remove_cache_entry(path: PathBuf, directory: bool) {
    let result = if directory {
        tokio::fs::remove_dir_all(&path).await
    } else {
        tokio::fs::remove_file(&path).await
    };
    if let Err(error) = result {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(%error, "failed to clean thumbnail cache entry");
        }
    }
}

struct StagingFile {
    path: PathBuf,
    published: bool,
}

impl StagingFile {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            published: false,
        }
    }
}

impl Drop for StagingFile {
    fn drop(&mut self) {
        if !self.published {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use tokio::io::AsyncWriteExt;

    use super::*;

    const PNG_1X1: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137,
    ];

    #[derive(Default)]
    struct FakeEncoder {
        calls: AtomicUsize,
        filmstrip_calls: AtomicUsize,
    }

    #[axum::async_trait]
    impl ThumbnailEncoder for FakeEncoder {
        async fn encode(
            &self,
            _source: &ThumbnailSource,
            output: &Path,
            _cancellation: &CancellationToken,
        ) -> Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
            let mut file = tokio::fs::File::create(output).await?;
            file.write_all(PNG_1X1).await?;
            Ok(())
        }

        async fn encode_filmstrip(
            &self,
            _source: &ThumbnailSource,
            output: &Path,
            _cancellation: &CancellationToken,
        ) -> Result<()> {
            self.filmstrip_calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
            let mut header = PNG_1X1.to_vec();
            header[16..20].copy_from_slice(&FILMSTRIP_WIDTH.to_be_bytes());
            header[20..24].copy_from_slice(&FILMSTRIP_HEIGHT.to_be_bytes());
            let mut file = tokio::fs::File::create(output).await?;
            file.write_all(&header).await?;
            Ok(())
        }
    }

    async fn fixture() -> (
        tempfile::TempDir,
        ThumbnailSource,
        Arc<FakeEncoder>,
        ThumbnailService<FakeEncoder>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let source_path = dir.path().join("source.bin");
        tokio::fs::write(&source_path, b"first").await.unwrap();
        let source = ThumbnailSource {
            id: "media-1".into(),
            path: source_path,
            kind: ThumbnailKind::Video,
            duration_seconds: Some(12.0),
        };
        let encoder = Arc::new(FakeEncoder::default());
        let service = ThumbnailService::new(
            dir.path().join("thumbnails"),
            encoder.clone(),
            Arc::new(Semaphore::new(2)),
        );
        (dir, source, encoder, service)
    }

    #[tokio::test]
    async fn cache_reuses_and_singleflights_generation() {
        let (_dir, source, encoder, service) = fixture().await;
        let cancel = CancellationToken::new();
        let (first, second) = tokio::join!(
            service.get_or_create(&source, &cancel),
            service.get_or_create(&source, &cancel)
        );
        assert_eq!(first.unwrap().key, second.unwrap().key);
        assert_eq!(encoder.calls.load(Ordering::SeqCst), 1);
        service.get_or_create(&source, &cancel).await.unwrap();
        assert_eq!(encoder.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn source_change_invalidates_old_cache_and_delete_removes_owner() {
        let (dir, source, encoder, service) = fixture().await;
        let cancel = CancellationToken::new();
        let first = service.get_or_create(&source, &cancel).await.unwrap();
        tokio::time::sleep(Duration::from_millis(2)).await;
        tokio::fs::write(&source.path, b"second-content")
            .await
            .unwrap();
        let second = service.get_or_create(&source, &cancel).await.unwrap();
        assert_ne!(first.key, second.key);
        assert_eq!(encoder.calls.load(Ordering::SeqCst), 2);
        assert!(!first.path.exists());

        service.remove_source(&source.id).await.unwrap();
        assert!(!second.path.exists());
        assert!(dir.path().join("thumbnails").exists());
    }

    #[tokio::test]
    async fn filmstrip_has_fixed_geometry_singleflights_and_preserves_poster() {
        let (_dir, source, encoder, service) = fixture().await;
        let cancel = CancellationToken::new();
        let poster = service.get_or_create(&source, &cancel).await.unwrap();
        let (first, second) = tokio::join!(
            service.get_or_create_filmstrip(&source, &cancel),
            service.get_or_create_filmstrip(&source, &cancel)
        );
        let first = first.unwrap();
        assert_eq!(first.key, second.unwrap().key);
        assert_eq!(
            (first.width, first.height),
            (FILMSTRIP_WIDTH, FILMSTRIP_HEIGHT)
        );
        assert_eq!(encoder.filmstrip_calls.load(Ordering::SeqCst), 1);
        assert!(poster.path.exists());
        assert!(first.path.exists());

        service
            .cleanup_stale(std::slice::from_ref(&source))
            .await
            .unwrap();
        assert!(poster.path.exists());
        assert!(first.path.exists());

        tokio::time::sleep(Duration::from_millis(2)).await;
        tokio::fs::write(&source.path, b"changed-video")
            .await
            .unwrap();
        let refreshed = service
            .get_or_create_filmstrip(&source, &cancel)
            .await
            .unwrap();
        assert_ne!(first.key, refreshed.key);
        assert!(!first.path.exists());
        assert!(!poster.path.exists());
        assert_eq!(encoder.filmstrip_calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn owner_delete_waits_for_generation_and_leaves_no_late_filmstrip() {
        let (_dir, source, encoder, service) = fixture().await;
        let service = Arc::new(service);
        let task_service = service.clone();
        let task_source = source.clone();
        let task = tokio::spawn(async move {
            task_service
                .get_or_create_filmstrip(&task_source, &CancellationToken::new())
                .await
        });
        while encoder.filmstrip_calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        tokio::fs::remove_file(&source.path).await.unwrap();
        service.remove_source(&source.id).await.unwrap();
        assert!(task.await.unwrap().is_err());

        let owner_dir = service.root.join("cache").join(owner_key(&source.id));
        assert!(!owner_dir.exists());
    }

    #[tokio::test]
    async fn startup_cleanup_removes_orphans_and_partial_files() {
        let (dir, source, _encoder, service) = fixture().await;
        let orphan = dir.path().join("thumbnails/cache").join("a".repeat(64));
        tokio::fs::create_dir_all(&orphan).await.unwrap();
        tokio::fs::write(orphan.join(format!("{}.png", "b".repeat(64))), PNG_1X1)
            .await
            .unwrap();
        let staging = dir.path().join("thumbnails/staging");
        tokio::fs::create_dir_all(&staging).await.unwrap();
        tokio::fs::write(staging.join("partial.tmp.png"), b"partial")
            .await
            .unwrap();

        service.cleanup_stale(&[source]).await.unwrap();
        assert!(!orphan.exists());
        assert!(!staging.join("partial.tmp.png").exists());
    }
}
