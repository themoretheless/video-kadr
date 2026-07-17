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
pub trait MediaSearch: Send + Sync {
    async fn index(&self, document: &MediaDocument) -> Result<()>;
    async fn remove(&self, id: &str) -> Result<()>;
    async fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>>;
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
impl MediaSearch for SqliteMediaSearch {
    async fn index(&self, document: &MediaDocument) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;
        sqlx::query("DELETE FROM media_search_fts WHERE id = ?")
            .bind(&document.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO media_search_fts (id, kind, title, filename) VALUES (?, ?, ?, ?)")
            .bind(&document.id)
            .bind(&document.kind)
            .bind(&document.title)
            .bind(&document.filename)
            .execute(&mut *tx)
            .await?;
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

    async fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>> {
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

    async fn rebuild(&self, documents: &[MediaDocument]) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;
        sqlx::query("DELETE FROM media_search_fts")
            .execute(&mut *tx)
            .await?;
        for document in documents {
            sqlx::query(
                "INSERT INTO media_search_fts (id, kind, title, filename) VALUES (?, ?, ?, ?)",
            )
            .bind(&document.id)
            .bind(&document.kind)
            .bind(&document.title)
            .bind(&document.filename)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
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
    use super::*;

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
