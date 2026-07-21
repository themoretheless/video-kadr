//! One-time repair of the legacy project key before uniqueness is enforced.

use anyhow::Result;
use sqlx::SqlitePool;

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
         WHERE EXISTS ( \
           SELECT 1 FROM projects AS preferred \
           WHERE preferred.video_id = projects.video_id \
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
         WHERE EXISTS ( \
           SELECT 1 FROM projects AS preferred \
           WHERE preferred.video_id = projects.video_id \
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
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS ux_projects_video_id ON projects(video_id)")
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

    transaction.commit().await?;
    Ok(())
}
