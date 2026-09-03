use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::object_storage::SourceObjectStore;

/// One persisted media item: an imported/uploaded source or a rendered output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaEntry {
    pub id: String,
    /// "source" or "output".
    pub kind: String,
    pub filename: String,
    /// Relative physical key below `sources/` or `outputs/`. Legacy entries
    /// omit it and continue to resolve directly by filename.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_key: Option<String>,
    pub url: String,
    /// Probed source category (`video`, `audio`, or `image`). Legacy library
    /// rows omit it and remain readable; outputs intentionally leave it empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Probed frame rate and stream codecs. Additive optional fields keep old
    /// `library.json` files readable while allowing relink/proxy UIs to avoid
    /// guessing whether a video has an audio stream.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vcodec: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acodec: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    pub created_at: u64,
}

impl MediaEntry {
    /// Build an entry from a result JSON blob (the shape import/edit/upload return).
    pub fn from_result(kind: &str, v: &Value) -> Self {
        MediaEntry {
            id: v["id"].as_str().unwrap_or_default().to_string(),
            kind: kind.to_string(),
            filename: v["filename"].as_str().unwrap_or_default().to_string(),
            storage_key: v["storageKey"].as_str().map(str::to_owned),
            url: v["url"].as_str().unwrap_or_default().to_string(),
            media_type: v["mediaType"].as_str().map(str::to_owned),
            title: v["title"].as_str().map(|s| s.to_string()),
            duration: v["duration"].as_f64(),
            width: v["width"].as_u64().map(|n| n as u32),
            height: v["height"].as_u64().map(|n| n as u32),
            fps: v["fps"].as_f64(),
            vcodec: v["vcodec"].as_str().map(str::to_owned),
            acodec: v["acodec"].as_str().map(str::to_owned),
            size_bytes: v["sizeBytes"].as_u64(),
            created_at: now_secs(),
        }
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// JSON-file-backed media library that survives restarts. Small enough that we
/// just keep the whole list in memory and rewrite the file on each change.
#[derive(Clone)]
pub struct Library {
    entries: Arc<Mutex<Vec<MediaEntry>>>,
    path: PathBuf,
    storage: PathBuf,
    object_storage: Option<SourceObjectStore>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SourceBackupSyncReport {
    pub uploaded: usize,
    pub unchanged: usize,
    pub failed: usize,
}

impl Library {
    /// Load the library from `storage/library.json` (empty if missing/corrupt).
    pub async fn load(storage: PathBuf) -> Self {
        let path = storage.join("library.json");
        let entries = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice::<Vec<MediaEntry>>(&bytes).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        Library {
            entries: Arc::new(Mutex::new(entries)),
            path,
            storage,
            object_storage: None,
        }
    }

    pub fn with_object_storage(mut self, object_storage: SourceObjectStore) -> Self {
        self.object_storage = Some(object_storage);
        self
    }

    pub fn has_object_storage(&self) -> bool {
        self.object_storage.is_some()
    }

    /// Reconcile every Space-owned immutable original with external storage.
    /// Failures are isolated per source so a transient provider error never
    /// prevents the remaining library from being backed up.
    pub async fn sync_source_backups(&self) -> SourceBackupSyncReport {
        let Some(object_storage) = &self.object_storage else {
            return SourceBackupSyncReport::default();
        };
        let entries = self.entries.lock().await.clone();
        let mut report = SourceBackupSyncReport::default();
        for entry in entries.into_iter().filter(|entry| {
            entry.kind == "source"
                && entry
                    .storage_key
                    .as_deref()
                    .is_some_and(|key| key.starts_with("spaces/"))
        }) {
            let result = async {
                let path = self.resolve_media_path(&entry).await?;
                let key = Self::storage_relative_path(&entry)?;
                object_storage
                    .sync(&path, &key)
                    .await
                    .map_err(std::io::Error::other)
            }
            .await;
            match result {
                Ok(true) => report.uploaded += 1,
                Ok(false) => report.unchanged += 1,
                Err(error) => {
                    report.failed += 1;
                    tracing::warn!(source_id = %entry.id, %error, "library: source backup reconciliation failed");
                }
            }
        }
        report
    }

    /// Persist the whole list atomically (temp file + rename). Returns an error
    /// instead of swallowing it, so callers can avoid committing an in-memory
    /// change that never reached disk.
    async fn save(&self, entries: &[MediaEntry]) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(entries)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        // Write to a temp file then rename, so a crash never leaves a partial file.
        let tmp = self.path.with_extension("json.tmp");
        tokio::fs::write(&tmp, &json).await?;
        tokio::fs::rename(&tmp, &self.path).await?;
        Ok(())
    }

    pub async fn add(&self, entry: MediaEntry) -> bool {
        // Reject malformed entries: an empty id would collapse unrelated media
        // into one slot via the dedup-by-id below (and serve a broken url).
        if entry.id.is_empty()
            || entry.filename.is_empty()
            || self.unresolved_file_path(&entry).is_err()
        {
            tracing::warn!("library: skipping add of entry with empty id/filename");
            return false;
        }
        let mut guard = self.entries.lock().await;
        let mut next = guard.clone();
        next.retain(|e| e.id != entry.id);
        next.push(entry);
        // Persist first; only commit to memory if the write succeeded, so a failed
        // save never reports success while the entry is lost on restart.
        if let Err(e) = self.save(&next).await {
            tracing::error!(error = %e, "library: persist failed on add, entry not stored");
            return false;
        }
        *guard = next;
        true
    }

    /// Return entries newest-first, hiding any whose file is currently missing.
    /// Read-only: a temporarily-unavailable file is hidden, not deleted, so it
    /// reappears once present. (A previous version pruned and rewrote the store
    /// on every read, which could silently drop a still-wanted entry.)
    pub async fn list(&self) -> Vec<MediaEntry> {
        let entries = self.entries.lock().await.clone();
        let mut kept = Vec::with_capacity(entries.len());
        for e in entries {
            if self.resolve_media_path(&e).await.is_ok() {
                kept.push(e);
            }
        }
        kept.sort_by_key(|e| std::cmp::Reverse(e.created_at));
        kept
    }

    pub async fn get(&self, id: &str) -> Option<MediaEntry> {
        self.entries
            .lock()
            .await
            .iter()
            .find(|e| e.id == id)
            .cloned()
    }

    pub async fn get_by_filename(&self, filename: &str) -> Option<MediaEntry> {
        self.entries
            .lock()
            .await
            .iter()
            .find(|entry| entry.filename == filename)
            .cloned()
    }

    /// Move a legacy flat source into its Space-owned directory and persist
    /// the new key as one recoverable operation. A failed library save moves
    /// the file back before returning the error.
    pub async fn relocate_source_to_space(
        &self,
        id: &str,
        space_id: &str,
    ) -> std::io::Result<bool> {
        let space_path = Path::new(space_id);
        if !matches!(
            space_path.components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        ) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid collaboration space id",
            ));
        }
        let mut guard = self.entries.lock().await;
        let Some(index) = guard.iter().position(|entry| entry.id == id) else {
            return Ok(false);
        };
        let entry = &guard[index];
        if entry.kind != "source" {
            return Ok(false);
        }
        let storage_key = Path::new("spaces")
            .join(space_id)
            .join(&entry.filename)
            .to_string_lossy()
            .into_owned();
        if entry.storage_key.as_deref() == Some(&storage_key) {
            let entry = entry.clone();
            drop(guard);
            self.upload_source_if_configured(&entry).await?;
            return Ok(false);
        }
        if entry.storage_key.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "source is already stored in another collaboration space",
            ));
        }
        let source = self.resolve_media_path(entry).await?;
        let destination = self.storage.join("sources").join(&storage_key);
        let destination_dir = destination.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid source destination",
            )
        })?;
        tokio::fs::create_dir_all(destination_dir).await?;
        if tokio::fs::try_exists(&destination).await? {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "space source destination already exists",
            ));
        }
        tokio::fs::rename(&source, &destination).await?;
        let mut next = guard.clone();
        next[index].storage_key = Some(storage_key);
        if let Err(error) = self.save(&next).await {
            if let Err(rollback_error) = tokio::fs::rename(&destination, &source).await {
                tracing::error!(%rollback_error, "library: source relocation rollback failed");
            }
            return Err(error);
        }
        *guard = next;
        let entry = guard[index].clone();
        drop(guard);
        self.upload_source_if_configured(&entry).await?;
        Ok(true)
    }

    /// Remove an entry and delete its file. Returns true if it existed and the
    /// removal was persisted. The file is deleted only after a successful save,
    /// so a persist failure never deletes a file the stored library still lists.
    pub async fn remove(&self, id: &str) -> bool {
        let mut guard = self.entries.lock().await;
        let Some(pos) = guard.iter().position(|e| e.id == id) else {
            return false;
        };
        let entry = guard[pos].clone();
        let file_path = self.resolve_media_path(&entry).await.ok();
        let mut next = guard.clone();
        next.remove(pos);
        if let Err(e) = self.save(&next).await {
            tracing::error!(id, error = %e, "library: persist failed on remove, keeping entry");
            return false;
        }
        *guard = next;
        drop(guard);
        if let Some(path) = file_path {
            let _ = tokio::fs::remove_file(path).await;
        }
        if entry.kind == "source" {
            if let (Some(object_storage), Ok(key)) =
                (&self.object_storage, Self::storage_relative_path(&entry))
            {
                if let Err(error) = object_storage.delete(&key).await {
                    tracing::warn!(id, %error, "library: failed to delete source backup");
                }
            }
        }
        true
    }

    /// Resolve a library entry to a regular, non-symlink file inside its
    /// declared source/output directory. Persisted metadata is untrusted, so a
    /// corrupted library file cannot escape storage.
    pub async fn resolve_media_path(&self, entry: &MediaEntry) -> std::io::Result<PathBuf> {
        let candidate = self.unresolved_file_path(entry)?;
        if entry.kind == "source" && !tokio::fs::try_exists(&candidate).await? {
            if let Some(object_storage) = &self.object_storage {
                let key = Self::storage_relative_path(entry)?;
                object_storage
                    .hydrate(&candidate, &key)
                    .await
                    .map_err(std::io::Error::other)?;
            }
        }
        let metadata = tokio::fs::symlink_metadata(&candidate).await?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "library entry is not a regular file",
            ));
        }
        let base = tokio::fs::canonicalize(self.storage.join(entry.storage_subdir())).await?;
        let resolved = tokio::fs::canonicalize(candidate).await?;
        if !resolved.starts_with(&base) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "library entry escapes storage",
            ));
        }
        Ok(resolved)
    }

    async fn upload_source_if_configured(&self, entry: &MediaEntry) -> std::io::Result<()> {
        let Some(object_storage) = &self.object_storage else {
            return Ok(());
        };
        let path = self.resolve_media_path(entry).await?;
        let key = Self::storage_relative_path(entry)?;
        object_storage
            .sync(&path, &key)
            .await
            .map(|_| ())
            .map_err(std::io::Error::other)
    }

    pub fn storage_relative_path(entry: &MediaEntry) -> std::io::Result<PathBuf> {
        if !matches!(entry.kind.as_str(), "source" | "output") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid library filename or kind",
            ));
        }
        let filename = Path::new(&entry.filename);
        if !matches!(
            filename.components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        ) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid library filename or kind",
            ));
        }
        let relative = entry.storage_key.as_deref().unwrap_or(&entry.filename);
        let components = Path::new(relative).components().collect::<Vec<_>>();
        let valid = if entry.storage_key.is_none() {
            matches!(components.as_slice(), [Component::Normal(_)])
        } else {
            entry.kind == "source"
                && matches!(
                    components.as_slice(),
                    [Component::Normal(prefix), Component::Normal(space), Component::Normal(file)]
                        if *prefix == "spaces" && !space.is_empty() && *file == std::ffi::OsStr::new(&entry.filename)
                )
        };
        if !valid {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid library storage key",
            ));
        }
        Ok(PathBuf::from(relative))
    }

    fn unresolved_file_path(&self, entry: &MediaEntry) -> std::io::Result<PathBuf> {
        Ok(self
            .storage
            .join(entry.storage_subdir())
            .join(Self::storage_relative_path(entry)?))
    }
}

impl MediaEntry {
    pub fn storage_subdir(&self) -> &'static str {
        if self.kind == "output" {
            "outputs"
        } else {
            "sources"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Create the on-disk file an entry points at, so `list()` keeps it.
    async fn touch(storage: &std::path::Path, kind: &str, filename: &str) {
        let sub = if kind == "output" {
            "outputs"
        } else {
            "sources"
        };
        let dir = storage.join(sub);
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join(filename), b"x").await.unwrap();
    }

    fn entry(id: &str, kind: &str, filename: &str, created_at: u64) -> MediaEntry {
        let sub = if kind == "output" {
            "outputs"
        } else {
            "sources"
        };
        MediaEntry {
            id: id.into(),
            kind: kind.into(),
            filename: filename.into(),
            storage_key: None,
            url: format!("/files/{sub}/{filename}"),
            media_type: (kind == "source").then(|| "video".into()),
            title: None,
            duration: None,
            width: None,
            height: None,
            fps: None,
            vcodec: None,
            acodec: None,
            size_bytes: None,
            created_at,
        }
    }

    #[tokio::test]
    async fn add_then_list_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        touch(&storage, "source", "a.mp4").await;
        touch(&storage, "source", "b.mp4").await;
        lib.add(entry("a", "source", "a.mp4", 100)).await;
        lib.add(entry("b", "source", "b.mp4", 200)).await;
        let list = lib.list().await;
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "b"); // created_at 200 -> newest first
        assert_eq!(list[1].id, "a");
    }

    #[tokio::test]
    async fn list_drops_entries_whose_file_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        touch(&storage, "source", "present.mp4").await;
        lib.add(entry("p", "source", "present.mp4", 1)).await;
        lib.add(entry("g", "source", "ghost.mp4", 2)).await; // never created
        let list = lib.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "p");
    }

    #[tokio::test]
    async fn add_dedups_by_id_keeping_latest() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        touch(&storage, "source", "f.mp4").await;
        lib.add(entry("same", "source", "f.mp4", 1)).await;
        lib.add(entry("same", "source", "f.mp4", 2)).await;
        let list = lib.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].created_at, 2);
    }

    #[tokio::test]
    async fn add_skips_empty_id_entries() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        touch(&storage, "source", "x.mp4").await;
        // Two empty-id results must not collapse into one colliding slot - both
        // are rejected outright.
        lib.add(entry("", "source", "x.mp4", 1)).await;
        lib.add(entry("", "source", "x.mp4", 2)).await;
        assert!(lib.get("").await.is_none());
        assert!(lib.list().await.is_empty());
    }

    #[tokio::test]
    async fn remove_deletes_file_and_entry() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        touch(&storage, "output", "o.mp4").await;
        lib.add(entry("o1", "output", "o.mp4", 1)).await;
        assert!(lib.remove("o1").await);
        assert!(!lib.remove("o1").await); // already gone
        assert!(tokio::fs::metadata(storage.join("outputs").join("o.mp4"))
            .await
            .is_err());
        assert!(lib.list().await.is_empty());
    }

    #[tokio::test]
    async fn space_source_is_restored_from_object_storage_when_local_copy_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let object_storage = SourceObjectStore::in_memory("tenant").unwrap();
        let lib = Library::load(storage.clone()).await;
        touch(&storage, "source", "cloud.mp4").await;
        let original = storage.join("sources/cloud.mp4");
        tokio::fs::write(&original, b"cloud source").await.unwrap();
        assert!(lib.add(entry("cloud", "source", "cloud.mp4", 1)).await);
        assert!(lib
            .relocate_source_to_space("cloud", "11111111-1111-4111-8111-111111111111")
            .await
            .unwrap());
        let lib = lib.with_object_storage(object_storage);
        assert_eq!(
            lib.sync_source_backups().await,
            SourceBackupSyncReport {
                uploaded: 1,
                unchanged: 0,
                failed: 0,
            }
        );
        assert_eq!(lib.sync_source_backups().await.unchanged, 1);
        let entry = lib.get("cloud").await.unwrap();
        let local = lib.resolve_media_path(&entry).await.unwrap();
        tokio::fs::remove_file(&local).await.unwrap();

        let restored = lib.resolve_media_path(&entry).await.unwrap();
        assert_eq!(tokio::fs::read(restored).await.unwrap(), b"cloud source");
    }

    #[tokio::test]
    async fn path_traversal_and_symlinks_are_never_resolved_or_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        tokio::fs::create_dir_all(storage.join("sources"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(storage.join("outputs"))
            .await
            .unwrap();
        let outside = dir.path().join("outside.mp4");
        tokio::fs::write(&outside, b"private").await.unwrap();
        let lib = Library::load(storage.clone()).await;
        let traversal = entry("bad", "source", "../outside.mp4", 1);
        assert!(!lib.add(traversal.clone()).await);
        assert!(lib.resolve_media_path(&traversal).await.is_err());
        assert_eq!(tokio::fs::read(&outside).await.unwrap(), b"private");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, storage.join("sources/link.mp4")).unwrap();
            let linked = entry("link", "source", "link.mp4", 1);
            assert!(lib.resolve_media_path(&linked).await.is_err());
        }
    }

    #[tokio::test]
    async fn entries_persist_across_reload() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        touch(&storage, "source", "keep.mp4").await;
        {
            let lib = Library::load(storage.clone()).await;
            lib.add(entry("k", "source", "keep.mp4", 5)).await;
        }
        let lib2 = Library::load(storage.clone()).await;
        let list = lib2.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "k");
    }

    #[tokio::test]
    async fn relocates_space_source_and_persists_safe_storage_key() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        touch(&storage, "source", "private.mp4").await;
        assert!(lib.add(entry("private", "source", "private.mp4", 1)).await);

        assert!(lib
            .relocate_source_to_space("private", "11111111-1111-4111-8111-111111111111")
            .await
            .unwrap());
        assert!(tokio::fs::metadata(storage.join("sources/private.mp4"))
            .await
            .is_err());
        let moved = lib.get("private").await.unwrap();
        assert_eq!(
            moved.storage_key.as_deref(),
            Some("spaces/11111111-1111-4111-8111-111111111111/private.mp4")
        );
        assert_eq!(
            tokio::fs::read(lib.resolve_media_path(&moved).await.unwrap())
                .await
                .unwrap(),
            b"x"
        );

        let reloaded = Library::load(storage).await;
        assert_eq!(reloaded.list().await[0].storage_key, moved.storage_key);
    }

    #[tokio::test]
    async fn rejects_unsafe_nested_storage_keys() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        tokio::fs::create_dir_all(storage.join("sources"))
            .await
            .unwrap();
        let lib = Library::load(storage).await;
        for key in [
            "../outside.mp4",
            "spaces/../private.mp4",
            "other/space/private.mp4",
        ] {
            let mut unsafe_entry = entry("unsafe", "source", "private.mp4", 1);
            unsafe_entry.storage_key = Some(key.into());
            assert!(!lib.add(unsafe_entry).await, "{key}");
        }
    }

    #[tokio::test]
    async fn list_is_read_only_and_keeps_hidden_entries() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        // No file yet: the entry is hidden from the listing...
        lib.add(entry("late", "source", "late.mp4", 1)).await;
        assert!(lib.list().await.is_empty());
        // ...but not pruned. Once the file appears it shows up again (the old
        // read-with-write list() would have dropped it permanently).
        touch(&storage, "source", "late.mp4").await;
        let list = lib.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "late");
    }

    #[test]
    fn from_result_maps_json_fields() {
        let v = json!({
            "id": "vid",
            "filename": "vid.mp4",
            "url": "/files/sources/vid.mp4",
            "title": "Hello",
            "duration": 12.5,
            "width": 1280,
            "height": 720,
            "fps": 29.97,
            "vcodec": "h264",
            "acodec": "aac",
            "sizeBytes": 999
        });
        let e = MediaEntry::from_result("source", &v);
        assert_eq!(e.id, "vid");
        assert_eq!(e.kind, "source");
        assert_eq!(e.filename, "vid.mp4");
        assert_eq!(e.title.as_deref(), Some("Hello"));
        assert_eq!(e.duration, Some(12.5));
        assert_eq!(e.width, Some(1280));
        assert_eq!(e.height, Some(720));
        assert_eq!(e.fps, Some(29.97));
        assert_eq!(e.vcodec.as_deref(), Some("h264"));
        assert_eq!(e.acodec.as_deref(), Some("aac"));
        assert_eq!(e.size_bytes, Some(999));
    }
}
