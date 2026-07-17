use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

/// One persisted media item: an imported/uploaded source or a rendered output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaEntry {
    pub id: String,
    /// "source" or "output".
    pub kind: String,
    pub filename: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
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
            url: v["url"].as_str().unwrap_or_default().to_string(),
            title: v["title"].as_str().map(|s| s.to_string()),
            duration: v["duration"].as_f64(),
            width: v["width"].as_u64().map(|n| n as u32),
            height: v["height"].as_u64().map(|n| n as u32),
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
        }
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
        if entry.id.is_empty() || entry.filename.is_empty() {
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
            if tokio::fs::metadata(self.file_path(&e)).await.is_ok() {
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

    /// Remove an entry and delete its file. Returns true if it existed and the
    /// removal was persisted. The file is deleted only after a successful save,
    /// so a persist failure never deletes a file the stored library still lists.
    pub async fn remove(&self, id: &str) -> bool {
        let mut guard = self.entries.lock().await;
        let Some(pos) = guard.iter().position(|e| e.id == id) else {
            return false;
        };
        let entry = guard[pos].clone();
        let mut next = guard.clone();
        next.remove(pos);
        if let Err(e) = self.save(&next).await {
            tracing::error!(id, error = %e, "library: persist failed on remove, keeping entry");
            return false;
        }
        *guard = next;
        drop(guard);
        let _ = tokio::fs::remove_file(self.file_path(&entry)).await;
        true
    }

    fn file_path(&self, e: &MediaEntry) -> PathBuf {
        let sub = if e.kind == "output" {
            "outputs"
        } else {
            "sources"
        };
        self.storage.join(sub).join(&e.filename)
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
            url: format!("/files/{sub}/{filename}"),
            title: None,
            duration: None,
            width: None,
            height: None,
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
        assert_eq!(e.size_bytes, Some(999));
    }
}
