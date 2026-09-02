use std::cmp::Ordering;
use std::sync::Arc;

use anyhow::{ensure, Context, Result};

use crate::db::Db;
use crate::library::MediaEntry;
use crate::ports::{MediaDocument, MediaIndexWriter};

const CURSOR_NAME: &str = "library-v1";
const CURSOR_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS media_index_cursors (
    name TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    entry_id TEXT NOT NULL
);
";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexReport {
    pub indexed: usize,
    pub skipped: usize,
    pub bad_entries: Vec<String>,
    pub deferred_error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct IndexCursor {
    created_at: u64,
    entry_id: String,
}

impl IndexCursor {
    fn includes(&self, entry: &MediaEntry) -> bool {
        compare_entry(entry, self) != Ordering::Greater
    }

    fn advance(&mut self, entry: &MediaEntry) {
        self.created_at = entry.created_at;
        self.entry_id.clone_from(&entry.id);
    }
}

#[derive(Clone)]
pub struct MediaIndexer {
    db: Db,
    writer: Arc<dyn MediaIndexWriter>,
}

impl MediaIndexer {
    pub fn new(db: Db, writer: Arc<dyn MediaIndexWriter>) -> Self {
        Self { db, writer }
    }

    pub async fn migrate(db: &Db) -> Result<()> {
        sqlx::query(CURSOR_SCHEMA).execute(db.pool()).await?;
        Ok(())
    }

    pub async fn sync_incremental(&self, entries: &[MediaEntry]) -> Result<IndexReport> {
        let mut cursor = self.load_cursor().await?;
        let mut entries = entries.to_vec();
        entries.sort_by(compare_entries);
        let mut report = IndexReport::default();
        for entry in &entries {
            if cursor.includes(entry) {
                report.skipped += 1;
                continue;
            }
            let document = match document_from_entry(entry) {
                Ok(document) => document,
                Err(error) => {
                    report.bad_entries.push(format!("{}: {error}", entry.id));
                    cursor.advance(entry);
                    self.save_cursor(&cursor).await?;
                    continue;
                }
            };
            if let Err(error) = self.writer.index(&document).await {
                report.deferred_error = Some(error.to_string());
                break;
            }
            cursor.advance(entry);
            self.save_cursor(&cursor).await?;
            report.indexed += 1;
        }
        Ok(report)
    }

    pub async fn rebuild(&self, entries: &[MediaEntry]) -> Result<IndexReport> {
        self.writer.rebuild(&[]).await?;
        let mut entries = entries.to_vec();
        entries.sort_by(compare_entries);
        let mut report = IndexReport::default();
        let mut cursor = IndexCursor::default();
        for entry in &entries {
            match document_from_entry(entry) {
                Ok(document) => match self.writer.index(&document).await {
                    Ok(()) => {
                        report.indexed += 1;
                        cursor.advance(entry);
                    }
                    Err(error) => {
                        report.deferred_error = Some(error.to_string());
                        break;
                    }
                },
                Err(error) => {
                    report.bad_entries.push(format!("{}: {error}", entry.id));
                    cursor.advance(entry);
                }
            }
        }
        self.save_cursor(&cursor).await?;
        Ok(report)
    }

    pub async fn index_entry(&self, entry: &MediaEntry) -> Result<()> {
        let document = document_from_entry(entry)?;
        self.writer.index(&document).await?;
        let mut cursor = self.load_cursor().await?;
        if !cursor.includes(entry) {
            cursor.advance(entry);
            self.save_cursor(&cursor).await?;
        }
        Ok(())
    }

    async fn load_cursor(&self) -> Result<IndexCursor> {
        let row = sqlx::query!(
            "SELECT created_at, entry_id FROM media_index_cursors WHERE name = ?",
            CURSOR_NAME
        )
        .fetch_optional(self.db.pool())
        .await?;
        row.map(|row| {
            let created_at = row.created_at;
            Ok(IndexCursor {
                created_at: u64::try_from(created_at).context("negative media index cursor")?,
                entry_id: row.entry_id,
            })
        })
        .transpose()
        .map(Option::unwrap_or_default)
    }

    async fn save_cursor(&self, cursor: &IndexCursor) -> Result<()> {
        let created_at = i64::try_from(cursor.created_at).context("media cursor overflow")?;
        sqlx::query(
            "INSERT INTO media_index_cursors (name, created_at, entry_id) VALUES (?, ?, ?) \
             ON CONFLICT(name) DO UPDATE SET created_at = excluded.created_at, entry_id = excluded.entry_id",
        )
        .bind(CURSOR_NAME)
        .bind(created_at)
        .bind(&cursor.entry_id)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
}

fn document_from_entry(entry: &MediaEntry) -> Result<MediaDocument> {
    ensure!(!entry.id.is_empty(), "empty id");
    ensure!(
        matches!(entry.kind.as_str(), "source" | "output"),
        "invalid kind"
    );
    ensure!(!entry.filename.is_empty(), "empty filename");
    ensure!(!entry.filename.contains(['/', '\\']), "nested filename");
    Ok(MediaDocument::from(entry))
}

fn compare_entries(left: &MediaEntry, right: &MediaEntry) -> Ordering {
    left.created_at
        .cmp(&right.created_at)
        .then_with(|| left.id.cmp(&right.id))
}

fn compare_entry(entry: &MediaEntry, cursor: &IndexCursor) -> Ordering {
    entry
        .created_at
        .cmp(&cursor.created_at)
        .then_with(|| entry.id.cmp(&cursor.entry_id))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tokio::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingWriter(Mutex<BTreeMap<String, MediaDocument>>);

    #[axum::async_trait]
    impl MediaIndexWriter for RecordingWriter {
        async fn index(&self, document: &MediaDocument) -> Result<()> {
            self.0
                .lock()
                .await
                .insert(document.id.clone(), document.clone());
            Ok(())
        }

        async fn remove(&self, id: &str) -> Result<()> {
            self.0.lock().await.remove(id);
            Ok(())
        }

        async fn rebuild(&self, documents: &[MediaDocument]) -> Result<()> {
            let mut stored = self.0.lock().await;
            stored.clear();
            stored.extend(
                documents
                    .iter()
                    .map(|value| (value.id.clone(), value.clone())),
            );
            Ok(())
        }
    }

    fn entry(id: &str, kind: &str, created_at: u64) -> MediaEntry {
        MediaEntry {
            id: id.into(),
            kind: kind.into(),
            filename: format!("{id}.mp4"),
            url: String::new(),
            media_type: None,
            title: Some(id.into()),
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
    async fn cursor_is_incremental_and_bad_entries_do_not_block_followers() {
        let directory = tempfile::tempdir().unwrap();
        let db = Db::open(directory.path()).await.unwrap();
        let writer = Arc::new(RecordingWriter::default());
        let indexer = MediaIndexer::new(db, writer.clone());
        let entries = vec![
            entry("one", "source", 1),
            entry("bad", "unknown", 2),
            entry("three", "output", 3),
        ];

        let first = indexer.sync_incremental(&entries).await.unwrap();
        assert_eq!(first.indexed, 2);
        assert_eq!(first.bad_entries.len(), 1);
        let second = indexer.sync_incremental(&entries).await.unwrap();
        assert_eq!(second.skipped, 3);
        assert_eq!(writer.0.lock().await.len(), 2);

        let mut expanded = entries;
        expanded.push(entry("four", "source", 4));
        assert_eq!(
            indexer.sync_incremental(&expanded).await.unwrap().indexed,
            1
        );
        assert!(writer.0.lock().await.contains_key("four"));
    }

    #[tokio::test]
    async fn rebuild_discards_derived_state_and_recreates_cursor() {
        let directory = tempfile::tempdir().unwrap();
        let db = Db::open(directory.path()).await.unwrap();
        let writer = Arc::new(RecordingWriter::default());
        let indexer = MediaIndexer::new(db, writer.clone());
        let report = indexer
            .rebuild(&[entry("one", "source", 1), entry("two", "output", 2)])
            .await
            .unwrap();
        assert_eq!(report.indexed, 2);
        assert_eq!(
            indexer
                .sync_incremental(&[entry("two", "output", 2)])
                .await
                .unwrap()
                .skipped,
            1
        );
    }
}
