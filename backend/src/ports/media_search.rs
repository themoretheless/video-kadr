use anyhow::Result;
use serde::Serialize;
use sqlx::Row;

use crate::db::Db;
use crate::library::MediaEntry;

const SEARCH_SCHEMA: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS media_search_fts USING fts5(
    id UNINDEXED,
    kind UNINDEXED,
    title,
    filename,
    tokenize = 'unicode61 remove_diacritics 2'
);
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaDocument {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub filename: String,
}

impl From<&MediaEntry> for MediaDocument {
    fn from(entry: &MediaEntry) -> Self {
        Self {
            id: entry.id.clone(),
            kind: entry.kind.clone(),
            title: entry.title.clone().unwrap_or_default(),
            filename: entry.filename.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub filename: String,
    pub score: f64,
}

#[axum::async_trait]
pub trait MediaSearchQuery: Send + Sync {
    async fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>>;
}

#[axum::async_trait]
pub trait MediaIndexWriter: Send + Sync {
    async fn index(&self, document: &MediaDocument) -> Result<()>;
    async fn remove(&self, id: &str) -> Result<()>;
    async fn rebuild(&self, documents: &[MediaDocument]) -> Result<()>;
}

#[derive(Clone)]
pub struct SqliteMediaSearch {
    db: Db,
}

impl SqliteMediaSearch {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn migrate(db: &Db) -> Result<()> {
        sqlx::query(SEARCH_SCHEMA).execute(db.pool()).await?;
        Ok(())
    }
}

#[axum::async_trait]
impl MediaSearchQuery for SqliteMediaSearch {
    async fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let Some(query) = safe_fts_query(query) else {
            return Ok(Vec::new());
        };
        let rows = sqlx::query(
            "SELECT id, kind, title, filename, bm25(media_search_fts) AS score \
             FROM media_search_fts WHERE media_search_fts MATCH ? \
             ORDER BY score, rowid LIMIT ?",
        )
        .bind(query)
        .bind(i64::from(limit.clamp(1, 100)))
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(SearchHit {
                    id: row.try_get("id")?,
                    kind: row.try_get("kind")?,
                    title: row.try_get("title")?,
                    filename: row.try_get("filename")?,
                    score: row.try_get("score")?,
                })
            })
            .collect()
    }
}

#[axum::async_trait]
impl MediaIndexWriter for SqliteMediaSearch {
    async fn index(&self, document: &MediaDocument) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;
        replace_document(&mut tx, document).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn remove(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM media_search_fts WHERE id = ?")
            .bind(id)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    async fn rebuild(&self, documents: &[MediaDocument]) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;
        sqlx::query("DELETE FROM media_search_fts")
            .execute(&mut *tx)
            .await?;
        for document in documents {
            replace_document(&mut tx, document).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

async fn replace_document(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    document: &MediaDocument,
) -> Result<()> {
    sqlx::query("DELETE FROM media_search_fts WHERE id = ?")
        .bind(&document.id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO media_search_fts (id, kind, title, filename) VALUES (?, ?, ?, ?)")
        .bind(&document.id)
        .bind(&document.kind)
        .bind(&document.title)
        .bind(&document.filename)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn safe_fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .filter_map(|term| {
            let clean: String = term
                .chars()
                .filter(|character| character.is_alphanumeric() || matches!(character, '-' | '_'))
                .take(64)
                .collect();
            (!clean.is_empty()).then(|| format!("\"{clean}\"*"))
        })
        .take(8)
        .collect();
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tokio::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct InMemoryMediaSearch {
        documents: Mutex<BTreeMap<String, MediaDocument>>,
    }

    #[axum::async_trait]
    impl MediaSearchQuery for InMemoryMediaSearch {
        async fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>> {
            if limit == 0 || query.trim().is_empty() {
                return Ok(Vec::new());
            }
            let query = query.to_lowercase();
            Ok(self
                .documents
                .lock()
                .await
                .values()
                .filter(|document| {
                    document.title.to_lowercase().contains(&query)
                        || document.filename.to_lowercase().contains(&query)
                })
                .take(limit.min(100) as usize)
                .map(|document| SearchHit {
                    id: document.id.clone(),
                    kind: document.kind.clone(),
                    title: document.title.clone(),
                    filename: document.filename.clone(),
                    score: 0.0,
                })
                .collect())
        }
    }

    #[axum::async_trait]
    impl MediaIndexWriter for InMemoryMediaSearch {
        async fn index(&self, document: &MediaDocument) -> Result<()> {
            self.documents
                .lock()
                .await
                .insert(document.id.clone(), document.clone());
            Ok(())
        }

        async fn remove(&self, id: &str) -> Result<()> {
            self.documents.lock().await.remove(id);
            Ok(())
        }

        async fn rebuild(&self, documents: &[MediaDocument]) -> Result<()> {
            let mut stored = self.documents.lock().await;
            stored.clear();
            for document in documents {
                stored.insert(document.id.clone(), document.clone());
            }
            Ok(())
        }
    }

    async fn assert_media_search_contract(search: &(impl MediaSearchQuery + MediaIndexWriter)) {
        let old = MediaDocument {
            id: "shared".into(),
            kind: "source".into(),
            title: "Old sunset".into(),
            filename: "old.mp4".into(),
        };
        let current = MediaDocument {
            id: "shared".into(),
            kind: "output".into(),
            title: "Fresh sunrise".into(),
            filename: "fresh.mp4".into(),
        };

        search
            .rebuild(&[old.clone(), current.clone()])
            .await
            .unwrap();
        assert!(search.search("sunset", 10).await.unwrap().is_empty());
        let hits = search.search("sunrise", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "shared");
        assert_eq!(hits[0].kind, "output");
        assert!(search.search("sunrise", 0).await.unwrap().is_empty());

        search.index(&old).await.unwrap();
        assert!(search.search("sunrise", 10).await.unwrap().is_empty());
        assert_eq!(search.search("sunset", 10).await.unwrap().len(), 1);
        search.remove("missing").await.unwrap();
        search.remove("shared").await.unwrap();
        assert!(search.search("sunset", 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn in_memory_search_obeys_port_contract() {
        assert_media_search_contract(&InMemoryMediaSearch::default()).await;
    }

    #[tokio::test]
    async fn sqlite_search_obeys_port_contract() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).await.unwrap();
        assert_media_search_contract(&SqliteMediaSearch::new(db)).await;
    }

    #[tokio::test]
    async fn sqlite_search_is_ranked_safe_and_rebuildable() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).await.unwrap();
        let search = SqliteMediaSearch::new(db);
        let documents = vec![
            MediaDocument {
                id: "one".into(),
                kind: "source".into(),
                title: "Summer interview".into(),
                filename: "interview-final.mp4".into(),
            },
            MediaDocument {
                id: "two".into(),
                kind: "output".into(),
                title: "Winter reel".into(),
                filename: "reel.mp4".into(),
            },
        ];
        search.rebuild(&documents).await.unwrap();
        assert_eq!(search.search("interv", 10).await.unwrap()[0].id, "one");
        assert!(search.search("\" OR *", 10).await.unwrap().is_empty());

        search.remove("one").await.unwrap();
        assert!(search.search("interview", 10).await.unwrap().is_empty());
        search.index(&documents[0]).await.unwrap();
        assert_eq!(search.search("summer", 10).await.unwrap()[0].id, "one");
    }
}
