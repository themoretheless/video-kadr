//! Proxy media is a disposable derivative of an immutable source identity.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::SystemTime;

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
use crate::domain::color_management::ColorManagementStatusV1;
use crate::domain::media_probe::{ProbeResult, Rational, StreamKind};
use crate::runtime::cpu_pool::CpuPool;
use crate::runtime::TaskSupervisor;

const PROXY_SCHEMA_VERSION: u32 = 4;
pub const FFMPEG_PROXY_COMPATIBILITY: &str = "ffmpeg-proxy-v4-sdr-color-management-v1";

#[derive(Clone)]
struct VerifiedProxyFile {
    size: u64,
    modified: SystemTime,
    sha256: Fingerprint,
}

static VERIFIED_PROXY_FILES: OnceLock<Mutex<HashMap<String, VerifiedProxyFile>>> = OnceLock::new();

fn verified_proxy_files() -> &'static Mutex<HashMap<String, VerifiedProxyFile>> {
    VERIFIED_PROXY_FILES.get_or_init(|| Mutex::new(HashMap::new()))
}

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
    pub producer_compatibility: String,
    pub source_media: ProxyMediaProvenance,
    pub proxy_media: ProxyMediaProvenance,
    pub file: ArtifactFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaClock {
    pub time_base: Rational,
    pub duration_ticks: i64,
    pub start_ticks: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyMediaProvenance {
    pub clock: MediaClock,
    pub audio_clock: Option<MediaClock>,
    pub coded_width: u32,
    pub coded_height: u32,
    pub display_width: u32,
    pub display_height: u32,
    pub video_codec: String,
    pub frame_rate: Option<Rational>,
    pub audio_codec: Option<String>,
    pub audio_sample_rate: Option<u32>,
    pub audio_channels: Option<u32>,
    pub color_management: ColorManagementStatusV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaIntent {
    Preview,
    Export,
}

#[axum::async_trait]
pub trait ProxyEncoder: Send + Sync {
    fn producer_compatibility(&self) -> &'static str;

    async fn generate(
        &self,
        source: &SourceIdentity,
        profile: &ProxyProfile,
        staging_path: &Path,
        cancellation: &CancellationToken,
    ) -> Result<()>;

    async fn probe(&self, path: &Path) -> Result<ProbeResult>;
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

    pub fn producer_compatibility(&self) -> &'static str {
        self.encoder.producer_compatibility()
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
        let current = fingerprint_file(
            &self.cpu_pool,
            source.original_path.clone(),
            cancellation.child_token(),
        )
        .await?;
        if current.sha256 != source.fingerprint {
            return Err(anyhow!("proxy source changed after it was scheduled"));
        }
        let compatibility = self.encoder.producer_compatibility();
        let key = proxy_key(&source, &profile, compatibility);
        let lock = self.key_lock(&key).await;
        let _guard = lock.lock().await;
        let result = self
            .ensure_locked(source, profile, key.clone(), cancellation)
            .await;
        drop(_guard);
        self.release_key_lock(&key, &lock).await;
        result
    }

    /// Resolve a catalog entry by content key without accepting a filesystem
    /// path from the caller. The returned path is checksum-verified and
    /// contained below the proxy root.
    pub async fn validated_artifact(
        &self,
        key: &Fingerprint,
        cancellation: CancellationToken,
    ) -> Result<(ProxyArtifact, PathBuf)> {
        let manifest = self.root.join(proxy_manifest_path(key));
        let artifact: ProxyArtifact = read_json_bounded(&manifest, DEFAULT_MANIFEST_LIMIT).await?;
        if artifact.schema_version != PROXY_SCHEMA_VERSION
            || artifact.key != *key
            || artifact.producer_compatibility != self.encoder.producer_compatibility()
            || proxy_key_from_parts(
                &artifact.source_fingerprint,
                &artifact.profile,
                &artifact.producer_compatibility,
            ) != *key
            || validate_proxy_media(
                &artifact.source_media,
                &artifact.proxy_media,
                &artifact.profile,
            )
            .is_err()
        {
            return Err(anyhow!("proxy artifact is stale"));
        }
        let expected = proxy_relative_path(key, &artifact.profile);
        if path_token(&expected)? != artifact.file.path {
            return Err(anyhow!("proxy artifact locator is invalid"));
        }
        let expected_path = self.root.join(&expected);
        let metadata = tokio::fs::metadata(&expected_path).await?;
        let modified = metadata.modified().ok();
        let cache_key = format!("{}:{key}", self.root.display());
        let cached = if let Some(modified) = modified {
            verified_proxy_files()
                .lock()
                .await
                .get(&cache_key)
                .is_some_and(|entry| {
                    entry.size == metadata.len()
                        && entry.modified == modified
                        && entry.sha256 == artifact.file.sha256
                })
        } else {
            false
        };
        let path = if cached {
            expected_path
        } else {
            let verified = artifact
                .file
                .verify(&self.root, &self.cpu_pool, cancellation)
                .await?;
            if let Some(modified) = modified {
                verified_proxy_files().lock().await.insert(
                    cache_key,
                    VerifiedProxyFile {
                        size: metadata.len(),
                        modified,
                        sha256: artifact.file.sha256.clone(),
                    },
                );
            }
            verified
        };
        Ok((artifact, path))
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
                proxy.source_fingerprint == source.fingerprint
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

    /// Remove a catalog entry by content key before a durable queue re-run.
    /// The artifact path is always re-derived from the trusted key/profile;
    /// a locator stored in an invalid manifest is never followed.
    pub async fn invalidate_key(&self, key: &Fingerprint) -> Result<()> {
        let manifest_path = self.root.join(proxy_manifest_path(key));
        if let Ok(artifact) =
            read_json_bounded::<ProxyArtifact>(&manifest_path, DEFAULT_MANIFEST_LIMIT).await
        {
            remove_if_exists(&self.root.join(proxy_relative_path(key, &artifact.profile))).await?;
        }
        remove_if_exists(&manifest_path).await?;
        verified_proxy_files()
            .lock()
            .await
            .remove(&format!("{}:{key}", self.root.display()));
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
            && artifact.source_fingerprint == source.fingerprint
            && artifact.profile == *profile
            && artifact.producer_compatibility == self.encoder.producer_compatibility()
            && proxy_key_from_parts(
                &artifact.source_fingerprint,
                &artifact.profile,
                &artifact.producer_compatibility,
            ) == *key
            && path_token(&proxy_relative_path(key, profile))
                .ok()
                .as_deref()
                == Some(artifact.file.path.as_str())
            && validate_proxy_media(&artifact.source_media, &artifact.proxy_media, profile).is_ok();
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
        let identity = fingerprint_file(
            &self.cpu_pool,
            staging.to_path_buf(),
            cancellation.child_token(),
        )
        .await?;
        let source_probe = self.encoder.probe(&source.original_path).await?;
        let proxy_probe = self.encoder.probe(staging).await?;
        let source_media = media_provenance(&source_probe)?;
        let proxy_media = media_provenance(&proxy_probe)?;
        validate_proxy_media(&source_media, &proxy_media, &profile)?;
        let current_source = fingerprint_file(
            &self.cpu_pool,
            source.original_path.clone(),
            cancellation.child_token(),
        )
        .await?;
        if current_source.sha256 != source.fingerprint {
            return Err(anyhow!("proxy source changed while it was being encoded"));
        }
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
            producer_compatibility: self.encoder.producer_compatibility().to_owned(),
            source_media,
            proxy_media,
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

pub fn proxy_key(
    source: &SourceIdentity,
    profile: &ProxyProfile,
    producer_compatibility: &str,
) -> Fingerprint {
    proxy_key_from_parts(&source.fingerprint, profile, producer_compatibility)
}

fn proxy_key_from_parts(
    source_fingerprint: &Fingerprint,
    profile: &ProxyProfile,
    producer_compatibility: &str,
) -> Fingerprint {
    Fingerprint::combine([
        b"proxy-v4-sdr-color-management-v1".as_slice(),
        source_fingerprint.as_str().as_bytes(),
        profile.fingerprint().as_str().as_bytes(),
        producer_compatibility.as_bytes(),
    ])
}

fn media_provenance(probe: &ProbeResult) -> Result<ProxyMediaProvenance> {
    let video = probe
        .streams
        .iter()
        .find(|stream| stream.kind == StreamKind::Video)
        .ok_or_else(|| anyhow!("proxy media has no video stream"))?;
    let time_base = video
        .time_base
        .ok_or_else(|| anyhow!("proxy video time base is missing"))?;
    if time_base.numerator <= 0 || time_base.denominator <= 0 {
        return Err(anyhow!("proxy video time base is invalid"));
    }
    let seconds_per_tick = time_base.as_f64();
    let duration = video.duration_seconds.unwrap_or(probe.duration);
    let start = video
        .start_time_seconds
        .or(probe.container.start_time_seconds)
        .unwrap_or(0.0);
    if !duration.is_finite() || duration <= 0.0 || !start.is_finite() {
        return Err(anyhow!("proxy media clock is invalid"));
    }
    let coded_width = video.width.unwrap_or(probe.width);
    let coded_height = video.height.unwrap_or(probe.height);
    if coded_width == 0 || coded_height == 0 {
        return Err(anyhow!("proxy video dimensions are invalid"));
    }
    let (display_width, display_height) = if matches!(video.rotation_degrees, 90 | 270) {
        (coded_height, coded_width)
    } else {
        (coded_width, coded_height)
    };
    let audio = probe
        .streams
        .iter()
        .find(|stream| stream.kind == StreamKind::Audio);
    Ok(ProxyMediaProvenance {
        clock: MediaClock {
            time_base,
            duration_ticks: (duration / seconds_per_tick).round() as i64,
            start_ticks: (start / seconds_per_tick).round() as i64,
        },
        audio_clock: audio
            .map(|stream| {
                let time_base = stream
                    .time_base
                    .ok_or_else(|| anyhow!("proxy audio time base is missing"))?;
                let duration = stream.duration_seconds.unwrap_or(probe.duration);
                let start = stream
                    .start_time_seconds
                    .or(probe.container.start_time_seconds)
                    .unwrap_or(0.0);
                if !duration.is_finite() || duration <= 0.0 || !start.is_finite() {
                    return Err(anyhow!("proxy audio clock is invalid"));
                }
                Ok(MediaClock {
                    time_base,
                    duration_ticks: (duration / time_base.as_f64()).round() as i64,
                    start_ticks: (start / time_base.as_f64()).round() as i64,
                })
            })
            .transpose()?,
        coded_width,
        coded_height,
        display_width,
        display_height,
        video_codec: video
            .codec_name
            .clone()
            .ok_or_else(|| anyhow!("proxy video codec is missing"))?,
        frame_rate: video.frame_rate,
        audio_codec: audio.and_then(|stream| stream.codec_name.clone()),
        audio_sample_rate: audio.and_then(|stream| stream.sample_rate),
        audio_channels: audio.and_then(|stream| stream.channels),
        color_management: video.color.color_management.clone(),
    })
}

fn validate_proxy_media(
    source: &ProxyMediaProvenance,
    proxy: &ProxyMediaProvenance,
    profile: &ProxyProfile,
) -> Result<()> {
    let valid_color = match profile.codec {
        ProxyCodec::H264 => {
            proxy.color_management == ColorManagementStatusV1::rec709_limited_output()
        }
        ProxyCodec::ProresProxy => proxy.color_management.is_prores_rec709_limited_output(),
    };
    if !valid_color {
        return Err(anyhow!("proxy output is not tagged SDR Rec.709 limited"));
    }
    let mapping_tolerance = media_mapping_tolerance(source, proxy);
    if timeline_start_seconds(proxy).abs() > mapping_tolerance {
        return Err(anyhow!("proxy timestamps are not normalized to zero"));
    }
    if proxy.display_width > profile.max_width
        || proxy.display_width == 0
        || proxy.display_height == 0
    {
        return Err(anyhow!("proxy dimensions violate the profile"));
    }
    if (!profile.include_audio && proxy.audio_codec.is_some())
        || (profile.include_audio && source.audio_codec.is_some() && proxy.audio_codec.is_none())
    {
        return Err(anyhow!("proxy audio policy was not preserved"));
    }
    if let (Some(source_audio), Some(proxy_audio)) = (&source.audio_clock, &proxy.audio_clock) {
        let source_offset = clock_start_seconds(source_audio) - clock_start_seconds(&source.clock);
        let proxy_offset = clock_start_seconds(proxy_audio) - clock_start_seconds(&proxy.clock);
        if (source_offset - proxy_offset).abs() > mapping_tolerance {
            return Err(anyhow!("proxy audio/video offset diverges from source"));
        }
    }
    let source_seconds = source.clock.duration_ticks as f64 * source.clock.time_base.as_f64();
    let proxy_seconds = proxy.clock.duration_ticks as f64 * proxy.clock.time_base.as_f64();
    let frame_tolerance = source
        .frame_rate
        .or(proxy.frame_rate)
        .map(|rate| 1.0 / rate.as_f64())
        .unwrap_or(0.1)
        .max(0.001);
    if (source_seconds - proxy_seconds).abs() > frame_tolerance {
        return Err(anyhow!("proxy duration diverges from source"));
    }
    Ok(())
}

fn clock_start_seconds(clock: &MediaClock) -> f64 {
    clock.start_ticks as f64 * clock.time_base.as_f64()
}

fn timeline_start_seconds(media: &ProxyMediaProvenance) -> f64 {
    media
        .audio_clock
        .as_ref()
        .map(clock_start_seconds)
        .unwrap_or_else(|| clock_start_seconds(&media.clock))
        .min(clock_start_seconds(&media.clock))
}

fn media_mapping_tolerance(source: &ProxyMediaProvenance, proxy: &ProxyMediaProvenance) -> f64 {
    source
        .frame_rate
        .or(proxy.frame_rate)
        .map(|rate| 1.0 / rate.as_f64())
        .unwrap_or(0.05)
        .max(0.05)
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
        fn producer_compatibility(&self) -> &'static str {
            "fake-proxy-v2"
        }

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

        async fn probe(&self, _path: &Path) -> Result<ProbeResult> {
            Ok(ProbeResult::from_ffprobe_json(&serde_json::json!({
                "format": {"duration":"10.0", "start_time":"0"},
                "streams": [
                    {"index":0,"codec_type":"video","codec_name":"h264","width":960,"height":540,"avg_frame_rate":"30/1","time_base":"1/90000","start_time":"0","duration":"10.0","pix_fmt":"yuv420p","color_range":"tv","color_space":"bt709","color_transfer":"bt709","color_primaries":"bt709","chroma_location":"left"},
                    {"index":1,"codec_type":"audio","codec_name":"aac","sample_rate":"48000","channels":2,"time_base":"1/48000","start_time":"0","duration":"10.0"}
                ]
            }))?)
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
        let alias_path = root.path().join("alias.mp4");
        tokio::fs::write(&alias_path, b"full resolution")
            .await
            .unwrap();
        let alias = service
            .inspect_source(
                SourceMedia {
                    id: "alias".into(),
                    original_path: alias_path.clone(),
                    duration_seconds: 10.0,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let shared = service
            .ensure(
                alias.clone(),
                ProxyProfile::default(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(shared.key, first.key);
        assert_eq!(encoder.calls.load(Ordering::SeqCst), 1);
        assert_ne!(
            service.resolve(&alias, Some(&shared), MediaIntent::Preview),
            alias_path
        );
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
    async fn source_change_after_scheduling_is_rejected_before_encode_or_reuse() {
        let root = tempfile::tempdir().unwrap();
        let source_path = root.path().join("source.mp4");
        tokio::fs::write(&source_path, b"scheduled bytes")
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
        tokio::fs::write(&source_path, b"changed bytes")
            .await
            .unwrap();
        assert!(service
            .ensure(source, ProxyProfile::default(), CancellationToken::new())
            .await
            .is_err());
        assert_eq!(encoder.calls.load(Ordering::SeqCst), 0);
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

    #[test]
    fn compatibility_changes_identity_and_validation_rejects_bad_time_mapping() {
        let source = SourceIdentity {
            id: "source".into(),
            original_path: PathBuf::from("source.mp4"),
            duration_seconds: 10.0,
            fingerprint: Fingerprint::digest(b"source"),
        };
        let profile = ProxyProfile::default();
        assert_ne!(
            proxy_key(&source, &profile, "producer-a"),
            proxy_key(&source, &profile, "producer-b")
        );
        let media = ProxyMediaProvenance {
            clock: MediaClock {
                time_base: Rational {
                    numerator: 1,
                    denominator: 90_000,
                },
                duration_ticks: 900_000,
                start_ticks: 0,
            },
            audio_clock: Some(MediaClock {
                time_base: Rational {
                    numerator: 1,
                    denominator: 48_000,
                },
                duration_ticks: 480_000,
                start_ticks: 0,
            }),
            coded_width: 960,
            coded_height: 540,
            display_width: 960,
            display_height: 540,
            video_codec: "h264".into(),
            frame_rate: Some(Rational {
                numerator: 30,
                denominator: 1,
            }),
            audio_codec: Some("aac".into()),
            audio_sample_rate: Some(48_000),
            audio_channels: Some(2),
            color_management: ColorManagementStatusV1::rec709_limited_output(),
        };
        assert!(validate_proxy_media(&media, &media, &profile).is_ok());
        let mut shifted = media.clone();
        shifted.clock.start_ticks = 9_000;
        assert!(validate_proxy_media(&media, &shifted, &profile).is_err());
        let mut short = media.clone();
        short.clock.duration_ticks /= 2;
        assert!(validate_proxy_media(&media, &short, &profile).is_err());
        let mut source_offset = media.clone();
        source_offset.audio_clock.as_mut().unwrap().start_ticks = 4_800;
        let mut proxy_lost_offset = media.clone();
        proxy_lost_offset.audio_clock.as_mut().unwrap().start_ticks = 0;
        assert!(validate_proxy_media(&source_offset, &proxy_lost_offset, &profile).is_err());
    }
}
