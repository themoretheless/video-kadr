//! Durable, artifact-idempotent scheduler for derived media work.
//!
//! This is deliberately separate from the import/edit outbox.  A task is the
//! producer of one content-addressed artifact and may be referenced by several
//! submitted graphs.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::analysis::proxy::{proxy_key, ProxyProfile, SourceIdentity, FFMPEG_PROXY_COMPATIBILITY};
use crate::db::Db;
use crate::domain::artifact_graph::Fingerprint;
use crate::library::now_secs;

pub const MAX_GRAPH_NODES: usize = 256;
pub const MAX_GRAPH_EDGES: usize = 1024;
pub const MAX_GRAPH_DEPTH: usize = 32;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS derived_graphs (
  graph_id TEXT PRIMARY KEY, project_id TEXT NOT NULL, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS derived_tasks (
  task_id TEXT PRIMARY KEY,
  artifact_key TEXT NOT NULL UNIQUE,
  kind TEXT NOT NULL,
  project_id TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('pending','running','succeeded','failed','cancelled','blocked')),
  priority INTEGER NOT NULL,
  priority_revision INTEGER NOT NULL DEFAULT 0,
  enqueue_sequence INTEGER NOT NULL DEFAULT 0,
  enqueued_at INTEGER NOT NULL,
  available_at INTEGER NOT NULL,
  generation INTEGER NOT NULL DEFAULT 0,
  attempt INTEGER NOT NULL DEFAULT 0,
  max_attempts INTEGER NOT NULL DEFAULT 3,
  lease_owner TEXT,
  lease_until INTEGER,
  result_json TEXT,
  error TEXT,
  updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_derived_ready
  ON derived_tasks(state, available_at, priority, enqueued_at);
CREATE TABLE IF NOT EXISTS derived_graph_tasks (
  graph_id TEXT NOT NULL, task_id TEXT NOT NULL,
  PRIMARY KEY(graph_id, task_id),
  FOREIGN KEY(graph_id) REFERENCES derived_graphs(graph_id),
  FOREIGN KEY(task_id) REFERENCES derived_tasks(task_id)
);
CREATE TABLE IF NOT EXISTS derived_dependencies (
  task_id TEXT NOT NULL, depends_on TEXT NOT NULL,
  PRIMARY KEY(task_id, depends_on),
  CHECK(task_id <> depends_on),
  FOREIGN KEY(task_id) REFERENCES derived_tasks(task_id),
  FOREIGN KEY(depends_on) REFERENCES derived_tasks(task_id)
);
CREATE INDEX IF NOT EXISTS idx_derived_dependencies_parent
  ON derived_dependencies(depends_on, task_id);
CREATE TABLE IF NOT EXISTS derived_scheduler (
  singleton INTEGER PRIMARY KEY CHECK(singleton = 1), claim_sequence INTEGER NOT NULL, enqueue_sequence INTEGER NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO derived_scheduler(singleton, claim_sequence) VALUES(1, 0);
CREATE TABLE IF NOT EXISTS derived_project_fairness (
  project_id TEXT PRIMARY KEY, last_claim_sequence INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS derived_cancelled_projects (
  project_id TEXT PRIMARY KEY, cancelled_at INTEGER NOT NULL
);
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DerivedTaskKind {
    Probe,
    Proxy,
    Waveform,
    Thumbnails,
    Conform,
}

impl DerivedTaskKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Probe => "probe",
            Self::Proxy => "proxy",
            Self::Waveform => "waveform",
            Self::Thumbnails => "thumbnails",
            Self::Conform => "conform",
        }
    }
    fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "probe" => Self::Probe,
            "proxy" => Self::Proxy,
            "waveform" => Self::Waveform,
            "thumbnails" => Self::Thumbnails,
            "conform" => Self::Conform,
            _ => return Err(anyhow!("unknown derived task kind: {value}")),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DerivedTaskState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Blocked,
}

impl DerivedTaskState {
    fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "pending" => Self::Pending,
            "running" => Self::Running,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "blocked" => Self::Blocked,
            _ => return Err(anyhow!("unknown derived task state: {value}")),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DerivedTaskSpec {
    /// Stable name used by dependency lists within this submission.
    pub key: String,
    /// Content-addressed identity including source, normalized parameters and tool version.
    pub artifact_key: String,
    pub kind: DerivedTaskKind,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub priority: i64,
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProxyTaskPayload {
    source_id: String,
    source_fingerprint: String,
    profile: ProxyProfile,
}

fn default_max_attempts() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DerivedGraphSpec {
    pub project_id: String,
    pub tasks: Vec<DerivedTaskSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueuedGraph {
    pub graph_id: String,
    pub tasks: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedDerivedTask {
    pub task_id: String,
    pub artifact_key: String,
    pub kind: DerivedTaskKind,
    pub project_id: String,
    pub payload: Value,
    pub generation: u64,
    pub attempt: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedTaskRecord {
    pub task_id: String,
    pub artifact_key: String,
    pub kind: DerivedTaskKind,
    pub project_id: String,
    pub consumer_project_ids: Vec<String>,
    pub state: DerivedTaskState,
    pub priority: i64,
    pub priority_revision: u64,
    pub generation: u64,
    pub attempt: u32,
    pub enqueued_at: i64,
    pub enqueue_sequence: i64,
    pub available_at: i64,
    pub lease_until: Option<i64>,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriorityConflict {
    pub actual_revision: u64,
}

#[derive(Clone)]
pub struct DerivedJobStore {
    db: Db,
}

impl DerivedJobStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn migrate(&self) -> Result<()> {
        sqlx::query(SCHEMA).execute(self.db.pool()).await?;
        let task_columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('derived_tasks')")
                .fetch_all(self.db.pool())
                .await?;
        if !task_columns
            .iter()
            .any(|column| column == "enqueue_sequence")
        {
            sqlx::query(
                "ALTER TABLE derived_tasks ADD COLUMN enqueue_sequence INTEGER NOT NULL DEFAULT 0",
            )
            .execute(self.db.pool())
            .await?;
            sqlx::query("UPDATE derived_tasks SET enqueue_sequence=rowid WHERE enqueue_sequence=0")
                .execute(self.db.pool())
                .await?;
        }
        let scheduler_columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('derived_scheduler')")
                .fetch_all(self.db.pool())
                .await?;
        if !scheduler_columns
            .iter()
            .any(|column| column == "enqueue_sequence")
        {
            sqlx::query("ALTER TABLE derived_scheduler ADD COLUMN enqueue_sequence INTEGER NOT NULL DEFAULT 0").execute(self.db.pool()).await?;
        }
        sqlx::query("UPDATE derived_scheduler SET enqueue_sequence=MAX(enqueue_sequence,(SELECT COALESCE(MAX(enqueue_sequence),0) FROM derived_tasks)) WHERE singleton=1").execute(self.db.pool()).await?;
        Ok(())
    }

    pub async fn enqueue_graph(&self, spec: &DerivedGraphSpec) -> Result<EnqueuedGraph> {
        validate_graph(spec)?;
        let now = now_secs() as i64;
        let graph_id = Uuid::new_v4().to_string();
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        if spec.project_id.starts_with("media:") {
            let cancelled: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM derived_cancelled_projects WHERE project_id=?",
            )
            .bind(&spec.project_id)
            .fetch_one(&mut *tx)
            .await?;
            ensure!(
                cancelled == 0,
                "automatic source analysis was explicitly cancelled"
            );
        }
        sqlx::query("INSERT INTO derived_graphs(graph_id, project_id, created_at) VALUES(?,?,?)")
            .bind(&graph_id)
            .bind(&spec.project_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;

        let mut ids = BTreeMap::new();
        let mut created = BTreeSet::new();
        for task in &spec.tasks {
            let proposed = Uuid::new_v4().to_string();
            let payload = serde_json::to_string(&task.payload)?;
            let enqueue_sequence: i64 = sqlx::query_scalar("UPDATE derived_scheduler SET enqueue_sequence=enqueue_sequence+1 WHERE singleton=1 RETURNING enqueue_sequence").fetch_one(&mut *tx).await?;
            let inserted = sqlx::query("INSERT OR IGNORE INTO derived_tasks(task_id,artifact_key,kind,project_id,payload_json,state,priority,enqueue_sequence,enqueued_at,available_at,max_attempts,updated_at) VALUES(?,?,?,?,?,'pending',?,?,?,?,?,?)")
                .bind(&proposed).bind(&task.artifact_key).bind(task.kind.as_str()).bind(&spec.project_id)
                .bind(&payload).bind(task.priority).bind(enqueue_sequence).bind(now).bind(now)
                .bind(i64::from(task.max_attempts)).bind(now).execute(&mut *tx).await?;
            let row = sqlx::query(
                "SELECT task_id,kind,payload_json,state FROM derived_tasks WHERE artifact_key=?",
            )
            .bind(&task.artifact_key)
            .fetch_one(&mut *tx)
            .await?;
            let task_id: String = row.try_get("task_id")?;
            ensure!(
                row.try_get::<String, _>("kind")? == task.kind.as_str(),
                "artifact key reused with a different task kind"
            );
            let persisted_payload: Value = serde_json::from_str(row.try_get("payload_json")?)?;
            ensure!(
                semantic_payload(&persisted_payload) == semantic_payload(&task.payload),
                "artifact key reused with different semantic payload"
            );
            let persisted_state: String = row.try_get("state")?;
            if inserted.rows_affected() == 0
                && (row.try_get::<String, _>("payload_json")? != payload
                    || matches!(persisted_state.as_str(), "failed" | "cancelled"))
            {
                sqlx::query("UPDATE derived_tasks SET payload_json=?,state=CASE WHEN state IN ('failed','cancelled') THEN 'pending' ELSE state END,attempt=CASE WHEN state IN ('failed','cancelled') THEN 0 ELSE attempt END,error=CASE WHEN state IN ('failed','cancelled') THEN NULL ELSE error END,updated_at=? WHERE task_id=? AND state<>'running'")
                    .bind(&payload).bind(now).bind(&task_id).execute(&mut *tx).await?;
            }
            if inserted.rows_affected() == 1 {
                created.insert(task_id.clone());
            }
            ids.insert(task.key.clone(), task_id.clone());
            sqlx::query("INSERT INTO derived_graph_tasks(graph_id,task_id) VALUES(?,?)")
                .bind(&graph_id)
                .bind(task_id)
                .execute(&mut *tx)
                .await?;
        }
        for task in &spec.tasks {
            let task_id = &ids[&task.key];
            let expected: BTreeSet<String> = task
                .dependencies
                .iter()
                .map(|key| ids[key].clone())
                .collect();
            if created.contains(task_id) {
                for dependency in &expected {
                    sqlx::query("INSERT INTO derived_dependencies(task_id,depends_on) VALUES(?,?)")
                        .bind(task_id)
                        .bind(dependency)
                        .execute(&mut *tx)
                        .await?;
                }
            } else {
                let persisted: BTreeSet<String> = sqlx::query_scalar(
                    "SELECT depends_on FROM derived_dependencies WHERE task_id=?",
                )
                .bind(task_id)
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .collect();
                ensure!(
                    persisted == expected,
                    "artifact key reused with a different dependency set"
                );
            }
        }
        tx.commit().await?;
        Ok(EnqueuedGraph {
            graph_id,
            tasks: ids,
        })
    }

    /// CAS priority update. Returns the new revision or the durable actual revision.
    pub async fn set_priority(
        &self,
        task_id: &str,
        priority: i64,
        expected_revision: u64,
    ) -> Result<std::result::Result<u64, PriorityConflict>> {
        ensure!(
            (-100..=100).contains(&priority),
            "priority must be within -100..=100"
        );
        let result = sqlx::query("UPDATE derived_tasks SET priority=?,priority_revision=priority_revision+1,updated_at=? WHERE task_id=? AND priority_revision=? AND state IN ('pending','blocked')")
            .bind(priority).bind(now_secs() as i64).bind(task_id).bind(expected_revision as i64)
            .execute(self.db.pool()).await?;
        if result.rows_affected() == 1 {
            return Ok(Ok(expected_revision + 1));
        }
        let actual: Option<i64> =
            sqlx::query_scalar("SELECT priority_revision FROM derived_tasks WHERE task_id=?")
                .bind(task_id)
                .fetch_optional(self.db.pool())
                .await?;
        actual
            .map(|value| {
                Err(PriorityConflict {
                    actual_revision: value as u64,
                })
            })
            .ok_or_else(|| anyhow!("derived task not found"))
    }

    /// Atomically chooses and leases one dependency-ready task. Aging is one
    /// priority point per minute; the least-recently served project wins ties.
    pub async fn claim_next(
        &self,
        worker: &str,
        lease_seconds: i64,
    ) -> Result<Option<ClaimedDerivedTask>> {
        ensure!(lease_seconds > 0, "lease must be positive");
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query(
            "WITH RECURSIVE donation(task_id,donated) AS ( \
               SELECT task_id,priority FROM derived_tasks WHERE state IN ('pending','blocked') \
               UNION ALL SELECT d.depends_on,x.donated FROM derived_dependencies d JOIN donation x ON x.task_id=d.task_id \
             ), effective AS (SELECT task_id,MAX(donated) donated FROM donation GROUP BY task_id) \
             SELECT t.task_id,t.artifact_key,t.kind,t.project_id,t.payload_json,t.generation,t.attempt \
             FROM derived_tasks t LEFT JOIN derived_project_fairness f ON f.project_id=t.project_id \
             LEFT JOIN effective e ON e.task_id=t.task_id \
             WHERE t.state='pending' AND t.available_at<=? AND NOT EXISTS ( \
               SELECT 1 FROM derived_dependencies d JOIN derived_tasks p ON p.task_id=d.depends_on \
               WHERE d.task_id=t.task_id AND p.state<>'succeeded') \
             ORDER BY COALESCE(f.last_claim_sequence,0) ASC, \
                      (MAX(t.priority,COALESCE(e.donated,t.priority)) + MIN(1000, (? - t.enqueued_at)/60)) DESC, \
                      t.enqueue_sequence ASC, t.task_id ASC LIMIT 1")
            .bind(now).bind(now).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let task_id: String = row.try_get("task_id")?;
        let old_generation: i64 = row.try_get("generation")?;
        let update = sqlx::query("UPDATE derived_tasks SET state='running',generation=generation+1,attempt=attempt+1,lease_owner=?,lease_until=?,updated_at=? WHERE task_id=? AND state='pending' AND generation=?")
            .bind(worker).bind(now.saturating_add(lease_seconds)).bind(now).bind(&task_id).bind(old_generation)
            .execute(&mut *tx).await?;
        ensure!(
            update.rows_affected() == 1,
            "derived claim lost generation race"
        );
        let sequence: i64 = sqlx::query_scalar("UPDATE derived_scheduler SET claim_sequence=claim_sequence+1 WHERE singleton=1 RETURNING claim_sequence")
            .fetch_one(&mut *tx).await?;
        let project_id: String = row.try_get("project_id")?;
        sqlx::query("INSERT INTO derived_project_fairness(project_id,last_claim_sequence) VALUES(?,?) ON CONFLICT(project_id) DO UPDATE SET last_claim_sequence=excluded.last_claim_sequence")
            .bind(&project_id).bind(sequence).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(ClaimedDerivedTask {
            task_id,
            artifact_key: row.try_get("artifact_key")?,
            kind: DerivedTaskKind::parse(row.try_get("kind")?)?,
            project_id,
            payload: serde_json::from_str(row.try_get("payload_json")?)?,
            generation: (old_generation + 1) as u64,
            attempt: (row.try_get::<i64, _>("attempt")? + 1) as u32,
        }))
    }

    pub async fn renew_lease(
        &self,
        task_id: &str,
        generation: u64,
        worker: &str,
        lease_seconds: i64,
    ) -> Result<bool> {
        let now = now_secs() as i64;
        Ok(sqlx::query("UPDATE derived_tasks SET lease_until=?,updated_at=? WHERE task_id=? AND state='running' AND generation=? AND lease_owner=? AND lease_until>?")
            .bind(now.saturating_add(lease_seconds)).bind(now).bind(task_id).bind(generation as i64).bind(worker).bind(now)
            .execute(self.db.pool()).await?.rows_affected() == 1)
    }

    pub async fn complete(
        &self,
        task_id: &str,
        generation: u64,
        worker: &str,
        result: &Value,
    ) -> Result<bool> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let changed = sqlx::query("UPDATE derived_tasks SET state='succeeded',result_json=?,error=NULL,lease_owner=NULL,lease_until=NULL,updated_at=? WHERE task_id=? AND state='running' AND generation=? AND lease_owner=? AND lease_until>?")
            .bind(serde_json::to_string(result)?).bind(now).bind(task_id).bind(generation as i64).bind(worker).bind(now)
            .execute(&mut *tx).await?.rows_affected() == 1;
        if changed {
            unblock_descendants(&mut tx, task_id, now).await?;
        }
        tx.commit().await?;
        Ok(changed)
    }

    /// Fails the fenced attempt, retrying with the supplied durable backoff
    /// while attempts remain. Terminal failure blocks all descendants.
    pub async fn fail(
        &self,
        task_id: &str,
        generation: u64,
        worker: &str,
        error: &str,
        retry_after_seconds: i64,
    ) -> Result<bool> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let changed = sqlx::query("UPDATE derived_tasks SET state=CASE WHEN attempt<max_attempts THEN 'pending' ELSE 'failed' END,available_at=CASE WHEN attempt<max_attempts THEN ? ELSE available_at END,error=?,lease_owner=NULL,lease_until=NULL,updated_at=? WHERE task_id=? AND state='running' AND generation=? AND lease_owner=? AND lease_until>?")
            .bind(now.saturating_add(retry_after_seconds.max(0))).bind(error).bind(now).bind(task_id).bind(generation as i64).bind(worker).bind(now)
            .execute(&mut *tx).await?.rows_affected() == 1;
        if changed {
            let terminal: bool =
                sqlx::query_scalar("SELECT state='failed' FROM derived_tasks WHERE task_id=?")
                    .bind(task_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if terminal {
                block_descendants(&mut tx, task_id, "dependency failed", now).await?;
            }
        }
        tx.commit().await?;
        Ok(changed)
    }

    /// Requeues expired leases with a new generation, fencing stale workers.
    pub async fn recover_expired(&self) -> Result<u64> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let terminal: Vec<String> = sqlx::query_scalar("SELECT task_id FROM derived_tasks WHERE state='running' AND lease_until<=? AND attempt>=max_attempts")
            .bind(now).fetch_all(&mut *tx).await?;
        let result = sqlx::query("UPDATE derived_tasks SET state=CASE WHEN attempt<max_attempts THEN 'pending' ELSE 'failed' END,generation=generation+1,lease_owner=NULL,lease_until=NULL,error='worker lease expired',available_at=?,updated_at=? WHERE state='running' AND lease_until<=?")
            .bind(now).bind(now).bind(now).execute(&mut *tx).await?;
        for task_id in terminal {
            block_descendants(&mut tx, &task_id, "dependency worker lease expired", now).await?;
        }
        tx.commit().await?;
        Ok(result.rows_affected())
    }

    /// Explicit operator retry resets the attempt budget. Descendants become
    /// pending only when no failed/cancelled ancestor still blocks them.
    pub async fn retry(&self, task_id: &str) -> Result<bool> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let changed = sqlx::query("UPDATE derived_tasks SET state='pending',attempt=0,generation=generation+1,available_at=?,lease_owner=NULL,lease_until=NULL,error=NULL,updated_at=? WHERE task_id=? AND state='failed'")
            .bind(now).bind(now).bind(task_id).execute(&mut *tx).await?.rows_affected() == 1;
        if changed {
            unblock_descendants(&mut tx, task_id, now).await?;
        }
        tx.commit().await?;
        Ok(changed)
    }

    /// A derived catalog resolver calls this after a supposedly-ready artifact
    /// fails manifest/checksum validation. The durable succeeded row is revived
    /// so fallback playback and background regeneration happen together.
    pub async fn invalidate_succeeded_artifact(
        &self,
        artifact_key: &str,
        reason: &str,
    ) -> Result<bool> {
        let now = now_secs() as i64;
        Ok(sqlx::query("UPDATE derived_tasks SET state='pending',attempt=0,generation=generation+1,available_at=?,result_json=NULL,error=?,lease_owner=NULL,lease_until=NULL,updated_at=? WHERE artifact_key=? AND kind='proxy' AND state='succeeded' AND EXISTS (SELECT 1 FROM derived_graph_tasks gt WHERE gt.task_id=derived_tasks.task_id)")
            .bind(now).bind(reason).bind(now).bind(artifact_key)
            .execute(self.db.pool()).await?.rows_affected() == 1)
    }

    /// Cancels a task and durably propagates blockage to every non-terminal descendant.
    pub async fn cancel(&self, task_id: &str) -> Result<u64> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let consumers: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM derived_graph_tasks WHERE task_id=?")
                .bind(task_id)
                .fetch_one(&mut *tx)
                .await?;
        ensure!(
            consumers <= 1,
            "shared derived task requires graph-scoped cancellation"
        );
        let changed = sqlx::query("UPDATE derived_tasks SET state='cancelled',generation=generation+1,lease_owner=NULL,lease_until=NULL,error='cancelled',updated_at=? WHERE task_id=? AND state IN ('pending','running','blocked')")
            .bind(now).bind(task_id).execute(&mut *tx).await?.rows_affected();
        if changed > 0 {
            block_descendants(&mut tx, task_id, "dependency cancelled", now).await?;
        }
        let descendants: i64 = sqlx::query_scalar("SELECT changes()")
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(changed + descendants.max(0) as u64)
    }

    pub async fn cancel_graph(&self, graph_id: &str) -> Result<u64> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let project_id: Option<String> =
            sqlx::query_scalar("SELECT project_id FROM derived_graphs WHERE graph_id=?")
                .bind(graph_id)
                .fetch_optional(&mut *tx)
                .await?;
        let task_ids: Vec<String> =
            sqlx::query_scalar("SELECT task_id FROM derived_graph_tasks WHERE graph_id=?")
                .bind(graph_id)
                .fetch_all(&mut *tx)
                .await?;
        ensure!(!task_ids.is_empty(), "derived graph not found");
        if let Some(project_id) = project_id.filter(|id| id.starts_with("media:")) {
            sqlx::query("INSERT INTO derived_cancelled_projects(project_id,cancelled_at) VALUES(?,?) ON CONFLICT(project_id) DO UPDATE SET cancelled_at=excluded.cancelled_at")
                .bind(project_id).bind(now).execute(&mut *tx).await?;
        }
        sqlx::query("DELETE FROM derived_graph_tasks WHERE graph_id=?")
            .bind(graph_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM derived_graphs WHERE graph_id=?")
            .bind(graph_id)
            .execute(&mut *tx)
            .await?;
        let mut cancelled = 0;
        for task_id in task_ids {
            let consumers: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM derived_graph_tasks WHERE task_id=?")
                    .bind(&task_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if consumers == 0 {
                cancelled += sqlx::query("UPDATE derived_tasks SET state='cancelled',generation=generation+1,lease_owner=NULL,lease_until=NULL,error='graph cancelled',updated_at=? WHERE task_id=? AND state IN ('pending','running','blocked')")
                    .bind(now).bind(task_id).execute(&mut *tx).await?.rows_affected();
            }
        }
        tx.commit().await?;
        Ok(cancelled)
    }

    /// Detaches every graph for one project that consumes this task. Shared
    /// artifacts continue for remaining projects; exclusive tasks are fenced.
    pub async fn cancel_for_project(&self, task_id: &str, project_id: &str) -> Result<u64> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let graph_ids: Vec<String> = sqlx::query_scalar("SELECT DISTINCT g.graph_id FROM derived_graphs g JOIN derived_graph_tasks gt ON gt.graph_id=g.graph_id WHERE gt.task_id=? AND g.project_id=?")
            .bind(task_id).bind(project_id).fetch_all(&mut *tx).await?;
        ensure!(
            !graph_ids.is_empty(),
            "derived project task membership not found"
        );
        if project_id.starts_with("media:") {
            sqlx::query("INSERT INTO derived_cancelled_projects(project_id,cancelled_at) VALUES(?,?) ON CONFLICT(project_id) DO UPDATE SET cancelled_at=excluded.cancelled_at")
                .bind(project_id).bind(now).execute(&mut *tx).await?;
        }
        let mut affected = std::collections::BTreeSet::new();
        for graph_id in &graph_ids {
            let ids: Vec<String> =
                sqlx::query_scalar("SELECT task_id FROM derived_graph_tasks WHERE graph_id=?")
                    .bind(graph_id)
                    .fetch_all(&mut *tx)
                    .await?;
            affected.extend(ids);
            sqlx::query("DELETE FROM derived_graph_tasks WHERE graph_id=?")
                .bind(graph_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM derived_graphs WHERE graph_id=?")
                .bind(graph_id)
                .execute(&mut *tx)
                .await?;
        }
        let mut cancelled = 0;
        for id in affected {
            let consumers: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM derived_graph_tasks WHERE task_id=?")
                    .bind(&id)
                    .fetch_one(&mut *tx)
                    .await?;
            if consumers == 0 {
                cancelled += sqlx::query("UPDATE derived_tasks SET state='cancelled',generation=generation+1,lease_owner=NULL,lease_until=NULL,error='project graph cancelled',updated_at=? WHERE task_id=? AND state IN ('pending','running','blocked')")
                    .bind(now).bind(id).execute(&mut *tx).await?.rows_affected();
            }
        }
        tx.commit().await?;
        Ok(cancelled)
    }

    /// Detach all automatically scheduled graphs for deleted source media and
    /// return now-unreferenced proxy keys for canonical filesystem GC.
    pub async fn detach_media_project(&self, media_id: &str) -> Result<Vec<String>> {
        let project_id = format!("media:{media_id}");
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let task_ids: Vec<String> = sqlx::query_scalar("SELECT DISTINCT gt.task_id FROM derived_graphs g JOIN derived_graph_tasks gt ON gt.graph_id=g.graph_id WHERE g.project_id=?")
            .bind(&project_id).fetch_all(&mut *tx).await?;
        sqlx::query("INSERT INTO derived_cancelled_projects(project_id,cancelled_at) VALUES(?,?) ON CONFLICT(project_id) DO UPDATE SET cancelled_at=excluded.cancelled_at")
            .bind(&project_id).bind(now).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM derived_graph_tasks WHERE graph_id IN (SELECT graph_id FROM derived_graphs WHERE project_id=?)")
            .bind(&project_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM derived_graphs WHERE project_id=?")
            .bind(&project_id)
            .execute(&mut *tx)
            .await?;
        let mut proxy_keys = Vec::new();
        for task_id in task_ids {
            let consumers: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM derived_graph_tasks WHERE task_id=?")
                    .bind(&task_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if consumers == 0 {
                if let Some(key) = sqlx::query_scalar::<_, String>(
                    "SELECT artifact_key FROM derived_tasks WHERE task_id=? AND kind='proxy'",
                )
                .bind(&task_id)
                .fetch_optional(&mut *tx)
                .await?
                {
                    proxy_keys.push(key);
                }
                sqlx::query("UPDATE derived_tasks SET state='cancelled',generation=generation+1,lease_owner=NULL,lease_until=NULL,error='source media deleted',updated_at=? WHERE task_id=? AND state<>'cancelled'")
                    .bind(now).bind(&task_id).execute(&mut *tx).await?;
            } else if let Some(replacement) = sqlx::query_scalar::<_, String>("SELECT substr(g.project_id,7) FROM derived_graph_tasks gt JOIN derived_graphs g ON g.graph_id=gt.graph_id WHERE gt.task_id=? AND g.project_id LIKE 'media:%' ORDER BY g.created_at LIMIT 1")
                .bind(&task_id).fetch_optional(&mut *tx).await? {
                let row = sqlx::query("SELECT payload_json,state FROM derived_tasks WHERE task_id=?")
                    .bind(&task_id).fetch_one(&mut *tx).await?;
                let mut payload: Value = serde_json::from_str(row.try_get("payload_json")?)?;
                if payload.get("sourceId").and_then(Value::as_str) == Some(media_id) {
                    payload["sourceId"] = Value::String(replacement);
                    let running = row.try_get::<String, _>("state")? == "running";
                    sqlx::query("UPDATE derived_tasks SET payload_json=?,state=CASE WHEN ? THEN 'pending' ELSE state END,generation=generation+?,lease_owner=CASE WHEN ? THEN NULL ELSE lease_owner END,lease_until=CASE WHEN ? THEN NULL ELSE lease_until END,updated_at=? WHERE task_id=?")
                        .bind(serde_json::to_string(&payload)?).bind(running).bind(i64::from(running)).bind(running).bind(running).bind(now).bind(&task_id).execute(&mut *tx).await?;
                }
            }
        }
        tx.commit().await?;
        Ok(proxy_keys)
    }

    pub async fn project_cancelled(&self, project_id: &str) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM derived_cancelled_projects WHERE project_id=?",
        )
        .bind(project_id)
        .fetch_one(self.db.pool())
        .await?
            > 0)
    }

    pub async fn artifact_has_consumers(&self, artifact_key: &str) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM derived_graph_tasks gt JOIN derived_tasks t ON t.task_id=gt.task_id WHERE t.artifact_key=?")
            .bind(artifact_key).fetch_one(self.db.pool()).await? > 0)
    }

    pub async fn state(&self, task_id: &str) -> Result<Option<DerivedTaskState>> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT state FROM derived_tasks WHERE task_id=?")
                .bind(task_id)
                .fetch_optional(self.db.pool())
                .await?;
        value
            .map(|value| DerivedTaskState::parse(&value))
            .transpose()
    }

    pub async fn list(&self, project_id: Option<&str>) -> Result<Vec<DerivedTaskRecord>> {
        let rows = if let Some(project_id) = project_id {
            sqlx::query(
                "SELECT DISTINCT t.* FROM derived_tasks t JOIN derived_graph_tasks gt ON gt.task_id=t.task_id JOIN derived_graphs g ON g.graph_id=gt.graph_id WHERE g.project_id=? ORDER BY t.enqueued_at DESC,t.task_id",
            )
            .bind(project_id)
            .fetch_all(self.db.pool())
            .await?
        } else {
            sqlx::query("SELECT * FROM derived_tasks ORDER BY enqueued_at DESC,task_id")
                .fetch_all(self.db.pool())
                .await?
        };
        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let task_id: String = row.try_get("task_id")?;
            let dependencies = sqlx::query_scalar(
                "SELECT depends_on FROM derived_dependencies WHERE task_id=? ORDER BY depends_on",
            )
            .bind(&task_id)
            .fetch_all(self.db.pool())
            .await?;
            let result = row
                .try_get::<Option<String>, _>("result_json")?
                .map(|value| serde_json::from_str(&value))
                .transpose()?;
            let consumer_project_ids = sqlx::query_scalar("SELECT DISTINCT g.project_id FROM derived_graphs g JOIN derived_graph_tasks gt ON gt.graph_id=g.graph_id WHERE gt.task_id=? ORDER BY g.project_id")
                .bind(&task_id).fetch_all(self.db.pool()).await?;
            records.push(DerivedTaskRecord {
                task_id,
                artifact_key: row.try_get("artifact_key")?,
                kind: DerivedTaskKind::parse(row.try_get("kind")?)?,
                project_id: row.try_get("project_id")?,
                consumer_project_ids,
                state: DerivedTaskState::parse(row.try_get("state")?)?,
                priority: row.try_get("priority")?,
                priority_revision: row.try_get::<i64, _>("priority_revision")? as u64,
                generation: row.try_get::<i64, _>("generation")? as u64,
                attempt: row.try_get::<i64, _>("attempt")? as u32,
                enqueued_at: row.try_get("enqueued_at")?,
                enqueue_sequence: row.try_get("enqueue_sequence")?,
                available_at: row.try_get("available_at")?,
                lease_until: row.try_get("lease_until")?,
                result,
                error: row.try_get("error")?,
                dependencies,
            });
        }
        Ok(records)
    }

    pub async fn graph_tasks(&self, graph_id: &str) -> Result<Vec<DerivedTaskRecord>> {
        let ids: Vec<String> =
            sqlx::query_scalar("SELECT task_id FROM derived_graph_tasks WHERE graph_id=?")
                .bind(graph_id)
                .fetch_all(self.db.pool())
                .await?;
        let all = self.list(None).await?;
        let wanted: BTreeSet<_> = ids.into_iter().collect();
        Ok(all
            .into_iter()
            .filter(|task| wanted.contains(&task.task_id))
            .collect())
    }

    pub async fn fail_permanent(
        &self,
        task_id: &str,
        generation: u64,
        worker: &str,
        error: &str,
    ) -> Result<bool> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let changed = sqlx::query("UPDATE derived_tasks SET state='failed',error=?,lease_owner=NULL,lease_until=NULL,updated_at=? WHERE task_id=? AND state='running' AND generation=? AND lease_owner=? AND lease_until>?")
            .bind(error).bind(now).bind(task_id).bind(generation as i64).bind(worker).bind(now)
            .execute(&mut *tx).await?.rows_affected() == 1;
        if changed {
            block_descendants(&mut tx, task_id, "dependency failed", now).await?;
        }
        tx.commit().await?;
        Ok(changed)
    }
}

async fn block_descendants(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    task_id: &str,
    reason: &str,
    now: i64,
) -> Result<()> {
    sqlx::query("WITH RECURSIVE descendants(id) AS (SELECT task_id FROM derived_dependencies WHERE depends_on=? UNION SELECT d.task_id FROM derived_dependencies d JOIN descendants x ON d.depends_on=x.id) UPDATE derived_tasks SET state='blocked',generation=generation+1,lease_owner=NULL,lease_until=NULL,error=?,updated_at=? WHERE task_id IN (SELECT id FROM descendants) AND state IN ('pending','running','blocked')")
        .bind(task_id).bind(reason).bind(now).execute(&mut **tx).await?;
    Ok(())
}

async fn unblock_descendants(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    task_id: &str,
    now: i64,
) -> Result<()> {
    sqlx::query("WITH RECURSIVE descendants(id) AS (SELECT task_id FROM derived_dependencies WHERE depends_on=? UNION SELECT d.task_id FROM derived_dependencies d JOIN descendants x ON d.depends_on=x.id), ancestors(task_id,ancestor_id) AS (SELECT task_id,depends_on FROM derived_dependencies UNION ALL SELECT a.task_id,d.depends_on FROM ancestors a JOIN derived_dependencies d ON d.task_id=a.ancestor_id) UPDATE derived_tasks SET state='pending',error=NULL,available_at=?,updated_at=? WHERE state='blocked' AND task_id IN (SELECT id FROM descendants) AND NOT EXISTS (SELECT 1 FROM ancestors a JOIN derived_tasks p ON p.task_id=a.ancestor_id WHERE a.task_id=derived_tasks.task_id AND p.state IN ('failed','cancelled'))")
        .bind(task_id).bind(now).bind(now).execute(&mut **tx).await?;
    Ok(())
}

fn validate_graph(spec: &DerivedGraphSpec) -> Result<()> {
    ensure!(!spec.project_id.trim().is_empty(), "project id is required");
    ensure!(!spec.tasks.is_empty(), "derived graph is empty");
    ensure!(
        spec.tasks.len() <= MAX_GRAPH_NODES,
        "derived graph exceeds node limit"
    );
    let by_key: BTreeMap<_, _> = spec
        .tasks
        .iter()
        .map(|task| (task.key.as_str(), task))
        .collect();
    ensure!(
        by_key.len() == spec.tasks.len(),
        "duplicate derived task key"
    );
    let artifacts: BTreeSet<_> = spec
        .tasks
        .iter()
        .map(|task| task.artifact_key.as_str())
        .collect();
    ensure!(
        artifacts.len() == spec.tasks.len(),
        "duplicate artifact key in graph"
    );
    let edges: usize = spec.tasks.iter().map(|task| task.dependencies.len()).sum();
    ensure!(edges <= MAX_GRAPH_EDGES, "derived graph exceeds edge limit");
    for task in &spec.tasks {
        ensure!(
            !task.key.is_empty() && !task.artifact_key.is_empty(),
            "task and artifact keys are required"
        );
        ensure!(task.max_attempts > 0, "max attempts must be positive");
        if task.kind == DerivedTaskKind::Proxy {
            let payload: ProxyTaskPayload = serde_json::from_value(task.payload.clone())?;
            ensure!(
                !payload.source_id.trim().is_empty(),
                "proxy sourceId is required"
            );
            let fingerprint = Fingerprint::parse(&payload.source_fingerprint)?;
            payload.profile.validate()?;
            let expected = proxy_key(
                &SourceIdentity {
                    id: String::new(),
                    original_path: std::path::PathBuf::new(),
                    duration_seconds: 0.0,
                    fingerprint,
                },
                &payload.profile,
                FFMPEG_PROXY_COMPATIBILITY,
            );
            ensure!(
                expected.as_str() == task.artifact_key,
                "proxy artifact key does not match its semantic payload"
            );
        }
        ensure!(
            (-100..=100).contains(&task.priority),
            "priority must be within -100..=100"
        );
        for dependency in &task.dependencies {
            ensure!(
                by_key.contains_key(dependency.as_str()),
                "missing dependency {dependency}"
            );
        }
    }
    fn visit<'a>(
        key: &'a str,
        by_key: &BTreeMap<&'a str, &'a DerivedTaskSpec>,
        visiting: &mut BTreeSet<&'a str>,
        done: &mut BTreeMap<&'a str, usize>,
    ) -> Result<usize> {
        if let Some(depth) = done.get(key) {
            return Ok(*depth);
        }
        ensure!(
            visiting.insert(key),
            "derived graph contains a cycle at {key}"
        );
        let mut depth = 1;
        for dependency in &by_key[key].dependencies {
            depth = depth.max(1 + visit(dependency, by_key, visiting, done)?);
        }
        visiting.remove(key);
        done.insert(key, depth);
        Ok(depth)
    }
    let mut done = BTreeMap::new();
    for key in by_key.keys() {
        ensure!(
            visit(key, &by_key, &mut BTreeSet::new(), &mut done)? <= MAX_GRAPH_DEPTH,
            "derived graph exceeds depth limit"
        );
    }
    Ok(())
}

fn semantic_payload(value: &Value) -> Value {
    let mut semantic = value.clone();
    if let Value::Object(object) = &mut semantic {
        object.remove("sourceId");
    }
    semantic
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn store() -> (tempfile::TempDir, DerivedJobStore) {
        let dir = tempdir().unwrap();
        let db = Db::open(dir.path()).await.unwrap();
        let store = DerivedJobStore::new(db);
        store.migrate().await.unwrap();
        (dir, store)
    }
    fn task(key: &str, deps: &[&str], priority: i64) -> DerivedTaskSpec {
        DerivedTaskSpec {
            key: key.into(),
            artifact_key: format!("artifact-{key}"),
            kind: DerivedTaskKind::Probe,
            payload: Value::Null,
            dependencies: deps.iter().map(|v| (*v).into()).collect(),
            priority,
            max_attempts: 2,
        }
    }
    fn graph(project: &str, tasks: Vec<DerivedTaskSpec>) -> DerivedGraphSpec {
        DerivedGraphSpec {
            project_id: project.into(),
            tasks,
        }
    }

    #[tokio::test]
    async fn rejects_missing_dependencies_and_cycles() {
        let (_dir, store) = store().await;
        assert!(store
            .enqueue_graph(&graph("p", vec![task("a", &["missing"], 0)]))
            .await
            .is_err());
        assert!(store
            .enqueue_graph(&graph(
                "p",
                vec![task("a", &["b"], 0), task("b", &["a"], 0)]
            ))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn dependencies_gate_claims() {
        let (_dir, store) = store().await;
        let enqueued = store
            .enqueue_graph(&graph(
                "p",
                vec![task("probe", &[], 0), task("proxy", &["probe"], 100)],
            ))
            .await
            .unwrap();
        let first = store.claim_next("w", 60).await.unwrap().unwrap();
        assert_eq!(first.task_id, enqueued.tasks["probe"]);
        assert!(store.claim_next("w", 60).await.unwrap().is_none());
        assert!(store
            .complete(&first.task_id, first.generation, "w", &Value::Null)
            .await
            .unwrap());
        assert_eq!(
            store.claim_next("w", 60).await.unwrap().unwrap().task_id,
            enqueued.tasks["proxy"]
        );
    }

    #[tokio::test]
    async fn artifact_enqueue_is_idempotent_across_graphs() {
        let (_dir, store) = store().await;
        let spec = graph("p", vec![task("a", &[], 0)]);
        let one = store.enqueue_graph(&spec).await.unwrap();
        let two = store.enqueue_graph(&spec).await.unwrap();
        assert_ne!(one.graph_id, two.graph_id);
        assert_eq!(one.tasks["a"], two.tasks["a"]);
    }

    #[tokio::test]
    async fn shared_artifact_completes_once_and_requires_graph_scoped_cancel() {
        let (_dir, store) = store().await;
        let spec = graph("p", vec![task("a", &[], 0)]);
        let first = store.enqueue_graph(&spec).await.unwrap();
        let second = store.enqueue_graph(&spec).await.unwrap();
        let id = first.tasks["a"].clone();
        assert!(store.cancel(&id).await.is_err());
        assert_eq!(store.cancel_graph(&first.graph_id).await.unwrap(), 0);
        let claim = store.claim_next("w", 60).await.unwrap().unwrap();
        assert!(store
            .complete(&id, claim.generation, "w", &Value::Null)
            .await
            .unwrap());
        assert_eq!(
            store.state(&second.tasks["a"]).await.unwrap(),
            Some(DerivedTaskState::Succeeded)
        );
        assert!(store.enqueue_graph(&spec).await.is_ok());
    }

    #[tokio::test]
    async fn explicit_reenqueue_revives_a_cancelled_manual_artifact() {
        let (_dir, store) = store().await;
        let spec = graph("manual", vec![task("a", &[], 0)]);
        let first = store.enqueue_graph(&spec).await.unwrap();
        let id = first.tasks["a"].clone();
        assert_eq!(store.cancel_graph(&first.graph_id).await.unwrap(), 1);
        assert_eq!(
            store.state(&id).await.unwrap(),
            Some(DerivedTaskState::Cancelled)
        );
        store.enqueue_graph(&spec).await.unwrap();
        assert_eq!(
            store.state(&id).await.unwrap(),
            Some(DerivedTaskState::Pending)
        );
        assert_eq!(
            store.claim_next("w", 60).await.unwrap().unwrap().task_id,
            id
        );
    }

    #[tokio::test]
    async fn invalid_catalog_artifact_revives_succeeded_producer() {
        let (_dir, store) = store().await;
        let mut proxy = task("a", &[], 0);
        proxy.kind = DerivedTaskKind::Proxy;
        let fingerprint = Fingerprint::digest(b"source");
        let profile = ProxyProfile::default();
        proxy.artifact_key = proxy_key(
            &SourceIdentity {
                id: "source".into(),
                original_path: std::path::PathBuf::new(),
                duration_seconds: 1.0,
                fingerprint: fingerprint.clone(),
            },
            &profile,
            FFMPEG_PROXY_COMPATIBILITY,
        )
        .to_string();
        let artifact_key = proxy.artifact_key.clone();
        proxy.payload = serde_json::json!({ "sourceId": "source", "sourceFingerprint": fingerprint, "profile": profile });
        let ready = store.enqueue_graph(&graph("p", vec![proxy])).await.unwrap();
        let id = ready.tasks["a"].clone();
        let claim = store.claim_next("w", 60).await.unwrap().unwrap();
        assert!(store
            .complete(
                &id,
                claim.generation,
                "w",
                &serde_json::json!({"ready":true})
            )
            .await
            .unwrap());
        assert!(store
            .invalidate_succeeded_artifact(&artifact_key, "checksum mismatch")
            .await
            .unwrap());
        assert_eq!(
            store.state(&id).await.unwrap(),
            Some(DerivedTaskState::Pending)
        );
        assert_eq!(
            store.claim_next("w2", 60).await.unwrap().unwrap().task_id,
            id
        );

        let probe = store
            .enqueue_graph(&graph("p2", vec![task("probe", &[], 0)]))
            .await
            .unwrap();
        let probe_id = probe.tasks["probe"].clone();
        let claim = store.claim_next("probe-worker", 60).await.unwrap().unwrap();
        assert!(store
            .complete(&probe_id, claim.generation, "probe-worker", &Value::Null)
            .await
            .unwrap());
        assert!(!store
            .invalidate_succeeded_artifact("artifact-probe", "hostile request")
            .await
            .unwrap());
        assert_eq!(
            store.state(&probe_id).await.unwrap(),
            Some(DerivedTaskState::Succeeded)
        );
    }

    #[tokio::test]
    async fn project_scoped_cancel_detaches_shared_artifact_then_fences_last_consumer() {
        let (_dir, store) = store().await;
        let first = store
            .enqueue_graph(&graph("media:a", vec![task("a", &[], 0)]))
            .await
            .unwrap();
        let second = store
            .enqueue_graph(&graph("media:b", vec![task("a", &[], 0)]))
            .await
            .unwrap();
        let id = first.tasks["a"].clone();
        assert_eq!(id, second.tasks["a"]);
        assert_eq!(store.cancel_for_project(&id, "media:a").await.unwrap(), 0);
        assert!(store.project_cancelled("media:a").await.unwrap());
        assert!(store
            .enqueue_graph(&graph("media:a", vec![task("a", &[], 0)]))
            .await
            .is_err());
        assert_eq!(
            store.state(&id).await.unwrap(),
            Some(DerivedTaskState::Pending)
        );
        assert_eq!(store.cancel_for_project(&id, "media:b").await.unwrap(), 1);
        assert_eq!(
            store.state(&id).await.unwrap(),
            Some(DerivedTaskState::Cancelled)
        );
    }

    #[tokio::test]
    async fn priority_update_is_compare_and_swap() {
        let (_dir, store) = store().await;
        let id = store
            .enqueue_graph(&graph("p", vec![task("a", &[], 0)]))
            .await
            .unwrap()
            .tasks["a"]
            .clone();
        assert_eq!(store.set_priority(&id, 10, 0).await.unwrap(), Ok(1));
        assert_eq!(
            store.set_priority(&id, 20, 0).await.unwrap(),
            Err(PriorityConflict { actual_revision: 1 })
        );
    }

    #[tokio::test]
    async fn project_cursor_breaks_equal_priority_ties_fairly() {
        let (_dir, store) = store().await;
        store
            .enqueue_graph(&graph("a", vec![task("a1", &[], 0), task("a2", &[], 0)]))
            .await
            .unwrap();
        let mut b1 = task("b1", &[], 0);
        b1.artifact_key = "artifact-project-b1".into();
        let mut b2 = task("b2", &[], 0);
        b2.artifact_key = "artifact-project-b2".into();
        store
            .enqueue_graph(&graph("b", vec![b1, b2]))
            .await
            .unwrap();
        let first = store.claim_next("w", 60).await.unwrap().unwrap();
        store
            .complete(&first.task_id, first.generation, "w", &Value::Null)
            .await
            .unwrap();
        let second = store.claim_next("w", 60).await.unwrap().unwrap();
        assert_ne!(second.project_id, first.project_id);
    }

    #[tokio::test]
    async fn interactive_descendant_donates_priority_to_its_dependency() {
        let (_dir, store) = store().await;
        let enqueued = store
            .enqueue_graph(&graph(
                "p",
                vec![
                    task("unrelated", &[], 5),
                    task("root", &[], -10),
                    task("interactive", &["root"], 10),
                ],
            ))
            .await
            .unwrap();
        assert_eq!(
            store.claim_next("w", 60).await.unwrap().unwrap().task_id,
            enqueued.tasks["root"]
        );
    }

    #[tokio::test]
    async fn lease_recovery_fences_the_old_worker() {
        let (_dir, store) = store().await;
        let id = store
            .enqueue_graph(&graph("p", vec![task("a", &[], 0)]))
            .await
            .unwrap()
            .tasks["a"]
            .clone();
        let old = store.claim_next("old", 1).await.unwrap().unwrap();
        sqlx::query("UPDATE derived_tasks SET lease_until=0 WHERE task_id=?")
            .bind(&id)
            .execute(store.db.pool())
            .await
            .unwrap();
        assert_eq!(store.recover_expired().await.unwrap(), 1);
        let new = store.claim_next("new", 60).await.unwrap().unwrap();
        assert!(new.generation > old.generation);
        assert!(!store
            .complete(&id, old.generation, "old", &Value::Null)
            .await
            .unwrap());
        assert!(store
            .complete(&id, new.generation, "new", &Value::Null)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn restart_preserves_priority_and_recovers_an_expired_claim() {
        let (dir, store) = store().await;
        let id = store
            .enqueue_graph(&graph("p", vec![task("a", &[], 0)]))
            .await
            .unwrap()
            .tasks["a"]
            .clone();
        assert_eq!(store.set_priority(&id, 42, 0).await.unwrap(), Ok(1));
        let claim = store.claim_next("old", 60).await.unwrap().unwrap();
        sqlx::query("UPDATE derived_tasks SET lease_until=0 WHERE task_id=?")
            .bind(&id)
            .execute(store.db.pool())
            .await
            .unwrap();
        drop(store);
        let reopened = DerivedJobStore::new(Db::open(dir.path()).await.unwrap());
        assert_eq!(reopened.recover_expired().await.unwrap(), 1);
        let record = reopened.list(None).await.unwrap().remove(0);
        assert_eq!(record.priority, 42);
        assert_eq!(record.priority_revision, 1);
        let resumed = reopened.claim_next("new", 60).await.unwrap().unwrap();
        assert!(resumed.generation > claim.generation);
    }

    #[tokio::test]
    async fn migration_backfills_the_early_schema_enqueue_sequence() {
        let (_dir, store) = store().await;
        sqlx::query("DROP TABLE derived_dependencies; DROP TABLE derived_graph_tasks; DROP TABLE derived_graphs; DROP TABLE derived_tasks; DROP TABLE derived_project_fairness; DROP TABLE derived_scheduler;")
            .execute(store.db.pool()).await.unwrap();
        sqlx::query("CREATE TABLE derived_tasks(task_id TEXT PRIMARY KEY,artifact_key TEXT NOT NULL UNIQUE,kind TEXT NOT NULL,project_id TEXT NOT NULL,payload_json TEXT NOT NULL,state TEXT NOT NULL,priority INTEGER NOT NULL,priority_revision INTEGER NOT NULL DEFAULT 0,enqueued_at INTEGER NOT NULL,available_at INTEGER NOT NULL,generation INTEGER NOT NULL DEFAULT 0,attempt INTEGER NOT NULL DEFAULT 0,max_attempts INTEGER NOT NULL DEFAULT 3,lease_owner TEXT,lease_until INTEGER,result_json TEXT,error TEXT,updated_at INTEGER NOT NULL); CREATE TABLE derived_scheduler(singleton INTEGER PRIMARY KEY,claim_sequence INTEGER NOT NULL); INSERT INTO derived_scheduler VALUES(1,0);")
            .execute(store.db.pool()).await.unwrap();
        store.migrate().await.unwrap();
        let task_columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('derived_tasks')")
                .fetch_all(store.db.pool())
                .await
                .unwrap();
        let scheduler_columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('derived_scheduler')")
                .fetch_all(store.db.pool())
                .await
                .unwrap();
        assert!(task_columns.contains(&"enqueue_sequence".into()));
        assert!(scheduler_columns.contains(&"enqueue_sequence".into()));
    }

    #[tokio::test]
    async fn retries_then_blocks_descendants() {
        let (_dir, store) = store().await;
        let graph = store
            .enqueue_graph(&graph("p", vec![task("a", &[], 0), task("b", &["a"], 0)]))
            .await
            .unwrap();
        let first = store.claim_next("w", 60).await.unwrap().unwrap();
        assert!(store
            .fail(&first.task_id, first.generation, "w", "oops", 0)
            .await
            .unwrap());
        let second = store.claim_next("w", 60).await.unwrap().unwrap();
        assert!(store
            .fail(&second.task_id, second.generation, "w", "oops", 0)
            .await
            .unwrap());
        assert_eq!(
            store.state(&graph.tasks["a"]).await.unwrap(),
            Some(DerivedTaskState::Failed)
        );
        assert_eq!(
            store.state(&graph.tasks["b"]).await.unwrap(),
            Some(DerivedTaskState::Blocked)
        );
        assert!(store.retry(&graph.tasks["a"]).await.unwrap());
        assert_eq!(
            store.state(&graph.tasks["b"]).await.unwrap(),
            Some(DerivedTaskState::Pending)
        );
        let retried = store.claim_next("w", 60).await.unwrap().unwrap();
        assert_eq!(retried.task_id, graph.tasks["a"]);
        assert!(store
            .complete(&retried.task_id, retried.generation, "w", &Value::Null)
            .await
            .unwrap());
        assert_eq!(
            store.claim_next("w", 60).await.unwrap().unwrap().task_id,
            graph.tasks["b"]
        );
    }

    #[tokio::test]
    async fn cancel_propagates_and_fences_running_attempt() {
        let (_dir, store) = store().await;
        let graph = store
            .enqueue_graph(&graph(
                "p",
                vec![
                    task("a", &[], 0),
                    task("b", &["a"], 0),
                    task("c", &["b"], 0),
                ],
            ))
            .await
            .unwrap();
        let claim = store.claim_next("w", 60).await.unwrap().unwrap();
        assert!(store.cancel(&claim.task_id).await.unwrap() >= 1);
        assert!(!store
            .complete(&claim.task_id, claim.generation, "w", &Value::Null)
            .await
            .unwrap());
        assert_eq!(
            store.state(&graph.tasks["a"]).await.unwrap(),
            Some(DerivedTaskState::Cancelled)
        );
        assert_eq!(
            store.state(&graph.tasks["c"]).await.unwrap(),
            Some(DerivedTaskState::Blocked)
        );
    }
}
