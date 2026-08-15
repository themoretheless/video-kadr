use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use serde::Serialize;
use sqlx::{Row, SqlitePool};

use crate::library::now_secs;

pub const MAX_LIBRARY_TITLE_CHARS: usize = 120;
pub const MAX_LIBRARY_TITLE_BYTES: usize = 512;
pub const MAX_LIBRARY_TAGS: usize = 20;
pub const MAX_LIBRARY_TAG_CHARS: usize = 32;
pub const MAX_LIBRARY_TAG_BYTES: usize = 128;
pub const MAX_LIBRARY_ID_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryMetadata {
    pub library_id: String,
    pub title: Option<String>,
    pub favorite: bool,
    pub tags: Vec<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct LibraryMetadataChanges {
    /// `None` keeps the current title; `Some(None)` clears it.
    pub title: Option<Option<String>>,
    pub favorite: Option<bool>,
    pub tags: Option<Vec<String>>,
}

pub(super) async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS library_metadata (
            library_id TEXT PRIMARY KEY NOT NULL,
            title TEXT,
            favorite INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
            updated_at INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS library_metadata_tags (
            library_id TEXT NOT NULL,
            tag TEXT NOT NULL,
            tag_order INTEGER NOT NULL CHECK (tag_order >= 0),
            PRIMARY KEY (library_id, tag),
            UNIQUE (library_id, tag_order),
            FOREIGN KEY (library_id) REFERENCES library_metadata(library_id) ON DELETE CASCADE
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_library_metadata_favorite_updated
         ON library_metadata(favorite DESC, updated_at DESC)",
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub fn validate_library_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > MAX_LIBRARY_ID_BYTES || id.chars().any(char::is_control) {
        return Err(anyhow!("invalid library id"));
    }
    Ok(())
}

pub fn normalize_library_title(title: Option<String>) -> Result<Option<String>> {
    let Some(title) = title else {
        return Ok(None);
    };
    let title = title.trim();
    if title.is_empty() {
        return Ok(None);
    }
    if title.len() > MAX_LIBRARY_TITLE_BYTES
        || title.chars().count() > MAX_LIBRARY_TITLE_CHARS
        || title.chars().any(char::is_control)
    {
        return Err(anyhow!(
            "library title exceeds limits or contains control characters"
        ));
    }
    Ok(Some(title.to_owned()))
}

pub fn normalize_library_tags(tags: Vec<String>) -> Result<Vec<String>> {
    if tags.len() > MAX_LIBRARY_TAGS {
        return Err(anyhow!("too many library tags"));
    }
    let mut normalized = Vec::with_capacity(tags.len());
    let mut seen = HashSet::with_capacity(tags.len());
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty()
            || tag.len() > MAX_LIBRARY_TAG_BYTES
            || tag.chars().count() > MAX_LIBRARY_TAG_CHARS
            || tag.chars().any(char::is_control)
        {
            return Err(anyhow!("library tag exceeds limits or is empty"));
        }
        let key = tag.to_lowercase();
        if !seen.insert(key) {
            return Err(anyhow!("duplicate library tag"));
        }
        normalized.push(tag.to_owned());
    }
    Ok(normalized)
}

impl super::Db {
    pub async fn get_library_metadata(&self, library_id: &str) -> Result<Option<LibraryMetadata>> {
        validate_library_id(library_id)?;
        let row = sqlx::query(
            "SELECT library_id, title, favorite, updated_at
             FROM library_metadata WHERE library_id = ?",
        )
        .bind(library_id)
        .fetch_optional(self.pool())
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let tags = fetch_tags(self.pool(), library_id).await?;
        Ok(Some(LibraryMetadata {
            library_id: row.try_get("library_id")?,
            title: row.try_get("title")?,
            favorite: row.try_get::<i64, _>("favorite")? != 0,
            tags,
            updated_at: row.try_get("updated_at")?,
        }))
    }

    pub async fn list_library_metadata(&self) -> Result<HashMap<String, LibraryMetadata>> {
        let rows =
            sqlx::query("SELECT library_id, title, favorite, updated_at FROM library_metadata")
                .fetch_all(self.pool())
                .await?;
        let tag_rows = sqlx::query(
            "SELECT library_id, tag FROM library_metadata_tags
             ORDER BY library_id, tag_order",
        )
        .fetch_all(self.pool())
        .await?;
        let mut tags_by_id: HashMap<String, Vec<String>> = HashMap::new();
        for row in tag_rows {
            tags_by_id
                .entry(row.try_get("library_id")?)
                .or_default()
                .push(row.try_get("tag")?);
        }
        let mut metadata = HashMap::with_capacity(rows.len());
        for row in rows {
            let library_id: String = row.try_get("library_id")?;
            metadata.insert(
                library_id.clone(),
                LibraryMetadata {
                    library_id: library_id.clone(),
                    title: row.try_get("title")?,
                    favorite: row.try_get::<i64, _>("favorite")? != 0,
                    tags: tags_by_id.remove(&library_id).unwrap_or_default(),
                    updated_at: row.try_get("updated_at")?,
                },
            );
        }
        Ok(metadata)
    }

    pub async fn replace_library_metadata(
        &self,
        library_id: &str,
        title: Option<String>,
        favorite: bool,
        tags: Vec<String>,
    ) -> Result<LibraryMetadata> {
        self.update_library_metadata(
            library_id,
            LibraryMetadataChanges {
                title: Some(title),
                favorite: Some(favorite),
                tags: Some(tags),
            },
        )
        .await
    }

    pub async fn update_library_metadata(
        &self,
        library_id: &str,
        changes: LibraryMetadataChanges,
    ) -> Result<LibraryMetadata> {
        validate_library_id(library_id)?;
        let mut transaction = self.pool().begin().await?;
        let row = sqlx::query("SELECT title, favorite FROM library_metadata WHERE library_id = ?")
            .bind(library_id)
            .fetch_optional(&mut *transaction)
            .await?;
        let current_title = row
            .as_ref()
            .map(|row| row.try_get("title"))
            .transpose()?
            .flatten();
        let current_favorite = row
            .as_ref()
            .map(|row| row.try_get::<i64, _>("favorite"))
            .transpose()?
            .unwrap_or(0)
            != 0;
        let current_tags = {
            let rows = sqlx::query(
                "SELECT tag FROM library_metadata_tags
                 WHERE library_id = ? ORDER BY tag_order",
            )
            .bind(library_id)
            .fetch_all(&mut *transaction)
            .await?;
            rows.into_iter()
                .map(|row| row.try_get("tag"))
                .collect::<std::result::Result<Vec<String>, sqlx::Error>>()?
        };

        let title = changes.title.unwrap_or(current_title);
        let favorite = changes.favorite.unwrap_or(current_favorite);
        let tags = changes.tags.unwrap_or(current_tags);
        let updated_at = now_secs() as i64;
        sqlx::query(
            "INSERT INTO library_metadata (library_id, title, favorite, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(library_id) DO UPDATE SET
               title = excluded.title,
               favorite = excluded.favorite,
               updated_at = excluded.updated_at",
        )
        .bind(library_id)
        .bind(&title)
        .bind(i64::from(favorite))
        .bind(updated_at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM library_metadata_tags WHERE library_id = ?")
            .bind(library_id)
            .execute(&mut *transaction)
            .await?;
        for (order, tag) in tags.iter().enumerate() {
            sqlx::query(
                "INSERT INTO library_metadata_tags (library_id, tag, tag_order)
                 VALUES (?, ?, ?)",
            )
            .bind(library_id)
            .bind(tag)
            .bind(order as i64)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(LibraryMetadata {
            library_id: library_id.to_owned(),
            title,
            favorite,
            tags,
            updated_at,
        })
    }

    /// Deleting the parent row cascades ordered tags in the same SQLite statement.
    pub async fn delete_library_metadata(&self, library_id: &str) -> Result<bool> {
        validate_library_id(library_id)?;
        let result = sqlx::query("DELETE FROM library_metadata WHERE library_id = ?")
            .bind(library_id)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

async fn fetch_tags(pool: &SqlitePool, library_id: &str) -> Result<Vec<String>> {
    let rows = sqlx::query(
        "SELECT tag FROM library_metadata_tags
         WHERE library_id = ? ORDER BY tag_order",
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| row.try_get("tag").map_err(Into::into))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_trims_and_rejects_ambiguous_tags() {
        assert_eq!(
            normalize_library_title(Some("  Interview  ".into())).unwrap(),
            Some("Interview".into())
        );
        assert_eq!(
            normalize_library_tags(vec![" work ".into(), "2026".into()]).unwrap(),
            vec!["work", "2026"]
        );
        assert!(normalize_library_tags(vec!["Work".into(), "work".into()]).is_err());
        assert!(normalize_library_tags(vec!["x".repeat(MAX_LIBRARY_TAG_CHARS + 1)]).is_err());
        assert!(normalize_library_title(Some("x".repeat(MAX_LIBRARY_TITLE_CHARS + 1))).is_err());
    }

    #[tokio::test]
    async fn replace_patch_and_delete_are_stable_and_cascade_tags() {
        let directory = tempfile::tempdir().unwrap();
        let db = super::super::Db::open(directory.path()).await.unwrap();
        db.replace_library_metadata(
            "clip-1",
            Some("Interview".into()),
            true,
            vec!["work".into(), "person".into()],
        )
        .await
        .unwrap();
        let patched = db
            .update_library_metadata(
                "clip-1",
                LibraryMetadataChanges {
                    favorite: Some(false),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(patched.title.as_deref(), Some("Interview"));
        assert_eq!(patched.tags, ["work", "person"]);
        assert!(!patched.favorite);

        assert!(db.delete_library_metadata("clip-1").await.unwrap());
        assert!(db.get_library_metadata("clip-1").await.unwrap().is_none());
        let tag_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM library_metadata_tags WHERE library_id = 'clip-1'",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(tag_count, 0);
    }
}
