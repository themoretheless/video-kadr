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

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    video_id TEXT NOT NULL,
    video_json TEXT NOT NULL,
    edit_json TEXT NOT NULL,
    schema_version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_projects_video_id ON projects(video_id);
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
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;
        sqlx::query(SCHEMA).execute(&pool).await?;
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
        let existing: Option<String> =
            sqlx::query_scalar("SELECT id FROM projects WHERE video_id = ?")
                .bind(video_id)
                .fetch_optional(&self.pool)
                .await?;
        let id = match existing {
            Some(id) => {
                sqlx::query(
                    "UPDATE projects SET name = ?, video_json = ?, edit_json = ?, updated_at = ? WHERE id = ?",
                )
                .bind(name)
                .bind(&video_str)
                .bind(&edit_str)
                .bind(now)
                .bind(&id)
                .execute(&self.pool)
                .await?;
                id
            }
            None => {
                let id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO projects (id, name, video_id, video_json, edit_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(name)
                .bind(video_id)
                .bind(&video_str)
                .bind(&edit_str)
                .bind(now)
                .bind(now)
                .execute(&self.pool)
                .await?;
                id
            }
        };
        self.get_project(&id)
            .await?
            .ok_or_else(|| anyhow!("project vanished right after upsert"))
    }

    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let rows = sqlx::query(
            "SELECT id, name, video_id, video_json, edit_json, created_at, updated_at \
             FROM projects ORDER BY updated_at DESC",
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
        status: JobStatus::from_token(&row.try_get::<String, _>("status")?),
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
    use super::*;
    use serde_json::json;

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
        assert_eq!(db.list_projects().await.unwrap().len(), 1);
        assert_eq!(p2.edit["filter"], "warm");
        assert!(p2.updated_at >= p1.updated_at);
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
