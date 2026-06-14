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

    async fn save(&self, entries: &[MediaEntry]) {
        if let Ok(json) = serde_json::to_vec_pretty(entries) {
            // Write to a temp file then rename, so a crash never leaves a partial file.
            let tmp = self.path.with_extension("json.tmp");
            if tokio::fs::write(&tmp, &json).await.is_ok() {
                let _ = tokio::fs::rename(&tmp, &self.path).await;
            }
        }
    }

    pub async fn add(&self, entry: MediaEntry) {
        let mut guard = self.entries.lock().await;
        guard.retain(|e| e.id != entry.id);
        guard.push(entry);
        let snapshot = guard.clone();
        drop(guard);
        self.save(&snapshot).await;
    }

    /// Return entries newest-first, dropping any whose file no longer exists.
    pub async fn list(&self) -> Vec<MediaEntry> {
        let mut guard = self.entries.lock().await;
        let mut kept = Vec::with_capacity(guard.len());
        for e in guard.iter() {
            if tokio::fs::metadata(self.file_path(e)).await.is_ok() {
                kept.push(e.clone());
            }
        }
        let changed = kept.len() != guard.len();
        *guard = kept.clone();
        drop(guard);
        if changed {
            self.save(&kept).await;
        }
        kept.sort_by_key(|e| std::cmp::Reverse(e.created_at));
        kept
    }

    /// Remove an entry and delete its file. Returns true if it existed.
    pub async fn remove(&self, id: &str) -> bool {
        let mut guard = self.entries.lock().await;
        let Some(pos) = guard.iter().position(|e| e.id == id) else {
            return false;
        };
        let entry = guard.remove(pos);
        let snapshot = guard.clone();
        drop(guard);
        let _ = tokio::fs::remove_file(self.file_path(&entry)).await;
        self.save(&snapshot).await;
        true
    }

    fn file_path(&self, e: &MediaEntry) -> PathBuf {
        let sub = if e.kind == "output" { "outputs" } else { "sources" };
        self.storage.join(sub).join(&e.filename)
    }
}
