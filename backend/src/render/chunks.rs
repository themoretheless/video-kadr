//! Scene-aware resumable encode manifest.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::artifacts::{
    fingerprint_file, path_token, read_json_bounded, write_json_atomic, ArtifactFile,
    DEFAULT_MANIFEST_LIMIT,
};
use crate::domain::artifact_graph::Fingerprint;
use crate::runtime::cpu_pool::CpuPool;

const CHUNK_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneBoundary {
    pub frame: u64,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameRange {
    pub start: u64,
    /// Exclusive.
    pub end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaParameters {
    pub container: String,
    pub video_codec: String,
    pub pixel_format: String,
    pub width: u32,
    pub height: u32,
    pub time_base_numerator: u32,
    pub time_base_denominator: u32,
    pub audio_codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
}

impl MediaParameters {
    pub fn validate(&self) -> Result<()> {
        if self.container.is_empty()
            || self.video_codec.is_empty()
            || self.pixel_format.is_empty()
            || self.width == 0
            || self.height == 0
            || self.time_base_numerator == 0
            || self.time_base_denominator == 0
        {
            return Err(anyhow!("chunk media parameters are incomplete"));
        }
        if self.audio_codec.is_some()
            && (self.audio_codec.as_deref() == Some("")
                || self.sample_rate.unwrap_or(0) == 0
                || self.channels.unwrap_or(0) == 0)
        {
            return Err(anyhow!("audio chunk parameters are incomplete"));
        }
        if self.audio_codec.is_none() && (self.sample_rate.is_some() || self.channels.is_some()) {
            return Err(anyhow!("audio parameters require an audio codec"));
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::digest(
            &serde_json::to_vec(self).expect("MediaParameters serialization cannot fail"),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChunkSpec {
    pub index: u32,
    pub frames: FrameRange,
    pub key: Fingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChunkEntry {
    pub spec: ChunkSpec,
    pub file: Option<ArtifactFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChunkManifest {
    pub schema_version: u32,
    pub manifest_id: Fingerprint,
    pub plan_fingerprint: Fingerprint,
    pub source_fingerprint: Fingerprint,
    pub output_fingerprint: Fingerprint,
    pub parameters: MediaParameters,
    pub chunks: Vec<ChunkEntry>,
}

impl ChunkManifest {
    #[allow(clippy::too_many_arguments)]
    pub fn plan(
        plan_fingerprint: Fingerprint,
        source_fingerprint: Fingerprint,
        output_fingerprint: Fingerprint,
        parameters: MediaParameters,
        total_frames: u64,
        max_chunk_frames: u64,
        scene_boundaries: &[SceneBoundary],
    ) -> Result<Self> {
        parameters.validate()?;
        if total_frames == 0 || max_chunk_frames == 0 {
            return Err(anyhow!("chunk plan needs positive frame counts"));
        }
        let confirmed: Vec<_> = scene_boundaries
            .iter()
            .filter(|boundary| boundary.confirmed && boundary.frame > 0)
            .filter(|boundary| boundary.frame < total_frames)
            .map(|boundary| boundary.frame)
            .collect();
        let mut start = 0_u64;
        let mut chunks = Vec::new();
        while start < total_frames {
            let hard_end = start.saturating_add(max_chunk_frames).min(total_frames);
            let end = confirmed
                .iter()
                .copied()
                .filter(|frame| *frame > start && *frame <= hard_end)
                .max()
                .unwrap_or(hard_end);
            let index = u32::try_from(chunks.len()).map_err(|_| anyhow!("too many chunks"))?;
            let key = chunk_identity(
                index,
                FrameRange { start, end },
                &plan_fingerprint,
                &source_fingerprint,
                &output_fingerprint,
                &parameters,
            );
            chunks.push(ChunkEntry {
                spec: ChunkSpec {
                    index,
                    frames: FrameRange { start, end },
                    key,
                },
                file: None,
            });
            start = end;
        }
        let manifest_id = manifest_identity(
            &plan_fingerprint,
            &source_fingerprint,
            &output_fingerprint,
            &parameters,
            &chunks,
        );
        Ok(Self {
            schema_version: CHUNK_SCHEMA_VERSION,
            manifest_id,
            plan_fingerprint,
            source_fingerprint,
            output_fingerprint,
            parameters,
            chunks,
        })
    }

    pub fn directory(&self) -> PathBuf {
        PathBuf::from("chunks").join(self.manifest_id.as_str())
    }

    pub fn manifest_path(&self, root: &Path) -> PathBuf {
        root.join(self.directory()).join("manifest.json")
    }

    pub fn expected_chunk_path(&self, index: u32) -> Result<PathBuf> {
        let entry = self.entry(index)?;
        Ok(self
            .directory()
            .join(format!("{:06}-{}.chunk", index, entry.spec.key)))
    }

    pub async fn save(&self, root: &Path) -> Result<()> {
        self.validate_identity()?;
        write_json_atomic(&self.manifest_path(root), self).await
    }

    pub async fn load(root: &Path, manifest_id: &Fingerprint) -> Result<Self> {
        let path = root
            .join("chunks")
            .join(manifest_id.as_str())
            .join("manifest.json");
        let manifest: Self = read_json_bounded(&path, DEFAULT_MANIFEST_LIMIT).await?;
        if &manifest.manifest_id != manifest_id {
            return Err(anyhow!("chunk manifest path does not match its identity"));
        }
        manifest.validate_identity()?;
        Ok(manifest)
    }

    pub async fn publish_chunk(
        &mut self,
        root: &Path,
        index: u32,
        staging_path: &Path,
        produced_parameters: &MediaParameters,
        pool: &CpuPool,
        cancellation: CancellationToken,
    ) -> Result<ArtifactFile> {
        self.validate_identity()?;
        if produced_parameters != &self.parameters {
            return Err(anyhow!("chunk parameters are incompatible with manifest"));
        }
        let identity = fingerprint_file(pool, staging_path.to_path_buf(), cancellation).await?;
        let relative = self.expected_chunk_path(index)?;
        let absolute = root.join(&relative);
        let parent = absolute
            .parent()
            .ok_or_else(|| anyhow!("chunk path needs a parent"))?;
        tokio::fs::create_dir_all(parent).await?;
        tokio::fs::rename(staging_path, &absolute).await?;
        let file = ArtifactFile {
            path: path_token(&relative)?,
            size: identity.size,
            sha256: identity.sha256,
        };
        self.entry_mut(index)?.file = Some(file.clone());
        if let Err(error) = self.save(root).await {
            self.entry_mut(index)?.file = None;
            let _ = tokio::fs::remove_file(&absolute).await;
            return Err(error);
        }
        Ok(file)
    }

    /// Clear missing/corrupt entries so a retry schedules only unfinished work.
    pub async fn reconcile(
        &mut self,
        root: &Path,
        pool: &CpuPool,
        cancellation: CancellationToken,
    ) -> Result<Vec<u32>> {
        self.validate_identity()?;
        let mut missing = Vec::new();
        let mut corrupt = Vec::new();
        for entry in &self.chunks {
            if cancellation.is_cancelled() {
                return Err(anyhow!("chunk reconciliation cancelled"));
            }
            let ready = match &entry.file {
                Some(file) => match file.verify(root, pool, cancellation.child_token()).await {
                    Ok(_) => true,
                    Err(error) if crate::artifacts::is_cpu_execution_error(&error) => {
                        return Err(error);
                    }
                    Err(_) => {
                        corrupt.push(entry.spec.index);
                        false
                    }
                },
                None => false,
            };
            if !ready {
                missing.push(entry.spec.index);
            }
        }
        if !corrupt.is_empty() {
            for index in corrupt {
                self.entry_mut(index)?.file = None;
            }
            self.save(root).await?;
        }
        Ok(missing)
    }

    pub async fn verified_stitch_inputs(
        &self,
        root: &Path,
        expected_parameters: &MediaParameters,
        pool: &CpuPool,
        cancellation: CancellationToken,
    ) -> Result<Vec<PathBuf>> {
        self.validate_identity()?;
        if expected_parameters != &self.parameters {
            return Err(anyhow!("mux parameters do not match chunk manifest"));
        }
        let mut paths = Vec::with_capacity(self.chunks.len());
        for entry in &self.chunks {
            let file = entry
                .file
                .as_ref()
                .ok_or_else(|| anyhow!("chunk {} is not ready", entry.spec.index))?;
            paths.push(file.verify(root, pool, cancellation.child_token()).await?);
        }
        Ok(paths)
    }

    fn validate_identity(&self) -> Result<()> {
        if self.schema_version != CHUNK_SCHEMA_VERSION {
            return Err(anyhow!("unsupported chunk manifest schema"));
        }
        self.parameters.validate()?;
        if self.chunks.is_empty() {
            return Err(anyhow!("chunk manifest is empty"));
        }
        for (position, entry) in self.chunks.iter().enumerate() {
            if usize::try_from(entry.spec.index).ok() != Some(position)
                || entry.spec.frames.start >= entry.spec.frames.end
                || (position > 0
                    && self.chunks[position - 1].spec.frames.end != entry.spec.frames.start)
            {
                return Err(anyhow!("chunk ranges are not contiguous"));
            }
            let expected_key = chunk_identity(
                entry.spec.index,
                entry.spec.frames,
                &self.plan_fingerprint,
                &self.source_fingerprint,
                &self.output_fingerprint,
                &self.parameters,
            );
            if entry.spec.key != expected_key {
                return Err(anyhow!("chunk key does not match its inputs"));
            }
            if let Some(file) = &entry.file {
                let expected_path = self
                    .directory()
                    .join(format!("{:06}-{}.chunk", entry.spec.index, entry.spec.key));
                if file.path != path_token(&expected_path)? {
                    return Err(anyhow!("chunk file path does not match its key"));
                }
            }
        }
        let expected = manifest_identity(
            &self.plan_fingerprint,
            &self.source_fingerprint,
            &self.output_fingerprint,
            &self.parameters,
            &self.chunks,
        );
        if expected != self.manifest_id {
            return Err(anyhow!("chunk manifest identity mismatch"));
        }
        Ok(())
    }

    fn entry(&self, index: u32) -> Result<&ChunkEntry> {
        self.chunks
            .get(index as usize)
            .filter(|entry| entry.spec.index == index)
            .ok_or_else(|| anyhow!("unknown chunk index {index}"))
    }

    fn entry_mut(&mut self, index: u32) -> Result<&mut ChunkEntry> {
        self.chunks
            .get_mut(index as usize)
            .filter(|entry| entry.spec.index == index)
            .ok_or_else(|| anyhow!("unknown chunk index {index}"))
    }
}

fn chunk_identity(
    index: u32,
    frames: FrameRange,
    plan: &Fingerprint,
    source: &Fingerprint,
    output: &Fingerprint,
    parameters: &MediaParameters,
) -> Fingerprint {
    let index_bytes = index.to_be_bytes();
    let start_bytes = frames.start.to_be_bytes();
    let end_bytes = frames.end.to_be_bytes();
    Fingerprint::combine([
        b"encode-chunk-v1".as_slice(),
        index_bytes.as_slice(),
        start_bytes.as_slice(),
        end_bytes.as_slice(),
        plan.as_str().as_bytes(),
        source.as_str().as_bytes(),
        output.as_str().as_bytes(),
        parameters.fingerprint().as_str().as_bytes(),
    ])
}

fn manifest_identity(
    plan: &Fingerprint,
    source: &Fingerprint,
    output: &Fingerprint,
    parameters: &MediaParameters,
    chunks: &[ChunkEntry],
) -> Fingerprint {
    let ranges: Vec<_> = chunks
        .iter()
        .map(|entry| {
            (
                entry.spec.index,
                entry.spec.frames.start,
                entry.spec.frames.end,
                entry.spec.key.as_str(),
            )
        })
        .collect();
    let ranges = serde_json::to_vec(&ranges).expect("chunk range serialization cannot fail");
    Fingerprint::combine([
        b"chunk-manifest-v1".as_slice(),
        plan.as_str().as_bytes(),
        source.as_str().as_bytes(),
        output.as_str().as_bytes(),
        parameters.fingerprint().as_str().as_bytes(),
        ranges.as_slice(),
    ])
}

#[cfg(test)]
mod tests {
    use crate::runtime::cpu_pool::CpuPoolConfig;

    use super::*;

    fn parameters() -> MediaParameters {
        MediaParameters {
            container: "mp4".into(),
            video_codec: "av1".into(),
            pixel_format: "yuv420p".into(),
            width: 1920,
            height: 1080,
            time_base_numerator: 1,
            time_base_denominator: 30,
            audio_codec: Some("aac".into()),
            sample_rate: Some(48_000),
            channels: Some(2),
        }
    }

    fn plan() -> ChunkManifest {
        ChunkManifest::plan(
            Fingerprint::digest(b"plan"),
            Fingerprint::digest(b"source"),
            Fingerprint::digest(b"output"),
            parameters(),
            300,
            100,
            &[
                SceneBoundary {
                    frame: 90,
                    confirmed: true,
                },
                SceneBoundary {
                    frame: 180,
                    confirmed: true,
                },
                SceneBoundary {
                    frame: 50,
                    confirmed: false,
                },
            ],
        )
        .unwrap()
    }

    fn pool() -> CpuPool {
        CpuPool::new(CpuPoolConfig {
            threads: 1,
            queue_capacity: 2,
        })
        .unwrap()
    }

    #[test]
    fn confirmed_scenes_are_preferred_without_exceeding_chunk_limit() {
        let manifest = plan();
        let ranges: Vec<_> = manifest
            .chunks
            .iter()
            .map(|chunk| chunk.spec.frames)
            .collect();
        assert_eq!(
            ranges,
            vec![
                FrameRange { start: 0, end: 90 },
                FrameRange {
                    start: 90,
                    end: 180
                },
                FrameRange {
                    start: 180,
                    end: 280
                },
                FrameRange {
                    start: 280,
                    end: 300
                }
            ]
        );
    }

    #[tokio::test]
    async fn resume_returns_only_missing_or_corrupt_chunks() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("staging");
        tokio::fs::write(&staging, b"chunk-zero").await.unwrap();
        let mut manifest = plan();
        manifest
            .publish_chunk(
                root.path(),
                0,
                &staging,
                &parameters(),
                &pool(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            manifest
                .reconcile(root.path(), &pool(), CancellationToken::new())
                .await
                .unwrap(),
            vec![1, 2, 3]
        );
        let chunk = root.path().join(manifest.expected_chunk_path(0).unwrap());
        tokio::fs::write(chunk, b"corrupt").await.unwrap();
        assert_eq!(
            manifest
                .reconcile(root.path(), &pool(), CancellationToken::new())
                .await
                .unwrap(),
            vec![0, 1, 2, 3]
        );
    }

    #[tokio::test]
    async fn incompatible_chunk_and_incomplete_manifest_cannot_be_muxed() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("staging");
        tokio::fs::write(&staging, b"chunk-zero").await.unwrap();
        let mut manifest = plan();
        let mut incompatible = parameters();
        incompatible.width = 1280;
        assert!(manifest
            .publish_chunk(
                root.path(),
                0,
                &staging,
                &incompatible,
                &pool(),
                CancellationToken::new(),
            )
            .await
            .is_err());
        assert!(manifest
            .verified_stitch_inputs(
                root.path(),
                &parameters(),
                &pool(),
                CancellationToken::new(),
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn manifest_round_trip_keeps_identity() {
        let root = tempfile::tempdir().unwrap();
        let manifest = plan();
        manifest.save(root.path()).await.unwrap();
        let loaded = ChunkManifest::load(root.path(), &manifest.manifest_id)
            .await
            .unwrap();
        assert_eq!(loaded, manifest);
    }

    #[tokio::test]
    async fn manifest_rejects_chunk_path_outside_its_key() {
        let root = tempfile::tempdir().unwrap();
        let mut manifest = plan();
        manifest.chunks[0].file = Some(ArtifactFile {
            path: "sources/other.mp4".into(),
            size: 1,
            sha256: Fingerprint::digest(b"x"),
        });
        assert!(manifest.save(root.path()).await.is_err());
        assert!(manifest
            .reconcile(root.path(), &pool(), CancellationToken::new())
            .await
            .is_err());
        assert!(manifest
            .verified_stitch_inputs(
                root.path(),
                &parameters(),
                &pool(),
                CancellationToken::new(),
            )
            .await
            .is_err());
    }

    #[test]
    fn audio_metadata_cannot_exist_without_a_codec() {
        let mut value = parameters();
        value.audio_codec = None;
        assert!(value.validate().is_err());
        value.sample_rate = None;
        value.channels = None;
        assert!(value.validate().is_ok());
    }
}
