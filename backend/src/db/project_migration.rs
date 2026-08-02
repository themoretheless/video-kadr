//! One-time repair of the legacy project key before uniqueness is enforced.

use anyhow::{Context, Result};
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::domain::project::{ProjectDocument, PROJECT_DOCUMENT_SCHEMA_VERSION};
use crate::library::now_secs;

pub(super) async fn migrate(pool: &SqlitePool) -> Result<()> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let has_updated_order: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('projects') WHERE name = 'updated_order'",
    )
    .fetch_one(&mut *transaction)
    .await?;
    if has_updated_order == 0 {
        sqlx::query("ALTER TABLE projects ADD COLUMN updated_order INTEGER NOT NULL DEFAULT 0")
            .execute(&mut *transaction)
            .await?;
    }
    let has_document_json: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('projects') WHERE name = 'document_json'",
    )
    .fetch_one(&mut *transaction)
    .await?;
    if has_document_json == 0 {
        sqlx::query("ALTER TABLE projects ADD COLUMN document_json TEXT")
            .execute(&mut *transaction)
            .await?;
    }
    let has_revision: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('projects') WHERE name = 'revision'",
    )
    .fetch_one(&mut *transaction)
    .await?;
    if has_revision == 0 {
        sqlx::query("ALTER TABLE projects ADD COLUMN revision INTEGER NOT NULL DEFAULT 0")
            .execute(&mut *transaction)
            .await?;
    }
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS project_migration_conflicts ( \
           project_id TEXT PRIMARY KEY, \
           name TEXT NOT NULL, \
           video_id TEXT NOT NULL, \
           video_json TEXT NOT NULL, \
           edit_json TEXT NOT NULL, \
           schema_version INTEGER NOT NULL, \
           created_at INTEGER NOT NULL, \
           updated_at INTEGER NOT NULL, \
           reason TEXT NOT NULL, \
           archived_at INTEGER NOT NULL \
         )",
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_project_migration_conflicts_video_id \
         ON project_migration_conflicts(video_id)",
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS project_document_quarantine ( \
           project_id TEXT PRIMARY KEY, document_json TEXT NOT NULL, \
           reason TEXT NOT NULL, archived_at INTEGER NOT NULL)",
    )
    .execute(&mut *transaction)
    .await?;

    // Legacy schemas indexed video_id without enforcing uniqueness. Keep the
    // latest row and archive every conflicting edit before enforcing the key.
    // rowid breaks timestamp ties by insertion order.
    let archived_at = now_secs() as i64;
    sqlx::query(
        "INSERT OR IGNORE INTO project_migration_conflicts \
           (project_id, name, video_id, video_json, edit_json, schema_version, \
            created_at, updated_at, reason, archived_at) \
         SELECT projects.id, projects.name, projects.video_id, projects.video_json, \
                projects.edit_json, projects.schema_version, projects.created_at, \
                projects.updated_at, 'duplicate-video-id-v1', ? \
         FROM projects \
         WHERE projects.document_json IS NULL AND EXISTS ( \
           SELECT 1 FROM projects AS preferred \
           WHERE preferred.video_id = projects.video_id \
             AND preferred.document_json IS NULL \
             AND ( \
               preferred.updated_at > projects.updated_at \
               OR (preferred.updated_at = projects.updated_at AND preferred.created_at > projects.created_at) \
               OR (preferred.updated_at = projects.updated_at AND preferred.created_at = projects.created_at AND preferred.rowid > projects.rowid) \
             ) \
         )",
    )
    .bind(archived_at)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "DELETE FROM projects \
         WHERE projects.document_json IS NULL AND EXISTS ( \
           SELECT 1 FROM projects AS preferred \
           WHERE preferred.video_id = projects.video_id \
             AND preferred.document_json IS NULL \
             AND ( \
               preferred.updated_at > projects.updated_at \
               OR (preferred.updated_at = projects.updated_at AND preferred.created_at > projects.created_at) \
               OR (preferred.updated_at = projects.updated_at AND preferred.created_at = projects.created_at AND preferred.rowid > projects.rowid) \
             ) \
         )",
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query("DROP INDEX IF EXISTS idx_projects_video_id")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DROP INDEX IF EXISTS ux_projects_video_id")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_projects_video_id ON projects(video_id)")
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "UPDATE projects SET updated_order = ( \
           SELECT COUNT(*) + 1 FROM projects AS older \
           WHERE older.updated_at < projects.updated_at \
              OR (older.updated_at = projects.updated_at AND older.created_at < projects.created_at) \
              OR (older.updated_at = projects.updated_at AND older.created_at = projects.created_at AND older.rowid < projects.rowid) \
         ) WHERE updated_order = 0",
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_projects_updated_order ON projects(updated_order DESC)",
    )
    .execute(&mut *transaction)
    .await?;

    // Convert each surviving legacy `{video_json, edit_json}` row into the
    // canonical v2 document. Existing documents are decoded and re-encoded so
    // unsupported forward versions fail explicitly and unknown v2 fields stay
    // intact. The pass is idempotent and also repairs a missing revision.
    let rows = sqlx::query(
        "SELECT id, name, video_id, video_json, edit_json, document_json, revision \
         FROM projects",
    )
    .fetch_all(&mut *transaction)
    .await?;
    for row in rows {
        let id: String = row.try_get("id")?;
        let name: String = row.try_get("name")?;
        let video_id: String = row.try_get("video_id")?;
        let revision: i64 = row.try_get("revision")?;
        let legacy_document = || -> Result<ProjectDocument> {
            let video = serde_json::from_str(&row.try_get::<String, _>("video_json")?)
                .with_context(|| format!("decode legacy project video {id}"))?;
            let edit = serde_json::from_str(&row.try_get::<String, _>("edit_json")?)
                .with_context(|| format!("decode legacy project edit {id}"))?;
            ProjectDocument::from_legacy(name.clone(), video_id.clone(), video, edit)
                .with_context(|| format!("backfill legacy project {id}"))
        };
        let document = match row.try_get::<Option<String>, _>("document_json")? {
            Some(json) if !json.trim().is_empty() => {
                let parsed = serde_json::from_str::<Value>(&json);
                if parsed
                    .as_ref()
                    .ok()
                    .and_then(|value| value.get("schemaVersion"))
                    .and_then(Value::as_u64)
                    .is_some_and(|version| version > u64::from(PROJECT_DOCUMENT_SCHEMA_VERSION))
                {
                    sqlx::query(
                        "INSERT OR REPLACE INTO project_document_quarantine \
                           (project_id, document_json, reason, archived_at) VALUES (?, ?, ?, ?)",
                    )
                    .bind(&id)
                    .bind(&json)
                    .bind("project schema is newer than this application")
                    .bind(archived_at)
                    .execute(&mut *transaction)
                    .await?;
                    continue;
                }
                match parsed
                    .map_err(anyhow::Error::from)
                    .and_then(|value| ProjectDocument::migrate(value).map_err(anyhow::Error::from))
                {
                    Ok(document) => document,
                    Err(error) => {
                        sqlx::query(
                            "INSERT OR REPLACE INTO project_document_quarantine \
                               (project_id, document_json, reason, archived_at) VALUES (?, ?, ?, ?)",
                        )
                        .bind(&id)
                        .bind(&json)
                        .bind(error.to_string())
                        .bind(archived_at)
                        .execute(&mut *transaction)
                        .await?;
                        legacy_document()?
                    }
                }
            }
            _ => legacy_document()?,
        };
        let document_json = serde_json::to_string(&document)?;
        sqlx::query(
            "UPDATE projects SET document_json = ?, schema_version = ?, revision = ? WHERE id = ?",
        )
        .bind(document_json)
        .bind(i64::from(PROJECT_DOCUMENT_SCHEMA_VERSION))
        .bind(revision.max(1))
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    }

    transaction.commit().await?;
    Ok(())
}
