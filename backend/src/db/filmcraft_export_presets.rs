use super::Db;
use crate::domain::filmcraft_export_presets::Library;
use anyhow::Result;

pub(super) async fn migrate(pool: &sqlx::SqlitePool) -> Result<()> {
    sqlx::query("CREATE TABLE IF NOT EXISTS filmcraft_export_preset_libraries (owner TEXT PRIMARY KEY, document TEXT NOT NULL)")
        .execute(pool).await?;
    Ok(())
}

impl Db {
    pub async fn filmcraft_export_presets(&self, owner: &str) -> Result<Library> {
        let document: Option<String> = sqlx::query_scalar(
            "SELECT document FROM filmcraft_export_preset_libraries WHERE owner=?",
        )
        .bind(owner)
        .fetch_optional(&self.pool)
        .await?;
        document
            .map(|d| Library::parse(d.as_bytes()))
            .unwrap_or_else(|| Ok(Library::default()))
    }

    /// Serialize read-modify-write across connections/processes. Failed edits roll back.
    pub async fn edit_filmcraft_export_presets<T>(
        &self,
        owner: &str,
        edit: impl FnOnce(&mut Library) -> Result<T>,
    ) -> Result<T> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let document: Option<String> = sqlx::query_scalar(
            "SELECT document FROM filmcraft_export_preset_libraries WHERE owner=?",
        )
        .bind(owner)
        .fetch_optional(&mut *transaction)
        .await?;
        let mut library = document
            .map(|d| Library::parse(d.as_bytes()))
            .unwrap_or_else(|| Ok(Library::default()))?;
        let result = edit(&mut library)?;
        sqlx::query("INSERT INTO filmcraft_export_preset_libraries(owner,document) VALUES(?,?) ON CONFLICT(owner) DO UPDATE SET document=excluded.document")
            .bind(owner).bind(serde_json::to_string(&library)?).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(result)
    }
}
