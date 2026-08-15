//! SQLite-backed persistence for projects, durable jobs and the render cache.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::library::now_secs;
use crate::luts::{LutAsset, LUT_SCHEMA_VERSION};
use crate::model::{Job, JobStatus};

pub const MAX_LUT_ASSET_COUNT: i64 = 256;
pub const MAX_LUT_STORAGE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_LUT_LIST_RESULTS: i64 = 256;

#[derive(Debug)]
struct LutQuotaExceeded;

impl std::fmt::Display for LutQuotaExceeded {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("LUT storage quota exceeded")
    }
}

impl std::error::Error for LutQuotaExceeded {}

pub(crate) fn is_lut_quota_exceeded(error: &anyhow::Error) -> bool {
    error.downcast_ref::<LutQuotaExceeded>().is_some()
}

mod composition_projects;
mod library_metadata;
mod project_migration;

pub(crate) use composition_projects::valid_source_id as valid_composition_source_id;
// Public limits are part of the HTTP/integration-test contract as well as the
// SQLite adapter, so clients never guess a different project envelope size.
pub use composition_projects::{
    CompositionProject, COMPOSITION_PROJECT_MODE, COMPOSITION_PROJECT_SCHEMA_VERSION,
    MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES, MAX_COMPOSITION_PROJECT_NAME_BYTES,
    MAX_COMPOSITION_PROJECT_SOURCES,
};
pub use library_metadata::{
    normalize_library_tags, normalize_library_title, LibraryMetadata, LibraryMetadataChanges,
    MAX_LIBRARY_TAGS, MAX_LIBRARY_TAG_BYTES, MAX_LIBRARY_TAG_CHARS, MAX_LIBRARY_TITLE_BYTES,
    MAX_LIBRARY_TITLE_CHARS,
};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    video_id TEXT NOT NULL,
    video_json TEXT NOT NULL,
    edit_json TEXT NOT NULL,
    schema_version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    updated_order INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    result_json TEXT,
    error TEXT,
    stage TEXT,
    progress REAL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_jobs_updated_at ON jobs(updated_at DESC);
CREATE TABLE IF NOT EXISTS render_cache (
    cache_key TEXT PRIMARY KEY,
    output_json TEXT NOT NULL,
    filename TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_render_cache_created_at ON render_cache(created_at DESC);
CREATE TABLE IF NOT EXISTS color_luts (
    id TEXT PRIMARY KEY,
    sha256 TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    cube_size INTEGER NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_color_luts_created_at ON color_luts(created_at DESC);
";

/// A saved editing project: a clip plus its full edit recipe. `video` and `edit`
/// are opaque JSON blobs (the frontend's `VideoInfo` and `EditState`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub video_id: String,
    pub video: Value,
    pub edit: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// Open (creating if needed) `storage/app.db` and ensure the schema exists.
    pub async fn open(storage: &Path) -> Result<Db> {
        let opts = SqliteConnectOptions::new()
            .filename(storage.join("app.db"))
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;
        sqlx::query(SCHEMA).execute(&pool).await?;
        project_migration::migrate(&pool).await?;
        composition_projects::migrate(&pool).await?;
        library_metadata::migrate(&pool).await?;
        let db = Db { pool };
        crate::jobs::SqliteJobStore::new(db.clone())
            .migrate()
            .await?;
        crate::ports::SqliteMediaSearch::migrate(&db).await?;
        Ok(db)
    }

    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Create a consistent standalone SQLite image while the WAL database may
    /// remain open. The destination must not already exist.
    pub async fn snapshot_to(&self, destination: &Path) -> Result<()> {
        if destination.exists() {
            return Err(anyhow!("database snapshot destination already exists"));
        }
        let destination = destination
            .to_str()
            .ok_or_else(|| anyhow!("database snapshot path is not UTF-8"))?;
        sqlx::query("VACUUM INTO ?")
            .bind(destination)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Create or update (keyed by `video_id`) the project for a clip and return it.
    pub async fn upsert_project(
        &self,
        video_id: &str,
        name: &str,
        video: &Value,
        edit: &Value,
    ) -> Result<Project> {
        let now = now_secs() as i64;
        let video_str = serde_json::to_string(video)?;
        let edit_str = serde_json::to_string(edit)?;
        let id = Uuid::new_v4().to_string();
        let row = sqlx::query(
            "INSERT INTO projects \
               (id, name, video_id, video_json, edit_json, created_at, updated_at, updated_order) \
             VALUES (?, ?, ?, ?, ?, ?, ?, \
               COALESCE((SELECT MAX(updated_order) + 1 FROM projects), 1)) \
             ON CONFLICT(video_id) DO UPDATE SET \
               name = excluded.name, \
               video_json = excluded.video_json, \
               edit_json = excluded.edit_json, \
               updated_at = excluded.updated_at, \
               updated_order = excluded.updated_order \
             RETURNING id, name, video_id, video_json, edit_json, created_at, updated_at",
        )
        .bind(id)
        .bind(name)
        .bind(video_id)
        .bind(video_str)
        .bind(edit_str)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        row_to_project(row)
    }

    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let rows = sqlx::query(
            "SELECT id, name, video_id, video_json, edit_json, created_at, updated_at \
             FROM projects ORDER BY updated_order DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_project).collect()
    }

    pub async fn get_project(&self, id: &str) -> Result<Option<Project>> {
        let row = sqlx::query(
            "SELECT id, name, video_id, video_json, edit_json, created_at, updated_at \
             FROM projects WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_project).transpose()
    }

    pub async fn get_project_by_video(&self, video_id: &str) -> Result<Option<Project>> {
        let row = sqlx::query(
            "SELECT id, name, video_id, video_json, edit_json, created_at, updated_at \
             FROM projects WHERE video_id = ? ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(video_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_project).transpose()
    }

    /// Returns true if a row was deleted.
    pub async fn delete_project(&self, id: &str) -> Result<bool> {
        let res = sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Insert or update a job record. Called on creation and at terminal states;
    /// progress ticks are intentionally not persisted (they are ephemeral).
    pub async fn persist_job(&self, job: &Job) -> Result<()> {
        let now = now_secs() as i64;
        let result_str = match &job.result {
            Some(v) => Some(serde_json::to_string(v)?),
            None => None,
        };
        sqlx::query(
            "INSERT INTO jobs (id, status, result_json, error, stage, progress, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET \
               status = excluded.status, result_json = excluded.result_json, \
               error = excluded.error, stage = excluded.stage, \
               progress = excluded.progress, updated_at = excluded.updated_at",
        )
        .bind(&job.id)
        .bind(job.status.as_str())
        .bind(&result_str)
        .bind(&job.error)
        .bind(&job.stage)
        .bind(job.progress)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Load every persisted job (used once at startup to recover state).
    pub async fn load_jobs(&self) -> Result<Vec<Job>> {
        let rows = sqlx::query(
            "SELECT id, status, result_json, error, stage, progress FROM jobs ORDER BY updated_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_job).collect()
    }

    /// Load one job on demand. The in-memory job map is intentionally bounded,
    /// so status requests and outbox dispatch must be able to hydrate a miss.
    pub async fn load_job(&self, id: &str) -> Result<Option<Job>> {
        let row = sqlx::query(
            "SELECT id, status, result_json, error, stage, progress FROM jobs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_job).transpose()
    }

    /// Load only the most recently updated jobs. Used at startup so a long-lived
    /// install does not rebuild an unbounded in-memory job map.
    pub async fn load_recent_jobs(&self, limit: i64) -> Result<Vec<Job>> {
        let rows = sqlx::query(
            "SELECT id, status, result_json, error, stage, progress \
             FROM jobs ORDER BY updated_at DESC LIMIT ?",
        )
        .bind(limit.max(0))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_job).collect()
    }

    /// Look up a cached render by its content key, returning the stored result
    /// JSON and the output filename (the caller verifies the file still exists).
    pub async fn cache_get(&self, key: &str) -> Result<Option<(Value, String)>> {
        let row = sqlx::query("SELECT output_json, filename FROM render_cache WHERE cache_key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(r) => {
                let output: Value = serde_json::from_str(&r.try_get::<String, _>("output_json")?)?;
                Ok(Some((output, r.try_get("filename")?)))
            }
            None => Ok(None),
        }
    }

    /// Remember a finished render so an identical request can skip ffmpeg.
    pub async fn cache_put(&self, key: &str, output: &Value, filename: &str) -> Result<()> {
        let now = now_secs() as i64;
        sqlx::query(
            "INSERT INTO render_cache (cache_key, output_json, filename, created_at) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT(cache_key) DO UPDATE SET \
               output_json = excluded.output_json, filename = excluded.filename, \
               created_at = excluded.created_at",
        )
        .bind(key)
        .bind(serde_json::to_string(output)?)
        .bind(filename)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Remove a stale render-cache entry.
    pub async fn cache_delete(&self, key: &str) -> Result<bool> {
        let res = sqlx::query("DELETE FROM render_cache WHERE cache_key = ?")
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Remove all cache rows pointing at a deleted output file.
    pub async fn cache_delete_filename(&self, filename: &str) -> Result<u64> {
        let res = sqlx::query("DELETE FROM render_cache WHERE filename = ?")
            .bind(filename)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    /// Persist an immutable LUT, deduplicating canonical file contents by SHA-256.
    /// The bool is true when this call inserted the returned asset.
    pub async fn insert_or_get_lut(&self, asset: &LutAsset) -> Result<(LutAsset, bool)> {
        let cube_size = i64::from(asset.cube_size);
        let size_bytes =
            i64::try_from(asset.size_bytes).context("LUT size exceeds SQLite range")?;
        let created_at = i64::try_from(asset.created_at).context("LUT timestamp overflow")?;
        if asset.size_bytes > MAX_LUT_STORAGE_BYTES {
            return Err(LutQuotaExceeded.into());
        }
        let remaining_bytes = i64::try_from(MAX_LUT_STORAGE_BYTES - asset.size_bytes)
            .context("LUT quota exceeds SQLite range")?;
        let mut transaction = self.pool.begin().await?;
        let result = sqlx::query(
            "INSERT INTO color_luts \
             (id, sha256, filename, display_name, cube_size, size_bytes, created_at) \
             SELECT ?, ?, ?, ?, ?, ?, ? \
             WHERE (SELECT COUNT(*) FROM color_luts) < ? \
               AND (SELECT COALESCE(SUM(size_bytes), 0) FROM color_luts) <= ? \
             ON CONFLICT(sha256) DO NOTHING",
        )
        .bind(&asset.id)
        .bind(&asset.sha256)
        .bind(&asset.filename)
        .bind(&asset.name)
        .bind(cube_size)
        .bind(size_bytes)
        .bind(created_at)
        .bind(MAX_LUT_ASSET_COUNT)
        .bind(remaining_bytes)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() == 1 {
            transaction.commit().await?;
            return Ok((asset.clone(), true));
        }
        let existing = sqlx::query(
            "SELECT id, sha256, filename, display_name, cube_size, size_bytes, created_at \
             FROM color_luts WHERE sha256 = ?",
        )
        .bind(&asset.sha256)
        .fetch_optional(&mut *transaction)
        .await?
        .map(row_to_lut)
        .transpose()?;
        transaction.commit().await?;
        match existing {
            Some(existing) => Ok((existing, false)),
            None => Err(LutQuotaExceeded.into()),
        }
    }

    pub async fn get_lut(&self, id: &str) -> Result<Option<LutAsset>> {
        let row = sqlx::query(
            "SELECT id, sha256, filename, display_name, cube_size, size_bytes, created_at \
             FROM color_luts WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_lut).transpose()
    }

    pub async fn get_lut_by_sha256(&self, sha256: &str) -> Result<Option<LutAsset>> {
        let row = sqlx::query(
            "SELECT id, sha256, filename, display_name, cube_size, size_bytes, created_at \
             FROM color_luts WHERE sha256 = ?",
        )
        .bind(sha256)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_lut).transpose()
    }

    pub async fn list_luts(&self) -> Result<Vec<LutAsset>> {
        let rows = sqlx::query(
            "SELECT id, sha256, filename, display_name, cube_size, size_bytes, created_at \
             FROM color_luts ORDER BY created_at DESC, id LIMIT ?",
        )
        .bind(MAX_LUT_LIST_RESULTS)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_lut).collect()
    }
}

fn row_to_job(row: SqliteRow) -> Result<Job> {
    let result = match row.try_get::<Option<String>, _>("result_json")? {
        Some(s) => Some(serde_json::from_str(&s)?),
        None => None,
    };
    Ok(Job {
        id: row.try_get("id")?,
        status: JobStatus::from_token(&row.try_get::<String, _>("status")?)?,
        result,
        error: row.try_get("error")?,
        progress: row.try_get("progress")?,
        stage: row.try_get("stage")?,
    })
}

fn row_to_project(row: SqliteRow) -> Result<Project> {
    Ok(Project {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        video_id: row.try_get("video_id")?,
        video: serde_json::from_str(&row.try_get::<String, _>("video_json")?)?,
        edit: serde_json::from_str(&row.try_get::<String, _>("edit_json")?)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_lut(row: SqliteRow) -> Result<LutAsset> {
    let cube_size = u32::try_from(row.try_get::<i64, _>("cube_size")?)
        .context("invalid LUT cube size in database")?;
    let size_bytes = u64::try_from(row.try_get::<i64, _>("size_bytes")?)
        .context("invalid LUT byte size in database")?;
    let created_at = u64::try_from(row.try_get::<i64, _>("created_at")?)
        .context("invalid LUT timestamp in database")?;
    Ok(LutAsset {
        schema_version: LUT_SCHEMA_VERSION,
        id: row.try_get("id")?,
        name: row.try_get("display_name")?,
        kind: "cube3d".into(),
        cube_size,
        size_bytes,
        sha256: row.try_get("sha256")?,
        created_at,
        filename: row.try_get("filename")?,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use serde_json::json;
    use tokio::sync::Barrier;

    async fn db() -> (Db, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).await.unwrap();
        (db, dir)
    }

    #[tokio::test]
    async fn upsert_is_keyed_by_video_id() {
        let (db, _d) = db().await;
        let p1 = db
            .upsert_project(
                "v1",
                "first",
                &json!({"id":"v1"}),
                &json!({"filter":"sepia"}),
            )
            .await
            .unwrap();
        sqlx::query("UPDATE projects SET created_at = 7 WHERE id = ?")
            .bind(&p1.id)
            .execute(db.pool())
            .await
            .unwrap();
        let p2 = db
            .upsert_project(
                "v1",
                "second",
                &json!({"id":"v1"}),
                &json!({"filter":"warm"}),
            )
            .await
            .unwrap();
        assert_eq!(p1.id, p2.id, "same video keeps the same project");
        assert_eq!(p2.created_at, 7, "updates preserve the creation time");
        assert_eq!(db.list_projects().await.unwrap().len(), 1);
        assert_eq!(p2.name, "second");
        assert_eq!(p2.video, json!({"id":"v1"}));
        assert_eq!(p2.edit["filter"], "warm");
        assert!(p2.updated_at >= p1.updated_at);
    }

    #[tokio::test]
    async fn project_list_uses_write_order_when_timestamps_tie() {
        let (db, _d) = db().await;
        for video_id in ["first", "second", "first", "second"] {
            db.upsert_project(video_id, video_id, &json!({"id": video_id}), &json!({}))
                .await
                .unwrap();
        }
        sqlx::query("UPDATE projects SET updated_at = 1")
            .execute(db.pool())
            .await
            .unwrap();

        let projects = db.list_projects().await.unwrap();
        assert_eq!(projects[0].video_id, "second");
        assert_eq!(projects[1].video_id, "first");
        let orders: Vec<i64> =
            sqlx::query_scalar("SELECT updated_order FROM projects ORDER BY updated_order DESC")
                .fetch_all(db.pool())
                .await
                .unwrap();
        assert!(orders[0] > orders[1]);
    }

    #[tokio::test]
    async fn concurrent_upserts_create_one_project_and_return_each_write() {
        const WRITERS: usize = 24;

        let (db, _d) = db().await;
        let barrier = Arc::new(Barrier::new(WRITERS));
        let handles: Vec<_> = (0..WRITERS)
            .map(|writer| {
                let db = db.clone();
                let barrier = barrier.clone();
                tokio::spawn(async move {
                    let name = format!("writer-{writer}");
                    barrier.wait().await;
                    db.upsert_project(
                        "shared-video",
                        &name,
                        &json!({"writer": writer}),
                        &json!({"revision": writer}),
                    )
                    .await
                })
            })
            .collect();

        let mut projects = Vec::with_capacity(WRITERS);
        for (writer, handle) in handles.into_iter().enumerate() {
            let project = handle.await.unwrap().unwrap();
            assert_eq!(project.name, format!("writer-{writer}"));
            assert_eq!(project.video["writer"], json!(writer));
            assert_eq!(project.edit["revision"], json!(writer));
            projects.push(project);
        }

        let id = projects[0].id.as_str();
        let created_at = projects[0].created_at;
        assert!(projects.iter().all(|project| project.id == id));
        assert!(projects
            .iter()
            .all(|project| project.created_at == created_at));

        let stored = db
            .get_project_by_video("shared-video")
            .await
            .unwrap()
            .unwrap();
        let final_writer = stored.video["writer"].as_u64().unwrap();
        assert_eq!(stored.name, format!("writer-{final_writer}"));
        assert_eq!(stored.edit["revision"], json!(final_writer));
        assert_eq!(db.list_projects().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn opening_legacy_database_deduplicates_projects_and_enforces_uniqueness() {
        let dir = tempfile::tempdir().unwrap();
        let options = SqliteConnectOptions::new()
            .filename(dir.path().join("app.db"))
            .create_if_missing(true);
        let legacy = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE projects ( \
               id TEXT PRIMARY KEY, \
               name TEXT NOT NULL, \
               video_id TEXT NOT NULL, \
               video_json TEXT NOT NULL, \
               edit_json TEXT NOT NULL, \
               schema_version INTEGER NOT NULL DEFAULT 1, \
               created_at INTEGER NOT NULL, \
               updated_at INTEGER NOT NULL \
             )",
        )
        .execute(&legacy)
        .await
        .unwrap();
        sqlx::query("CREATE INDEX idx_projects_video_id ON projects(video_id)")
            .execute(&legacy)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO projects
                 (id, name, video_id, video_json, edit_json, created_at, updated_at)
               VALUES
                 ('older', 'older project', 'legacy-video', '{"revision":1}', '{"filter":"old"}', 10, 20),
                 ('newer', 'newer project', 'legacy-video', '{"revision":2}', '{"filter":"new"}', 11, 30),
                 ('z-first', 'first tied project', 'tied-video', '{"revision":1}', '{}', 50, 50),
                 ('a-second', 'second tied project', 'tied-video', '{"revision":2}', '{}', 50, 50),
                 ('late-old', 'late insert with old timestamp', 'old-video', '{}', '{}', 1, 5)"#,
        )
        .execute(&legacy)
        .await
        .unwrap();
        legacy.close().await;

        let db = Db::open(dir.path()).await.unwrap();
        let project = db
            .get_project_by_video("legacy-video")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(project.id, "newer");
        assert_eq!(project.name, "newer project");
        assert_eq!(project.video["revision"], 2);
        assert_eq!(project.edit["filter"], "new");
        let tied = db
            .get_project_by_video("tied-video")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(tied.id, "a-second", "insertion order breaks timestamp ties");
        let projects = db.list_projects().await.unwrap();
        assert_eq!(projects.len(), 3);
        assert_eq!(
            projects
                .iter()
                .map(|project| project.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a-second", "newer", "late-old"]
        );

        let archived = sqlx::query(
            "SELECT name, video_json, edit_json, reason \
             FROM project_migration_conflicts WHERE project_id = ?",
        )
        .bind("older")
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(
            archived.try_get::<String, _>("name").unwrap(),
            "older project"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&archived.try_get::<String, _>("video_json").unwrap())
                .unwrap()["revision"],
            1
        );
        assert_eq!(
            serde_json::from_str::<Value>(&archived.try_get::<String, _>("edit_json").unwrap())
                .unwrap()["filter"],
            "old"
        );
        assert_eq!(
            archived.try_get::<String, _>("reason").unwrap(),
            "duplicate-video-id-v1"
        );
        let tied_archived: String = sqlx::query_scalar(
            "SELECT project_id FROM project_migration_conflicts WHERE video_id = 'tied-video'",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(tied_archived, "z-first");

        let error = sqlx::query(
            r#"INSERT INTO projects
                 (id, name, video_id, video_json, edit_json, created_at, updated_at)
               VALUES ('duplicate', 'duplicate', 'legacy-video', '{}', '{}', 40, 40)"#,
        )
        .execute(db.pool())
        .await
        .unwrap_err();
        match error {
            sqlx::Error::Database(error) => assert!(error.is_unique_violation()),
            error => panic!("expected a uniqueness error, got {error}"),
        }
    }

    #[tokio::test]
    async fn get_by_video_and_delete() {
        let (db, _d) = db().await;
        let p = db
            .upsert_project("vid", "n", &json!({"id":"vid"}), &json!({}))
            .await
            .unwrap();
        assert_eq!(
            db.get_project_by_video("vid").await.unwrap().unwrap().id,
            p.id
        );
        assert!(db.get_project_by_video("missing").await.unwrap().is_none());
        assert!(db.delete_project(&p.id).await.unwrap());
        assert!(!db.delete_project(&p.id).await.unwrap());
        assert!(db.get_project(&p.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn jobs_persist_and_load() {
        use crate::model::Job;
        let (db, _d) = db().await;
        let mut j = Job::pending("j1".into());
        j.status = JobStatus::Done;
        j.result = Some(json!({ "url": "/files/outputs/x.mp4" }));
        j.progress = Some(100.0);
        db.persist_job(&j).await.unwrap();
        let loaded = db.load_jobs().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "j1");
        assert_eq!(loaded[0].status, JobStatus::Done);
        assert_eq!(
            loaded[0].result.as_ref().unwrap()["url"],
            "/files/outputs/x.mp4"
        );
        assert_eq!(loaded[0].progress, Some(100.0));
        assert_eq!(db.load_job("j1").await.unwrap(), Some(j));
        assert!(db.load_job("missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn corrupt_job_status_is_rejected_instead_of_becoming_pending() {
        let (db, _d) = db().await;
        sqlx::query("INSERT INTO jobs (id, status, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind("corrupt")
            .bind("not-a-status")
            .bind(1_i64)
            .bind(1_i64)
            .execute(db.pool())
            .await
            .unwrap();

        let error = db.load_job("corrupt").await.unwrap_err();
        assert!(
            error.to_string().contains("unknown job status token"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn corrupt_job_status_is_quarantined_on_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let db = Db::open(dir.path()).await.unwrap();
            sqlx::query(
                "INSERT INTO jobs \
                   (id, status, result_json, error, stage, progress, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind("corrupt")
            .bind("future-status")
            .bind(r#"{"unexpected":true}"#)
            .bind("original error")
            .bind("future-stage")
            .bind(27.0_f64)
            .bind(1_i64)
            .bind(2_i64)
            .execute(db.pool())
            .await
            .unwrap();
            db.pool().close().await;
        }

        let reopened = Db::open(dir.path()).await.unwrap();
        let job = reopened.load_job("corrupt").await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Interrupted);
        assert_eq!(job.result, None);
        assert_eq!(job.stage, None);
        assert!(job
            .error
            .as_deref()
            .is_some_and(|error| error.contains("quarantined")));

        let archived = sqlx::query(
            "SELECT original_status, result_json, error, stage, progress \
             FROM job_status_quarantine WHERE job_id = ?",
        )
        .bind("corrupt")
        .fetch_one(reopened.pool())
        .await
        .unwrap();
        assert_eq!(
            archived.try_get::<String, _>("original_status").unwrap(),
            "future-status"
        );
        assert_eq!(
            archived
                .try_get::<Option<String>, _>("result_json")
                .unwrap(),
            Some(r#"{"unexpected":true}"#.into())
        );
        assert_eq!(
            archived.try_get::<Option<String>, _>("error").unwrap(),
            Some("original error".into())
        );
        assert_eq!(
            archived.try_get::<Option<String>, _>("stage").unwrap(),
            Some("future-stage".into())
        );
        assert_eq!(
            archived.try_get::<Option<f64>, _>("progress").unwrap(),
            Some(27.0)
        );

        let history = crate::jobs::SqliteJobStore::new(reopened)
            .event_history("corrupt")
            .await
            .unwrap();
        assert!(matches!(
            history.first().map(|entry| &entry.event),
            Some(crate::jobs::JobEvent::Created)
        ));
        assert!(matches!(
            history.last().map(|entry| &entry.event),
            Some(crate::jobs::JobEvent::Interrupted { .. })
        ));
    }

    #[tokio::test]
    async fn load_recent_jobs_respects_limit() {
        use crate::model::Job;
        let (db, _d) = db().await;
        for i in 0..10 {
            db.persist_job(&Job::pending(format!("j{i}")))
                .await
                .unwrap();
        }
        assert_eq!(db.load_recent_jobs(3).await.unwrap().len(), 3);
        assert!(db.load_recent_jobs(0).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn render_cache_roundtrip() {
        let (db, _d) = db().await;
        assert!(db.cache_get("k1").await.unwrap().is_none());
        let out = json!({ "id": "o1", "filename": "o1.mp4" });
        db.cache_put("k1", &out, "o1.mp4").await.unwrap();
        db.cache_put("k2", &out, "o1.mp4").await.unwrap();
        let (got, filename) = db.cache_get("k1").await.unwrap().unwrap();
        assert_eq!(got["id"], "o1");
        assert_eq!(filename, "o1.mp4");
        assert!(db.cache_delete("k1").await.unwrap());
        assert!(db.cache_get("k1").await.unwrap().is_none());
        assert!(!db.cache_delete("k1").await.unwrap());
        assert_eq!(db.cache_delete_filename("o1.mp4").await.unwrap(), 1);
        assert!(db.cache_get("k2").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn projects_persist_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let db = Db::open(dir.path()).await.unwrap();
            db.upsert_project("v1", "n", &json!({"id":"v1"}), &json!({"filter":"sepia"}))
                .await
                .unwrap();
        }
        // Reopening the same file restores the data (the point of SQLite).
        let db2 = Db::open(dir.path()).await.unwrap();
        let p = db2.get_project_by_video("v1").await.unwrap().unwrap();
        assert_eq!(p.edit["filter"], "sepia");
    }

    #[tokio::test]
    async fn immutable_luts_roundtrip_and_deduplicate_by_hash() {
        let (db, _d) = db().await;
        let first = LutAsset::new(
            "first".into(),
            "First".into(),
            "first.cube".into(),
            2,
            100,
            "abc".into(),
            10,
        );
        let (inserted, created) = db.insert_or_get_lut(&first).await.unwrap();
        assert!(created);
        assert_eq!(inserted, first);

        let duplicate = LutAsset::new(
            "second".into(),
            "Second".into(),
            "second.cube".into(),
            2,
            100,
            "abc".into(),
            20,
        );
        let (resolved, created) = db.insert_or_get_lut(&duplicate).await.unwrap();
        assert!(!created);
        assert_eq!(resolved.id, "first");
        assert_eq!(db.get_lut("first").await.unwrap(), Some(first));
        assert!(db.get_lut("second").await.unwrap().is_none());
        assert_eq!(db.list_luts().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn lut_quota_is_atomic_and_still_allows_deduplication() {
        let (db, _d) = db().await;
        for index in 0..MAX_LUT_ASSET_COUNT {
            let asset = LutAsset::new(
                format!("id-{index}"),
                format!("LUT {index}"),
                format!("id-{index}.cube"),
                2,
                1,
                format!("sha-{index}"),
                index as u64,
            );
            assert!(db.insert_or_get_lut(&asset).await.unwrap().1);
        }

        let overflow = LutAsset::new(
            "overflow".into(),
            "Overflow".into(),
            "overflow.cube".into(),
            2,
            1,
            "new-sha".into(),
            999,
        );
        let error = db.insert_or_get_lut(&overflow).await.unwrap_err();
        assert!(is_lut_quota_exceeded(&error));

        let duplicate = LutAsset::new(
            "duplicate".into(),
            "Duplicate".into(),
            "duplicate.cube".into(),
            2,
            1,
            "sha-0".into(),
            1_000,
        );
        let (resolved, created) = db.insert_or_get_lut(&duplicate).await.unwrap();
        assert!(!created);
        assert_eq!(resolved.id, "id-0");
        assert_eq!(
            db.list_luts().await.unwrap().len(),
            MAX_LUT_LIST_RESULTS as usize
        );
    }
}
