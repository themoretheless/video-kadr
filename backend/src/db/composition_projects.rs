use std::collections::BTreeSet;

use anyhow::{ensure, Context, Result};
use serde::Serialize;
use serde_json::Value;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, Sqlite, Transaction};
use uuid::Uuid;

use super::Db;
use crate::library::now_secs;

pub const COMPOSITION_PROJECT_SCHEMA_VERSION: u32 = 2;
pub const COMPOSITION_PROJECT_MODE: &str = "composition";
pub const MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_COMPOSITION_PROJECT_SOURCES: usize = 32;
pub const MAX_COMPOSITION_PROJECT_NAME_BYTES: usize = 256;
const COMPOSITION_DOCUMENT_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionProject {
    pub id: String,
    pub space_id: Option<String>,
    pub name: String,
    pub schema_version: u32,
    pub mode: String,
    pub document: Value,
    pub source_ids: Vec<String>,
    pub revision: u64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug)]
struct ProjectRow {
    id: String,
    space_id: Option<String>,
    name: String,
    schema_version: u32,
    mode: String,
    document: Value,
    revision: u64,
    created_at: i64,
    updated_at: i64,
}

pub(super) async fn migrate(pool: &sqlx::SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS composition_projects ( \
           id TEXT PRIMARY KEY, \
           name TEXT NOT NULL, \
           document_json TEXT NOT NULL, \
           schema_version INTEGER NOT NULL CHECK (schema_version = 2), \
           mode TEXT NOT NULL CHECK (mode = 'composition'), \
           created_at INTEGER NOT NULL, \
           updated_at INTEGER NOT NULL, \
           revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0), \
           updated_order INTEGER NOT NULL \
         )",
    )
    .execute(pool)
    .await?;
    let columns = sqlx::query("PRAGMA table_info(composition_projects)")
        .fetch_all(pool)
        .await?;
    if !columns
        .iter()
        .any(|row| row.get::<String, _>("name") == "space_id")
    {
        sqlx::query("ALTER TABLE composition_projects ADD COLUMN space_id TEXT REFERENCES collaboration_spaces(id) ON DELETE RESTRICT")
            .execute(pool)
            .await?;
    }
    if !columns
        .iter()
        .any(|row| row.get::<String, _>("name") == "revision")
    {
        sqlx::query("ALTER TABLE composition_projects ADD COLUMN revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0)")
            .execute(pool)
            .await?;
    }
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_composition_projects_space ON composition_projects(space_id, updated_order DESC)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_composition_projects_updated_order \
         ON composition_projects(updated_order DESC)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS composition_project_sources ( \
           project_id TEXT NOT NULL, \
           source_id TEXT NOT NULL, \
           source_order INTEGER NOT NULL, \
           PRIMARY KEY (project_id, source_id), \
           UNIQUE (project_id, source_order), \
           FOREIGN KEY (project_id) REFERENCES composition_projects(id) ON DELETE CASCADE \
         )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_composition_project_sources_source_id \
         ON composition_project_sources(source_id)",
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    pub async fn create_composition_project(
        &self,
        name: &str,
        document: &Value,
        source_ids: &[String],
    ) -> Result<CompositionProject> {
        self.create_composition_project_with_owner(name, document, source_ids, None)
            .await
    }

    pub async fn create_owned_composition_project(
        &self,
        name: &str,
        document: &Value,
        source_ids: &[String],
        owner: &str,
    ) -> Result<CompositionProject> {
        self.create_composition_project_with_owner(name, document, source_ids, Some(owner))
            .await
    }

    pub async fn create_space_composition_project(
        &self,
        name: &str,
        document: &Value,
        source_ids: &[String],
        owner: &str,
        space_id: &str,
    ) -> Result<CompositionProject> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM collaboration_space_members WHERE space_id = ? AND actor = ?",
        )
        .bind(space_id)
        .bind(owner)
        .fetch_optional(&self.pool)
        .await?;
        ensure!(
            matches!(role.as_deref(), Some("owner" | "editor")),
            "space role cannot create project"
        );
        self.create_composition_project_with_space(name, document, source_ids, owner, space_id)
            .await
    }

    async fn create_composition_project_with_owner(
        &self,
        name: &str,
        document: &Value,
        source_ids: &[String],
        owner: Option<&str>,
    ) -> Result<CompositionProject> {
        let document_json = validate_storage_input(name, document, source_ids)?;
        let id = Uuid::new_v4().to_string();
        let now = now_secs() as i64;
        let mut transaction = self.pool.begin().await?;
        if owner.is_some() {
            claim_project_sources(&mut transaction, None, source_ids, owner.unwrap()).await?;
        }
        let row = sqlx::query(
            "INSERT INTO composition_projects \
               (id, name, document_json, schema_version, mode, created_at, updated_at, updated_order) \
             VALUES (?, ?, ?, ?, ?, ?, ?, \
               COALESCE((SELECT MAX(updated_order) + 1 FROM composition_projects), 1)) \
             RETURNING id, space_id, name, document_json, schema_version, mode, revision, created_at, updated_at",
        )
        .bind(id)
        .bind(name)
        .bind(document_json)
        .bind(i64::from(COMPOSITION_PROJECT_SCHEMA_VERSION))
        .bind(COMPOSITION_PROJECT_MODE)
        .bind(now)
        .bind(now)
        .fetch_one(&mut *transaction)
        .await?;
        replace_sources(&mut transaction, row.try_get("id")?, source_ids).await?;
        let project = row_to_composition_project(row, source_ids.to_vec())?;
        if let Some(owner) = owner {
            sqlx::query(
                "INSERT INTO composition_project_members \
                 (project_id, actor, role, created_at, updated_at) VALUES (?, ?, 'owner', ?, ?)",
            )
            .bind(&project.id)
            .bind(owner)
            .bind(now)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(project)
    }

    async fn create_composition_project_with_space(
        &self,
        name: &str,
        document: &Value,
        source_ids: &[String],
        owner: &str,
        space_id: &str,
    ) -> Result<CompositionProject> {
        let document_json = validate_storage_input(name, document, source_ids)?;
        let id = Uuid::new_v4().to_string();
        let now = now_secs() as i64;
        let mut transaction = self.pool.begin().await?;
        claim_project_sources(&mut transaction, Some(space_id), source_ids, owner).await?;
        let row = sqlx::query(
            "INSERT INTO composition_projects (id, space_id, name, document_json, schema_version, mode, created_at, updated_at, updated_order) VALUES (?, ?, ?, ?, ?, ?, ?, ?, COALESCE((SELECT MAX(updated_order) + 1 FROM composition_projects), 1)) RETURNING id, space_id, name, document_json, schema_version, mode, revision, created_at, updated_at",
        )
        .bind(id).bind(space_id).bind(name).bind(document_json)
        .bind(i64::from(COMPOSITION_PROJECT_SCHEMA_VERSION)).bind(COMPOSITION_PROJECT_MODE)
        .bind(now).bind(now).fetch_one(&mut *transaction).await?;
        let project_id: String = row.try_get("id")?;
        replace_sources(&mut transaction, &project_id, source_ids).await?;
        sqlx::query(
            "INSERT INTO composition_project_members (project_id, actor, role, created_at, updated_at) SELECT ?, actor, CASE WHEN actor = ? THEN 'owner' WHEN role IN ('owner','editor') THEN 'editor' ELSE 'viewer' END, ?, ? FROM collaboration_space_members WHERE space_id = ?",
        )
        .bind(&project_id).bind(owner).bind(now).bind(now).bind(space_id)
        .execute(&mut *transaction).await?;
        let project = row_to_composition_project(row, source_ids.to_vec())?;
        transaction.commit().await?;
        Ok(project)
    }

    pub async fn list_composition_projects_for(
        &self,
        actor: &str,
    ) -> Result<Vec<CompositionProject>> {
        let mut transaction = self.pool.begin().await?;
        let rows = sqlx::query(
            "SELECT p.id, p.space_id, p.name, p.document_json, p.schema_version, p.mode, p.revision, p.created_at, p.updated_at \
             FROM composition_projects p JOIN composition_project_members m ON m.project_id = p.id \
             WHERE m.actor = ? ORDER BY p.updated_order DESC",
        )
        .bind(actor)
        .fetch_all(&mut *transaction)
        .await?;
        let mut projects = Vec::with_capacity(rows.len());
        for row in rows {
            let parsed = row_to_project_row(row)?;
            let source_ids = composition_project_sources(&mut transaction, &parsed.id).await?;
            projects.push(parsed.with_sources(source_ids)?);
        }
        transaction.commit().await?;
        Ok(projects)
    }

    pub async fn get_composition_project_for(
        &self,
        id: &str,
        actor: &str,
    ) -> Result<Option<CompositionProject>> {
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT p.id, p.space_id, p.name, p.document_json, p.schema_version, p.mode, p.revision, p.created_at, p.updated_at \
             FROM composition_projects p JOIN composition_project_members m ON m.project_id = p.id \
             WHERE p.id = ? AND m.actor = ?",
        )
        .bind(id)
        .bind(actor)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            transaction.commit().await?;
            return Ok(None);
        };
        let parsed = row_to_project_row(row)?;
        let source_ids = composition_project_sources(&mut transaction, &parsed.id).await?;
        transaction.commit().await?;
        Ok(Some(parsed.with_sources(source_ids)?))
    }

    pub async fn update_composition_project_for(
        &self,
        id: &str,
        actor: &str,
        name: &str,
        document: &Value,
        source_ids: &[String],
        expected_revision: u64,
    ) -> Result<Option<CompositionProject>> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM composition_project_members WHERE project_id = ? AND actor = ?",
        )
        .bind(id)
        .bind(actor)
        .fetch_optional(&self.pool)
        .await?;
        ensure!(
            matches!(role.as_deref(), Some("owner" | "editor")),
            "role cannot edit composition project"
        );
        self.update_composition_project_at_revision(
            id,
            name,
            document,
            source_ids,
            Some(expected_revision),
        )
        .await
    }

    pub async fn delete_composition_project_for(&self, id: &str, actor: &str) -> Result<bool> {
        let allowed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM composition_projects project \
             LEFT JOIN composition_project_members member ON member.project_id = project.id AND member.actor = ? \
             LEFT JOIN collaboration_space_members space_member ON space_member.space_id = project.space_id AND space_member.actor = ? \
             WHERE project.id = ? AND (member.role = 'owner' OR space_member.role = 'owner'))",
        )
        .bind(actor)
        .bind(actor)
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        ensure!(allowed, "role cannot delete composition project");
        self.delete_composition_project(id).await
    }

    pub async fn update_composition_project(
        &self,
        id: &str,
        name: &str,
        document: &Value,
        source_ids: &[String],
    ) -> Result<Option<CompositionProject>> {
        self.update_composition_project_at_revision(id, name, document, source_ids, None)
            .await
    }

    async fn update_composition_project_at_revision(
        &self,
        id: &str,
        name: &str,
        document: &Value,
        source_ids: &[String],
        expected_revision: Option<u64>,
    ) -> Result<Option<CompositionProject>> {
        let document_json = validate_storage_input(name, document, source_ids)?;
        let now = now_secs() as i64;
        let mut transaction = self.pool.begin().await?;
        let project_space: Option<Option<String>> =
            sqlx::query_scalar("SELECT space_id FROM composition_projects WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *transaction)
                .await?;
        if let Some(space_id) = project_space {
            claim_project_sources(
                &mut transaction,
                space_id.as_deref(),
                source_ids,
                "project-update",
            )
            .await?;
        }
        let row = sqlx::query(
            "UPDATE composition_projects SET \
               name = ?, document_json = ?, schema_version = ?, mode = ?, updated_at = ?, revision = revision + 1, \
               updated_order = COALESCE((SELECT MAX(updated_order) + 1 FROM composition_projects), 1) \
             WHERE id = ? AND (? IS NULL OR revision = ?) \
             RETURNING id, space_id, name, document_json, schema_version, mode, revision, created_at, updated_at",
        )
        .bind(name)
        .bind(document_json)
        .bind(i64::from(COMPOSITION_PROJECT_SCHEMA_VERSION))
        .bind(COMPOSITION_PROJECT_MODE)
        .bind(now)
        .bind(id)
        .bind(expected_revision.map(i64::try_from).transpose()?)
        .bind(expected_revision.map(i64::try_from).transpose()?)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM composition_projects WHERE id = ?)",
            )
            .bind(id)
            .fetch_one(&mut *transaction)
            .await?;
            transaction.rollback().await?;
            ensure!(!exists, "stale composition project revision");
            return Ok(None);
        };
        replace_sources(&mut transaction, id, source_ids).await?;
        let project = row_to_composition_project(row, source_ids.to_vec())?;
        transaction.commit().await?;
        Ok(Some(project))
    }

    pub async fn list_composition_projects(&self) -> Result<Vec<CompositionProject>> {
        let mut transaction = self.pool.begin().await?;
        let rows = sqlx::query(
            "SELECT id, space_id, name, document_json, schema_version, mode, revision, created_at, updated_at \
             FROM composition_projects ORDER BY updated_order DESC",
        )
        .fetch_all(&mut *transaction)
        .await?;
        let mut projects = Vec::with_capacity(rows.len());
        for row in rows {
            let parsed = row_to_project_row(row)?;
            let source_ids = composition_project_sources(&mut transaction, &parsed.id).await?;
            projects.push(parsed.with_sources(source_ids)?);
        }
        transaction.commit().await?;
        Ok(projects)
    }

    pub async fn get_composition_project(&self, id: &str) -> Result<Option<CompositionProject>> {
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT id, space_id, name, document_json, schema_version, mode, revision, created_at, updated_at \
             FROM composition_projects WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            transaction.commit().await?;
            return Ok(None);
        };
        let parsed = row_to_project_row(row)?;
        let source_ids = composition_project_sources(&mut transaction, &parsed.id).await?;
        transaction.commit().await?;
        Ok(Some(parsed.with_sources(source_ids)?))
    }

    pub async fn delete_composition_project(&self, id: &str) -> Result<bool> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("DELETE FROM composition_project_sources WHERE project_id = ?")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        let result = sqlx::query("DELETE FROM composition_projects WHERE id = ?")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    /// Count saved composition documents that still reference a library
    /// source. Library deletion uses this index to fail closed instead of
    /// silently turning durable projects into missing-media projects.
    pub async fn composition_source_reference_count(&self, source_id: &str) -> Result<i64> {
        sqlx::query_scalar("SELECT COUNT(*) FROM composition_project_sources WHERE source_id = ?")
            .bind(source_id)
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn composition_project_member_actors(&self, id: &str) -> Result<Vec<String>> {
        sqlx::query_scalar(
            "SELECT actor FROM composition_project_members WHERE project_id = ? ORDER BY actor",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }
}

async fn claim_project_sources(
    transaction: &mut Transaction<'_, Sqlite>,
    space_id: Option<&str>,
    source_ids: &[String],
    actor: &str,
) -> Result<()> {
    for source_id in source_ids {
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT space_id FROM collaboration_space_media WHERE source_id = ?",
        )
        .bind(source_id)
        .fetch_optional(&mut **transaction)
        .await?;
        match (space_id, existing.as_deref()) {
            (Some(expected), Some(actual)) => ensure!(
                expected == actual,
                "source belongs to another collaboration space"
            ),
            (None, Some(_)) => {
                anyhow::bail!("scoped source cannot be used by an unscoped project")
            }
            (Some(space_id), None) => {
                sqlx::query("INSERT INTO collaboration_space_media (space_id, source_id, created_by, created_at) VALUES (?, ?, ?, ?)")
                    .bind(space_id)
                    .bind(source_id)
                    .bind(actor)
                    .bind(i64::try_from(now_secs())?)
                    .execute(&mut **transaction)
                    .await?;
            }
            (None, None) => {}
        }
    }
    Ok(())
}

impl ProjectRow {
    fn with_sources(self, source_ids: Vec<String>) -> Result<CompositionProject> {
        ensure!(
            extract_document_source_ids(&self.document)? == source_ids,
            "stored composition source index does not match document"
        );
        Ok(CompositionProject {
            id: self.id,
            space_id: self.space_id,
            name: self.name,
            schema_version: self.schema_version,
            mode: self.mode,
            document: self.document,
            source_ids,
            revision: self.revision,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn validate_storage_input(name: &str, document: &Value, source_ids: &[String]) -> Result<String> {
    ensure!(!name.trim().is_empty(), "composition project name is empty");
    ensure!(
        name.len() <= MAX_COMPOSITION_PROJECT_NAME_BYTES,
        "composition project name is too large"
    );
    let extracted_source_ids = extract_document_source_ids(document)?;
    ensure!(
        extracted_source_ids == source_ids,
        "composition source index does not match document"
    );
    let mut unique = BTreeSet::new();
    for source_id in source_ids {
        ensure!(valid_source_id(source_id), "invalid composition source id");
        ensure!(unique.insert(source_id), "duplicate composition source id");
    }
    let document_json = serde_json::to_string(document)?;
    ensure!(
        document_json.len() <= MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES,
        "composition project document is too large"
    );
    Ok(document_json)
}

fn extract_document_source_ids(document: &Value) -> Result<Vec<String>> {
    let document = document
        .as_object()
        .context("composition document is not an object")?;
    ensure!(
        document.get("schemaVersion").and_then(Value::as_u64)
            == Some(COMPOSITION_DOCUMENT_SCHEMA_VERSION),
        "unsupported composition document schema"
    );
    let sources = document
        .get("sources")
        .and_then(Value::as_object)
        .context("composition document sources are not an object")?;
    ensure!(
        sources.len() <= MAX_COMPOSITION_PROJECT_SOURCES,
        "composition project has too many sources"
    );
    let mut source_ids = Vec::with_capacity(sources.len());
    for (source_id, source) in sources {
        ensure!(valid_source_id(source_id), "invalid composition source id");
        let embedded_id = source
            .as_object()
            .and_then(|object| object.get("id"))
            .and_then(Value::as_str);
        ensure!(
            embedded_id == Some(source_id.as_str()),
            "composition source key and id differ"
        );
        source_ids.push(source_id.clone());
    }
    Ok(source_ids)
}

pub(crate) fn valid_source_id(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value.bytes().any(|byte| byte.is_ascii_alphanumeric())
}

async fn replace_sources(
    transaction: &mut Transaction<'_, Sqlite>,
    project_id: &str,
    source_ids: &[String],
) -> Result<()> {
    sqlx::query("DELETE FROM composition_project_sources WHERE project_id = ?")
        .bind(project_id)
        .execute(&mut **transaction)
        .await?;
    for (order, source_id) in source_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO composition_project_sources (project_id, source_id, source_order) \
             VALUES (?, ?, ?)",
        )
        .bind(project_id)
        .bind(source_id)
        .bind(i64::try_from(order).context("composition source order overflow")?)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

async fn composition_project_sources(
    transaction: &mut Transaction<'_, Sqlite>,
    project_id: &str,
) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT source_id FROM composition_project_sources \
         WHERE project_id = ? ORDER BY source_order",
    )
    .bind(project_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(Into::into)
}

fn row_to_project_row(row: SqliteRow) -> Result<ProjectRow> {
    let schema_version = u32::try_from(row.try_get::<i64, _>("schema_version")?)
        .context("invalid composition project schema version")?;
    let mode: String = row.try_get("mode")?;
    ensure!(
        schema_version == COMPOSITION_PROJECT_SCHEMA_VERSION && mode == COMPOSITION_PROJECT_MODE,
        "invalid stored composition project envelope"
    );
    Ok(ProjectRow {
        id: row.try_get("id")?,
        space_id: row.try_get("space_id")?,
        name: row.try_get("name")?,
        schema_version,
        mode,
        document: serde_json::from_str(&row.try_get::<String, _>("document_json")?)?,
        revision: u64::try_from(row.try_get::<i64, _>("revision")?)
            .context("invalid composition project revision")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_composition_project(
    row: SqliteRow,
    source_ids: Vec<String>,
) -> Result<CompositionProject> {
    row_to_project_row(row)?.with_sources(source_ids)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    async fn db() -> (Db, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let db = Db::open(directory.path()).await.unwrap();
        (db, directory)
    }

    #[tokio::test]
    async fn create_update_list_and_delete_keep_document_and_sources_atomic() {
        let (db, _directory) = db().await;
        let first = db
            .create_composition_project(
                "First",
                &json!({"schemaVersion": 1, "sources": {"source-a": {"id": "source-a"}}}),
                &["source-a".into()],
            )
            .await
            .unwrap();
        let second = db
            .create_composition_project("Second", &json!({"schemaVersion": 1, "sources": {}}), &[])
            .await
            .unwrap();
        assert_eq!(
            db.list_composition_projects()
                .await
                .unwrap()
                .iter()
                .map(|project| project.id.as_str())
                .collect::<Vec<_>>(),
            vec![second.id.as_str(), first.id.as_str()]
        );

        let updated = db
            .update_composition_project(
                &first.id,
                "First updated",
                &json!({
                    "schemaVersion": 1,
                    "sources": {
                        "source-b": {"id": "source-b"},
                        "source-c": {"id": "source-c"}
                    }
                }),
                &["source-b".into(), "source-c".into()],
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.id, first.id);
        assert_eq!(updated.created_at, first.created_at);
        assert_eq!(updated.source_ids, ["source-b", "source-c"]);
        assert_eq!(
            db.list_composition_projects().await.unwrap()[0].id,
            first.id
        );
        let indexed: Vec<String> = sqlx::query_scalar(
            "SELECT source_id FROM composition_project_sources \
             WHERE project_id = ? ORDER BY source_order",
        )
        .bind(&first.id)
        .fetch_all(db.pool())
        .await
        .unwrap();
        assert_eq!(indexed, ["source-b", "source-c"]);
        assert_eq!(updated.document["sources"].as_object().unwrap().len(), 2);

        assert!(db.delete_composition_project(&first.id).await.unwrap());
        assert!(!db.delete_composition_project(&first.id).await.unwrap());
        assert!(db
            .get_composition_project(&first.id)
            .await
            .unwrap()
            .is_none());
        let indexed_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM composition_project_sources WHERE project_id = ?",
        )
        .bind(&first.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(indexed_count, 0);
    }

    #[tokio::test]
    async fn composition_projects_do_not_change_legacy_project_semantics() {
        let (db, _directory) = db().await;
        let legacy = db
            .upsert_project("shared-source", "Legacy", &json!({}), &json!({}))
            .await
            .unwrap();
        let composition = db
            .create_composition_project(
                "Composition",
                &json!({
                    "schemaVersion": 1,
                    "sources": {"shared-source": {"id": "shared-source"}}
                }),
                &["shared-source".into()],
            )
            .await
            .unwrap();

        assert_ne!(legacy.id, composition.id);
        let legacy_projects = db.list_projects().await.unwrap();
        assert_eq!(legacy_projects.len(), 1);
        assert_eq!(legacy_projects[0].id, legacy.id);
        let composition_projects = db.list_composition_projects().await.unwrap();
        assert_eq!(composition_projects, [composition]);
    }

    #[tokio::test]
    async fn source_reference_count_tracks_project_updates_and_deletes() {
        let (db, _directory) = db().await;
        let project = db
            .create_composition_project(
                "References",
                &json!({
                    "schemaVersion": 1,
                    "sources": {"source-a": {"id": "source-a"}}
                }),
                &["source-a".into()],
            )
            .await
            .unwrap();
        assert_eq!(
            db.composition_source_reference_count("source-a")
                .await
                .unwrap(),
            1
        );

        db.update_composition_project(
            &project.id,
            "References",
            &json!({
                "schemaVersion": 1,
                "sources": {"source-b": {"id": "source-b"}}
            }),
            &["source-b".into()],
        )
        .await
        .unwrap();
        assert_eq!(
            db.composition_source_reference_count("source-a")
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            db.composition_source_reference_count("source-b")
                .await
                .unwrap(),
            1
        );
        db.delete_composition_project(&project.id).await.unwrap();
        assert_eq!(
            db.composition_source_reference_count("source-b")
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn project_and_source_index_persist_across_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let project = {
            let db = Db::open(directory.path()).await.unwrap();
            db.create_composition_project(
                "Durable",
                &json!({
                    "schemaVersion": 1,
                    "sources": {"durable-source": {"id": "durable-source"}},
                    "revision": 7
                }),
                &["durable-source".into()],
            )
            .await
            .unwrap()
        };

        let reopened = Db::open(directory.path()).await.unwrap();
        let loaded = reopened
            .get_composition_project(&project.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded, project);
        assert_eq!(loaded.document["revision"], 7);
        assert_eq!(loaded.source_ids, ["durable-source"]);
    }

    #[tokio::test]
    async fn mismatched_source_index_is_rejected_without_changing_the_project() {
        let (db, _directory) = db().await;
        let original = db
            .create_composition_project(
                "Original",
                &json!({
                    "schemaVersion": 1,
                    "sources": {"source-old": {"id": "source-old"}},
                    "revision": 1
                }),
                &["source-old".into()],
            )
            .await
            .unwrap();

        let result = db
            .update_composition_project(
                &original.id,
                "Broken update",
                &json!({
                    "schemaVersion": 1,
                    "sources": {"source-new": {"id": "source-new"}},
                    "revision": 2
                }),
                &["source-old".into()],
            )
            .await;
        assert!(result.is_err());
        assert_eq!(
            db.get_composition_project(&original.id)
                .await
                .unwrap()
                .unwrap(),
            original
        );
    }

    #[test]
    fn source_ids_are_bounded_tokens_not_path_components() {
        for valid in ["source-1", "asset_name", "clip.v2"] {
            assert!(valid_source_id(valid));
        }
        for invalid in ["", ".", "..", "../escape", "source/child", "white space"] {
            assert!(!valid_source_id(invalid), "{invalid:?} must be rejected");
        }
    }
}
