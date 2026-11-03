//! Private durable native-template catalogue. Package bytes live in immutable files.
use super::Db;
use anyhow::{ensure, Result};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Row, SqlitePool};
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTemplateEntry {
    pub id: String,
    pub name: String,
    pub info: Value,
    pub bytes: u64,
    pub created_at: u64,
    #[serde(skip)]
    pub sha256: String,
}
pub(super) async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query("CREATE TABLE IF NOT EXISTS native_template_library(id TEXT PRIMARY KEY,owner TEXT NOT NULL,name TEXT NOT NULL,info_json TEXT NOT NULL,bytes INTEGER NOT NULL,sha256 TEXT NOT NULL,created_at INTEGER NOT NULL,FOREIGN KEY(owner) REFERENCES auth_users(username) ON DELETE CASCADE); CREATE INDEX IF NOT EXISTS native_template_owner_name ON native_template_library(owner,name,id);").execute(pool).await?;
    Ok(())
}
fn entry(row: sqlx::sqlite::SqliteRow) -> Result<NativeTemplateEntry> {
    Ok(NativeTemplateEntry {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        info: serde_json::from_str(&row.try_get::<String, _>("info_json")?)?,
        bytes: row.try_get::<i64, _>("bytes")? as u64,
        sha256: row.try_get("sha256")?,
        created_at: row.try_get::<i64, _>("created_at")? as u64,
    })
}
impl Db {
    pub(crate) async fn native_templates(&self, owner: &str) -> Result<Vec<NativeTemplateEntry>> {
        sqlx::query("SELECT * FROM native_template_library WHERE owner=? ORDER BY name,id")
            .bind(owner)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(entry)
            .collect()
    }
    pub(crate) async fn native_template(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<Option<NativeTemplateEntry>> {
        sqlx::query("SELECT * FROM native_template_library WHERE owner=? AND id=?")
            .bind(owner)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .map(entry)
            .transpose()
    }
    pub(crate) async fn insert_native_template(
        &self,
        owner: &str,
        record: &NativeTemplateEntry,
    ) -> Result<()> {
        ensure!(
            uuid::Uuid::parse_str(&record.id).is_ok(),
            "Invalid template identity"
        );
        ensure!(
            record.bytes > 0 && record.bytes <= crate::project_archive::MAX_ARCHIVE_BYTES,
            "Invalid template size"
        );
        let info = serde_json::to_string(&record.info)?;
        ensure!(info.len() <= 1024 * 1024, "Template metadata too large");
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM native_template_library WHERE owner=? AND name=?)",
        )
        .bind(owner)
        .bind(&record.name)
        .fetch_one(&mut *tx)
        .await?;
        ensure!(!exists, "Template name already exists");
        let (count, bytes): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*),COALESCE(SUM(bytes),0) FROM native_template_library WHERE owner=?",
        )
        .bind(owner)
        .fetch_one(&mut *tx)
        .await?;
        ensure!(
            count < 128 && bytes as u64 + record.bytes <= 16 * 1024 * 1024 * 1024,
            "Template library quota exceeded"
        );
        sqlx::query("INSERT INTO native_template_library(id,owner,name,info_json,bytes,sha256,created_at) VALUES(?,?,?,?,?,?,?)").bind(&record.id).bind(owner).bind(&record.name).bind(info).bind(record.bytes as i64).bind(&record.sha256).bind(record.created_at as i64).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn delete_native_template(&self, owner: &str, id: &str) -> Result<bool> {
        Ok(
            sqlx::query("DELETE FROM native_template_library WHERE owner=? AND id=?")
                .bind(owner)
                .bind(id)
                .execute(&self.pool)
                .await?
                .rows_affected()
                == 1,
        )
    }
}
