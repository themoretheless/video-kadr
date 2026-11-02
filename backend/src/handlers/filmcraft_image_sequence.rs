//! Authorized library adapter for Filmcraft numbered image sequences.
use crate::{
    domain::{
        filmcraft_image_sequence::{self, Numbered},
        group_surface::GroupSurface,
    },
    error::{AppError, AppResult},
    library::{Library, MediaEntry},
    runtime::cpu_pool::CpuTaskError,
    services::filmcraft_video_preview::SourceVideoRevision,
    state::AppState,
};
use axum::http::HeaderMap;
use filmcraft_time::FrameRate;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
    sync::Arc,
};
use tokio_util::sync::CancellationToken;

pub(super) struct Pair {
    pub a: Arc<GroupSurface>,
    pub b: Arc<GroupSurface>,
    used: Vec<(String, PathBuf, SourceVideoRevision)>,
    space: Option<String>,
    missing: BTreeSet<String>,
}
async fn access(
    state: &AppState,
    headers: &HeaderMap,
    owner: Option<&str>,
    id: &str,
) -> AppResult<()> {
    if let Some(owner) = owner {
        super::stabilization_jobs::authorize_source_owner(state, id, owner).await
    } else {
        super::library::authorize_source_access(state, id, headers).await
    }
}
async fn verify_reads(
    state: &AppState,
    headers: &HeaderMap,
    owner: Option<&str>,
    space: &Option<String>,
    used: &[(String, PathBuf, SourceVideoRevision)],
    missing: &BTreeSet<String>,
) -> AppResult<()> {
    for (id, path, revision) in used {
        access(state, headers, owner, id).await?;
        if state
            .db
            .source_space_id(id)
            .await
            .map_err(|e| AppError::internal("verify sequence scope", e))?
            != *space
        {
            return Err(AppError::bad_request("sequence source scope changed"));
        }
        if SourceVideoRevision::read(path)
            .await
            .map_err(|e| AppError::bad_request(e.to_string()))?
            != *revision
        {
            return Err(AppError::bad_request(
                "sequence source changed before publication",
            ));
        }
    }
    for id in missing {
        access(state, headers, owner, id).await?;
        let entry = state
            .library
            .get(id)
            .await
            .ok_or_else(|| AppError::bad_request("sequence binding disappeared"))?;
        if state.library.resolve_media_path(&entry).await.is_ok() {
            return Err(AppError::bad_request(
                "sequence source availability changed",
            ));
        }
    }
    Ok(())
}
impl Pair {
    pub async fn verify(&self, state: &AppState, headers: &HeaderMap) -> AppResult<()> {
        verify_reads(state, headers, None, &self.space, &self.used, &self.missing).await
    }
}

pub(super) struct Reader<'a> {
    state: &'a AppState,
    headers: &'a HeaderMap,
    owner: Option<&'a str>,
    space: Option<String>,
    baseline: Option<BTreeMap<String, Option<Vec<u8>>>>,
    missing: BTreeSet<String>,
    cancel: CancellationToken,
    frames: Vec<Option<String>>,
    entries: BTreeMap<String, MediaEntry>,
    cache: VecDeque<(usize, Arc<GroupSurface>)>,
    used: Vec<(String, PathBuf, SourceVideoRevision)>,
}
impl Reader<'_> {
    async fn decode(&mut self, index: usize) -> AppResult<Option<Arc<GroupSurface>>> {
        let name = self.frames[index]
            .as_ref()
            .ok_or_else(|| AppError::bad_request("missing sequence frame"))?;
        let entry = self
            .entries
            .get(name)
            .ok_or_else(|| AppError::bad_request("sequence frame binding changed"))?;
        // Recheck even cached pixels: access may have changed since discovery.
        access(self.state, self.headers, self.owner, &entry.id).await?;
        let entry = self
            .state
            .library
            .get(&entry.id)
            .await
            .ok_or_else(|| AppError::bad_request("sequence source disappeared"))?;
        if let Some(position) = self.cache.iter().position(|(i, _)| *i == index) {
            let value = self.cache.remove(position).unwrap();
            let frame = value.1.clone();
            self.cache.push_back(value);
            return Ok(Some(frame));
        }
        let Ok(path) = self.state.library.resolve_media_path(&entry).await else {
            self.missing.insert(entry.id.clone());
            return Ok(None);
        };
        let Ok(revision) = SourceVideoRevision::read(&path).await else {
            self.missing.insert(entry.id.clone());
            return Ok(None);
        };
        if let Some((_, _, old)) = self.used.iter().find(|(id, _, _)| id == &entry.id) {
            if *old != revision {
                return Err(AppError::bad_request(
                    "sequence source changed during decoding",
                ));
            }
        } else {
            self.used.push((entry.id.clone(), path.clone(), revision));
        }
        // Two decoded frames fit within 64 MiB at the decoder's 3840x2160 cap.
        while self.cache.len() >= 2 {
            self.cache.pop_front();
        }
        let result = self
            .state
            .cpu_pool
            .execute(self.cancel.clone(), move |cancel| {
                anyhow::ensure!(!cancel.is_cancelled(), "sequence decode cancelled");
                anyhow::ensure!(
                    std::fs::metadata(&path)?.len() <= 64 * 1024 * 1024,
                    "sequence image file budget exceeded"
                );
                let frame = crate::domain::group_media::decode_image_surface(&path)?;
                anyhow::ensure!(!cancel.is_cancelled(), "sequence decode cancelled");
                Ok(Arc::new(frame))
            })
            .await;
        match result {
            Ok(frame) => {
                self.cache.push_back((index, frame.clone()));
                Ok(Some(frame))
            }
            Err(CpuTaskError::Work(_)) => Ok(None),
            Err(error) => Err(AppError::service_unavailable(format!(
                "sequence decode unavailable: {error}"
            ))),
        }
    }
    pub async fn frame(&mut self, rate: FrameRate, time: i64) -> AppResult<Arc<GroupSurface>> {
        for index in filmcraft_image_sequence::candidates(&self.frames, rate, time) {
            if let Some(frame) = self.decode(index).await? {
                return Ok(frame);
            }
        }
        Err(AppError::bad_request(
            "no decodable sequence frame within eight candidates",
        ))
    }
}

impl Reader<'_> {
    pub fn clear_pixels(&mut self) {
        self.cache.clear();
    }
    pub async fn verify(&self) -> AppResult<()> {
        verify_reads(
            self.state,
            self.headers,
            self.owner,
            &self.space,
            &self.used,
            &self.missing,
        )
        .await
    }
    /// A complete source revision admits reusable analysis/checkpoints. Missing
    /// local assets are read lazily, but cannot attest a durable cache revision.
    pub async fn revision(&mut self) -> AppResult<Option<[u8; 32]>> {
        let (hash, complete) = self.snapshot().await?;
        Ok(complete.then_some(hash))
    }
    pub fn member_ids(&self) -> impl Iterator<Item = &str> {
        self.entries.values().map(|entry| entry.id.as_str())
    }
    /// Includes unavailable members explicitly, for durable queue snapshots.
    pub async fn snapshot(&mut self) -> AppResult<([u8; 32], bool)> {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        let mut current = BTreeMap::new();
        let mut complete = true;
        for (name, old) in &self.entries {
            access(self.state, self.headers, self.owner, &old.id).await?;
            let entry = self
                .state
                .library
                .get(&old.id)
                .await
                .ok_or_else(|| AppError::bad_request("sequence binding disappeared"))?;
            if logical_name(&entry) != *name
                || Library::storage_relative_path(&entry).ok()
                    != Library::storage_relative_path(old).ok()
                || self
                    .state
                    .db
                    .source_space_id(&entry.id)
                    .await
                    .map_err(|e| AppError::internal("verify sequence scope", e))?
                    != self.space
            {
                return Err(AppError::bad_request("sequence catalog changed"));
            }
            let relative = Library::storage_relative_path(&entry)
                .map_err(|e| AppError::bad_request(e.to_string()))?;
            let path = self.state.sources_dir().join(&relative);
            let revision = if tokio::fs::symlink_metadata(&path)
                .await
                .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
            {
                SourceVideoRevision::read(&path)
                    .await
                    .ok()
                    .map(|v| v.fingerprint())
            } else {
                None
            };
            complete &= revision.is_some();
            for value in [
                name.as_bytes(),
                entry.id.as_bytes(),
                relative.as_os_str().as_encoded_bytes(),
            ] {
                hash.update((value.len() as u64).to_le_bytes());
                hash.update(value);
            }
            hash.update([u8::from(revision.is_some())]);
            if let Some(v) = &revision {
                hash.update(v);
            }
            current.insert(entry.id.clone(), revision);
        }
        if let Some(baseline) = &self.baseline {
            for (id, old) in baseline {
                if old.is_some() && current.get(id) != Some(old) {
                    return Err(AppError::bad_request(
                        "sequence source changed during preview",
                    ));
                }
            }
        } else {
            self.baseline = Some(current);
        }
        self.verify().await?;
        Ok((hash.finalize().into(), complete))
    }
}

fn logical_name(entry: &MediaEntry) -> String {
    if let Some(name) = &entry.original_filename {
        return name.rsplit(['/', '\\']).next().unwrap_or(name).to_owned();
    }
    let uuid_name = std::path::Path::new(&entry.filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok());
    if !uuid_name && Numbered::parse(&entry.filename).is_some() {
        return entry.filename.clone();
    }
    // Older uploads stored only the original stem as the initial library title.
    if let Some(title) = entry.title.as_ref().filter(|s| !s.contains(['/', '\\'])) {
        if Numbered::parse(title).is_some() {
            return title.clone();
        }
        if let Some(extension) = std::path::Path::new(&entry.filename)
            .extension()
            .and_then(|s| s.to_str())
        {
            let name = format!("{title}.{extension}");
            if Numbered::parse(&name).is_some() {
                return name;
            }
        }
    }
    entry.filename.clone()
}

pub(super) async fn catalog<'a>(
    state: &'a AppState,
    headers: &'a HeaderMap,
    owner: Option<&'a str>,
    first: &MediaEntry,
    cancel: CancellationToken,
) -> AppResult<Reader<'a>> {
    let first_name = logical_name(first);
    let pattern = Numbered::parse(&first_name)
        .ok_or_else(|| AppError::bad_request("sequence source must be a numbered still image"))?;
    let scope =
        Library::storage_relative_path(first).map_err(|e| AppError::bad_request(e.to_string()))?;
    // The same accessibility view as the library, across accessible spaces.
    // Physical storage scope still keeps unrelated folders/sequences separate.
    let mut discovery_headers = headers.clone();
    discovery_headers.remove("x-space-id");
    let visible = if let Some(owner) = owner {
        super::library::library_visibility_for_owner(state, owner).await?
    } else {
        super::library::library_visibility(state, &discovery_headers).await?
    };
    let first_space = state
        .db
        .source_space_id(&first.id)
        .await
        .map_err(|e| AppError::internal("resolve sequence source space", e))?;
    let mut entries = BTreeMap::new();
    for entry in state.library.metadata_entries().await {
        if entry.kind != "source" || !visible.allows(&entry.id, &entry.kind) {
            continue;
        }
        let name = logical_name(&entry);
        let Some(number) = pattern.number_of(&name) else {
            continue;
        };
        if number < pattern.number || (name == first_name && entry.id != first.id) {
            continue;
        }
        let space = state
            .db
            .source_space_id(&entry.id)
            .await
            .map_err(|e| AppError::internal("resolve sequence frame space", e))?;
        if space != first_space {
            continue;
        }
        // Authorization can migrate a legacy Space entry to its isolated folder.
        // Resolve the fresh catalog entry afterwards, never a stale flat path.
        access(state, headers, owner, &entry.id).await?;
        let Some(entry) = state.library.get(&entry.id).await else {
            continue;
        };
        let Ok(relative) = Library::storage_relative_path(&entry) else {
            continue;
        };
        if relative.parent() != scope.parent() {
            continue;
        }
        if entries.insert(name, entry).is_some() {
            return Err(AppError::bad_request(
                "ambiguous image sequence names in library folder",
            ));
        }
    }
    // The explicitly selected first source was authorized by the caller. It is
    // checked again on decoding, and must decode before fallback is enabled.
    entries.insert(first_name.clone(), first.clone());
    let frames =
        filmcraft_image_sequence::frames(&first_name, &entries.keys().cloned().collect::<Vec<_>>())
            .map_err(|e| AppError::bad_request(e.to_string()))?;
    let reader = Reader {
        state,
        headers,
        owner,
        space: first_space,
        baseline: None,
        missing: BTreeSet::new(),
        cancel,
        frames,
        entries,
        cache: VecDeque::new(),
        used: vec![],
    };
    Ok(reader)
}

pub(super) async fn open<'a>(
    state: &'a AppState,
    headers: &'a HeaderMap,
    owner: Option<&'a str>,
    first: &MediaEntry,
    cancel: CancellationToken,
) -> AppResult<Reader<'a>> {
    let mut reader = catalog(state, headers, owner, first, cancel).await?;
    reader
        .decode(0)
        .await?
        .ok_or_else(|| AppError::bad_request("sequence first image cannot be decoded"))?;
    Ok(reader)
}

pub(super) async fn pair(
    state: &AppState,
    headers: &HeaderMap,
    first: &MediaEntry,
    rate: FrameRate,
    from: i64,
    to: i64,
    cancel: CancellationToken,
) -> AppResult<Pair> {
    let mut reader = open(state, headers, None, first, cancel).await?;
    let a = reader.frame(rate, from).await?;
    let b = reader.frame(rate, to).await?;
    Ok(Pair {
        a,
        b,
        used: reader.used,
        space: reader.space,
        missing: reader.missing,
    })
}
