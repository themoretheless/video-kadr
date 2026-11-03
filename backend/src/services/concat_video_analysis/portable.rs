//! Bounded standalone transport for verified video stores. No paths or ACL grants.
use super::{CacheManifest, PublishedCache, RunCache, Stage, StoredRun, MAX_CACHE_MANIFEST};
use anyhow::{ensure, Result};
use std::path::Path;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

const MAGIC: &[u8; 8] = b"VKMASK01";
const HEADER: u64 = 24;
const MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Identity {
    pub video_hash: String,
    pub model_hash: String,
    pub runtime_hash: String,
    pub cache_identity: String,
}

/// Imported bytes stay private and ephemeral until explicitly published.
/// The cache closes its file handles before the staging directory is removed.
pub struct Restored {
    cache: RunCache,
    _stage: Stage,
}
impl Restored {
    pub fn cache(&mut self) -> &mut RunCache {
        &mut self.cache
    }
    pub async fn publish(
        &mut self,
        root: &Path,
        budget: u64,
        cancel: &CancellationToken,
    ) -> Result<PublishedCache> {
        self.cache.publish(root, budget, cancel).await
    }
}
impl RunCache {
    pub fn portable_identity(&self) -> Result<Identity> {
        Ok(Identity {
            video_hash: self.video_hash.clone(),
            model_hash: self.model_hash.clone(),
            runtime_hash: self.runtime_hash.clone(),
            cache_identity: self.identity()?,
        })
    }
    /// Flatten every verified parent generation without exposing its directory.
    pub async fn write_portable<W: AsyncWrite + Unpin>(
        &mut self,
        mut writer: W,
        budget: u64,
        cancel: &CancellationToken,
    ) -> Result<()> {
        ensure!(
            budget > 0 && budget <= MAX_BYTES && !self.is_empty() && !cancel.is_cancelled(),
            "invalid portable cache export"
        );
        let manifest = CacheManifest {
            schema_version: 1,
            video_hash: self.video_hash.clone(),
            model_hash: self.model_hash.clone(),
            runtime_hash: self.runtime_hash.clone(),
            parent: None,
            total_runs: None,
            runs: self
                .runs
                .iter()
                .map(|run| StoredRun {
                    start_millis: run.start_millis,
                    metadata: run.metadata.clone(),
                })
                .collect(),
        };
        let bytes = serde_json::to_vec(&manifest)?;
        ensure!(
            bytes.len() as u64 <= MAX_CACHE_MANIFEST,
            "portable cache manifest too large"
        );
        let masks = manifest.runs.iter().try_fold(0u64, |total, run| {
            total
                .checked_add(run.metadata.mask_bytes)
                .ok_or_else(|| anyhow::anyhow!("portable cache size overflow"))
        })?;
        ensure!(
            HEADER
                .checked_add(bytes.len() as u64)
                .and_then(|n| n.checked_add(masks))
                .is_some_and(|n| n <= budget),
            "portable cache export budget exceeded"
        );
        writer.write_all(MAGIC).await?;
        writer.write_u64_le(bytes.len() as u64).await?;
        writer.write_u64_le(masks).await?;
        writer.write_all(&bytes).await?;
        for run in &mut self.runs {
            for frame in 0..run.metadata.frames {
                ensure!(!cancel.is_cancelled(), "portable cache export cancelled");
                let mask = run.masks.read(run.start_millis + frame * 100, cancel)?;
                writer.write_all(&mask.probabilities).await?;
            }
        }
        ensure!(!cancel.is_cancelled(), "portable cache export cancelled");
        writer.flush().await?;
        Ok(())
    }
}

pub async fn restore<R: AsyncRead + Unpin>(
    mut reader: R,
    expected: &Identity,
    root: &Path,
    budget: u64,
    cancel: &CancellationToken,
) -> Result<Restored> {
    ensure!(
        root.is_absolute()
            && root.is_dir()
            && budget >= HEADER
            && budget <= MAX_BYTES
            && !cancel.is_cancelled(),
        "invalid portable cache import"
    );
    RunCache::new_for_model(
        expected.video_hash.clone(),
        expected.runtime_hash.clone(),
        expected.model_hash.clone(),
    )?;
    ensure!(
        expected.cache_identity.len() == 64
            && expected
                .cache_identity
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid portable cache identity"
    );
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic).await?;
    ensure!(&magic == MAGIC, "unsupported portable cache format");
    let length = reader.read_u64_le().await?;
    let masks = reader.read_u64_le().await?;
    ensure!(
        length > 0
            && length <= MAX_CACHE_MANIFEST
            && HEADER
                .checked_add(length)
                .and_then(|n| n.checked_add(masks))
                .is_some_and(|n| n <= budget),
        "portable cache import budget exceeded"
    );
    let mut bytes = vec![0; usize::try_from(length)?];
    reader.read_exact(&mut bytes).await?;
    let manifest: CacheManifest = serde_json::from_slice(&bytes)?;
    ensure!(
        manifest.schema_version == 1
            && manifest.parent.is_none()
            && manifest.total_runs.is_none()
            && !manifest.runs.is_empty()
            && manifest.runs.len() <= 360000,
        "portable cache must be standalone"
    );
    ensure!(
        manifest.video_hash == expected.video_hash
            && manifest.model_hash == expected.model_hash
            && manifest.runtime_hash == expected.runtime_hash,
        "portable cache provenance mismatch"
    );
    let (total, frames) =
        manifest
            .runs
            .iter()
            .try_fold((0u64, 0u64), |(total, frames), run| -> Result<_> {
                Ok((
                    total
                        .checked_add(run.metadata.mask_bytes)
                        .ok_or_else(|| anyhow::anyhow!("portable cache size overflow"))?,
                    frames
                        .checked_add(run.metadata.frames)
                        .ok_or_else(|| anyhow::anyhow!("portable cache frame overflow"))?,
                ))
            })?;
    ensure!(
        total == masks && frames > 0 && frames <= 360000,
        "portable cache coverage/length mismatch"
    );
    let stage = Stage(root.join(format!("concat-video-cache-{}", uuid::Uuid::new_v4())));
    tokio::fs::create_dir(&stage.0).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&stage.0, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let mut buffer = vec![0; 64 * 1024];
    for (index, run) in manifest.runs.iter().enumerate() {
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let mut file = options.open(stage.0.join(format!("{index}.masks"))).await?;
        let mut remaining = run.metadata.mask_bytes;
        while remaining > 0 {
            ensure!(!cancel.is_cancelled(), "portable cache import cancelled");
            let count = usize::try_from(remaining.min(buffer.len() as u64))?;
            reader.read_exact(&mut buffer[..count]).await?;
            file.write_all(&buffer[..count]).await?;
            remaining -= count as u64;
        }
        file.sync_all().await?;
    }
    let mut extra = [0u8; 1];
    ensure!(
        reader.read(&mut extra).await? == 0,
        "portable cache trailing data"
    );
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    let mut file = options.open(stage.0.join("manifest.json")).await?;
    file.write_all(&bytes).await?;
    file.sync_all().await?;
    drop(file);
    use sha2::{Digest, Sha256};
    let receipt = PublishedCache {
        directory: stage.0.clone(),
        manifest_hash: format!("{:x}", Sha256::digest(&bytes)),
        video_hash: expected.video_hash.clone(),
        model_hash: expected.model_hash.clone(),
        runtime_hash: expected.runtime_hash.clone(),
    };
    let cache = RunCache::restore(&receipt, budget - HEADER, cancel).await?;
    ensure!(
        cache.identity()? == expected.cache_identity && !cancel.is_cancelled(),
        "portable cache identity mismatch/cancelled"
    );
    Ok(Restored {
        cache,
        _stage: stage,
    })
}
