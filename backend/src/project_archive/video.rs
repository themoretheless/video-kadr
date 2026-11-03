//! Version-two cache records, separate from the media/library namespace.
use super::*;
const CACHE_MAGIC: &[u8; 4] = b"VMC1";
pub(super) const CACHE_HEADER: u64 = 46;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchivedVideoCache {
    pub key: String,
    pub source_id: String,
    pub identity: crate::services::concat_video_analysis::portable::Identity,
    pub byte_length: u64,
    pub sha256: String,
}
#[derive(Debug, Clone)]
pub struct ArchiveVideoCacheInput {
    pub key: String,
    pub path: PathBuf,
}
#[derive(Debug)]
pub struct StagedArchiveVideoCache {
    key: String,
    path: PathBuf,
}
impl StagedArchiveVideoCache {
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for StagedArchiveVideoCache {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(super) fn validate(manifest: &ProjectArchiveManifest) -> Result<(), ArchiveError> {
    if manifest.video_caches.len() > MAX_COMPOSITION_PROJECT_SOURCES {
        return Err(ArchiveError::Limit("too many video cache entries"));
    }
    let mut keys = HashSet::new();
    let mut tuples = HashSet::new();
    let clips: Vec<_> = if let Some(nodes) = manifest
        .project
        .document
        .get("compositions")
        .and_then(Value::as_object)
    {
        nodes.values().filter_map(|n| n.get("content")).collect()
    } else {
        vec![&manifest.project.document]
    };
    let bindings: Vec<_> = clips
        .iter()
        .flat_map(|content| {
            content
                .get("tracks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter(|track| track.get("kind").and_then(Value::as_str) == Some("video"))
        .flat_map(|track| {
            track
                .get("clips")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|clip| {
            clip.get("concatCutout")
                .filter(|b| !b.is_null())
                .map(|binding| (clip, binding))
        })
        .collect();
    for cache in &manifest.video_caches {
        if !valid_composition_source_id(&cache.key)
            || !keys.insert(&cache.key)
            || !valid_composition_source_id(&cache.source_id)
        {
            return Err(ArchiveError::Invalid(
                "invalid or duplicate video cache key/source",
            ));
        }
        if cache.byte_length == 0 || cache.byte_length > MAX_ARCHIVE_ENTRY_BYTES {
            return Err(ArchiveError::Limit("video cache exceeds entry limit"));
        }
        parse_digest(&cache.sha256)?;
        let identity = &cache.identity;
        for hash in [
            &identity.video_hash,
            &identity.model_hash,
            &identity.runtime_hash,
            &identity.cache_identity,
        ] {
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(ArchiveError::Invalid("invalid video cache identity"));
            }
        }
        crate::services::concat_video_analysis::RunCache::new_for_model(
            identity.video_hash.clone(),
            identity.runtime_hash.clone(),
            identity.model_hash.clone(),
        )
        .map_err(|_| ArchiveError::Invalid("unsupported video cache model"))?;
        if !tuples.insert((
            &cache.source_id,
            &identity.video_hash,
            &identity.model_hash,
            &identity.runtime_hash,
        )) {
            return Err(ArchiveError::Invalid("duplicate video cache binding"));
        }
        let physical = crate::handlers::concat_cutout_access::library_id(
            &manifest.project.document,
            &cache.source_id,
        )
        .map_err(|_| ArchiveError::Invalid("invalid video cache library binding"))?;
        if !manifest.sources.iter().any(|s| {
            s.source_id == physical && s.media_type == "video" && s.sha256 == identity.video_hash
        }) {
            return Err(ArchiveError::Invalid(
                "video cache does not match archived source",
            ));
        }
        if !bindings
            .iter()
            .any(|(clip, binding)| matches_binding(clip, binding, cache))
        {
            return Err(ArchiveError::Invalid("unused video cache entry"));
        }
    }
    if manifest.schema_version == ARCHIVE_VIDEO_FORMAT_VERSION {
        for (clip, binding) in bindings {
            if !manifest
                .video_caches
                .iter()
                .any(|cache| matches_binding(clip, binding, cache))
            {
                return Err(ArchiveError::Invalid(
                    "archive is missing a bound video cache",
                ));
            }
        }
    }
    Ok(())
}
fn matches_binding(clip: &Value, binding: &Value, cache: &ArchivedVideoCache) -> bool {
    clip.get("sourceId").and_then(Value::as_str) == Some(cache.source_id.as_str())
        && binding.get("sourceHash").and_then(Value::as_str)
            == Some(cache.identity.video_hash.as_str())
        && binding.get("modelHash").and_then(Value::as_str)
            == Some(cache.identity.model_hash.as_str())
        && binding.get("runtimeHash").and_then(Value::as_str)
            == Some(cache.identity.runtime_hash.as_str())
}
pub(super) async fn write<W: AsyncWrite + Unpin>(
    writer: &mut W,
    caches: &[ArchivedVideoCache],
    inputs: &[ArchiveVideoCacheInput],
) -> Result<(), ArchiveError> {
    if caches.len() != inputs.len() {
        return Err(ArchiveError::Invalid("video cache payload count mismatch"));
    }
    for (cache, input) in caches.iter().zip(inputs) {
        if cache.key != input.key {
            return Err(ArchiveError::Invalid("video cache input order mismatch"));
        }
        let digest = parse_digest(&cache.sha256)?;
        writer.write_all(CACHE_MAGIC).await?;
        writer
            .write_all(&(cache.key.len() as u16).to_be_bytes())
            .await?;
        writer.write_all(&cache.byte_length.to_be_bytes()).await?;
        writer.write_all(&digest).await?;
        writer.write_all(cache.key.as_bytes()).await?;
        let mut file = tokio::fs::File::open(&input.path).await?;
        copy_exact_and_verify(&mut file, writer, cache.byte_length, &digest).await?;
        let mut extra = [0];
        if file.read(&mut extra).await? != 0 {
            return Err(ArchiveError::Invalid(
                "video cache changed during archive write",
            ));
        }
    }
    Ok(())
}
pub(super) async fn read<R: AsyncRead + Unpin>(
    reader: &mut R,
    caches: &[ArchivedVideoCache],
    root: &Path,
) -> Result<Vec<StagedArchiveVideoCache>, ArchiveError> {
    let mut result = vec![];
    for cache in caches {
        let mut magic = [0; 4];
        read_exact_archive(reader, &mut magic).await?;
        let length = read_u16(reader).await? as usize;
        let bytes = read_u64(reader).await?;
        let mut digest = [0; 32];
        read_exact_archive(reader, &mut digest).await?;
        if &magic != CACHE_MAGIC
            || length != cache.key.len()
            || bytes != cache.byte_length
            || digest != parse_digest(&cache.sha256)?
        {
            return Err(ArchiveError::Invalid("video cache entry header mismatch"));
        }
        let key = read_utf8(reader, length).await?;
        if key != cache.key {
            return Err(ArchiveError::Invalid("video cache entry key mismatch"));
        }
        let staged = StagedArchiveVideoCache {
            key,
            path: root.join(format!("{}.veproj-video-cache", Uuid::new_v4())),
        };
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let mut file = options.open(&staged.path).await?;
        let mut hasher = Sha256::new();
        copy_exact_and_hash(reader, &mut file, bytes, &mut hasher).await?;
        file.flush().await?;
        drop(file);
        if <[u8; 32]>::from(hasher.finalize()) != digest {
            return Err(ArchiveError::Invalid("video cache checksum mismatch"));
        }
        result.push(staged);
    }
    Ok(result)
}
