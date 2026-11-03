use std::collections::{BTreeMap, BTreeSet};

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
pub const MAX_LUT_PROJECT_DOCUMENT_BYTES: usize = 96 * 1024 * 1024;
/// Authored video strokes/portable tracks have a bounded independent allowance.
pub const MAX_VIDEO_CUTOUT_SETTINGS_BYTES:usize=16*1024*1024;
pub const MAX_SPHERICAL_SETTINGS_BYTES:usize=16*1024*1024;
pub const MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES:usize=MAX_LUT_PROJECT_DOCUMENT_BYTES+MAX_VIDEO_CUTOUT_SETTINGS_BYTES+MAX_SPHERICAL_SETTINGS_BYTES;
/// Ordinary project metadata retains 2MiB; only validated LUT/font/cutout/spherical tracks add allowance.
pub fn composition_document_byte_limit(document: &Value) -> Result<usize> {
    let mut limit=MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES;
    if document.get("compositions").is_some() {
        if let Some(luts)=document.get("luts").and_then(Value::as_array).filter(|luts|!luts.is_empty()){
            limit=limit.saturating_add(serde_json::to_vec(luts)?.len()).min(MAX_LUT_PROJECT_DOCUMENT_BYTES);
        }
    }
    if document.get("compositions").is_some(){
        if let Some(value)=document.get("fonts").filter(|value|!value.is_null()){
            let fonts:Vec<crate::domain::concat_project_fonts::FontResource>=serde_json::from_value(value.clone())?;
            crate::domain::concat_project_fonts::bank(&fonts)?;
            limit=limit.saturating_add(serde_json::to_vec(value)?.len()).min(MAX_LUT_PROJECT_DOCUMENT_BYTES);
        }
    }
    let contents:Vec<&Value>=if let Some(nodes)=document.get("compositions").and_then(Value::as_object){nodes.values().filter_map(|node|node.get("content")).collect()}else{vec![document]};
    let mut cutout_bytes=0usize;
    let mut spherical_bytes=0usize;
    for content in contents{
        if let Some(tracks)=content.get("tracks").and_then(Value::as_array){for track in tracks{
            if track.get("kind").and_then(Value::as_str)!=Some("video"){continue}
            if let Some(clips)=track.get("clips").and_then(Value::as_array){for clip in clips{
                if let Some(effects)=clip.get("effects").and_then(Value::as_array){for effect in effects{
                    if effect.get("kind").and_then(Value::as_str)!=Some("spherical_video"){continue}
                    if let Some(parameters)=effect.get("parameters"){
                        let spherical:crate::domain::spherical_video::SphericalVideo=serde_json::from_value(parameters.clone())?;spherical.validate()?;
                        for samples in [parameters.get("orientation"),parameters.get("rollingShutter").and_then(|v|v.get("captureClock"))].into_iter().flatten(){
                            spherical_bytes=spherical_bytes.checked_add(serde_json::to_vec(samples)?.len()).context("spherical settings byte count overflow")?;
                            ensure!(spherical_bytes<=MAX_SPHERICAL_SETTINGS_BYTES,"spherical timing tracks exceed saved document budget");
                        }
                    }
                }}
                if let Some(value)=clip.get("concatCutout").filter(|v|!v.is_null()){
                    let binding:crate::domain::concat_video_binding::Binding=serde_json::from_value(value.clone())?;binding.validate()?;
                    cutout_bytes=cutout_bytes.checked_add(serde_json::to_vec(value)?.len()).context("video cutout byte count overflow")?;
                    ensure!(cutout_bytes<=MAX_VIDEO_CUTOUT_SETTINGS_BYTES,"video cutout settings exceed saved document budget");
                }
            }}
        }}
    }
    Ok(limit.saturating_add(cutout_bytes).saturating_add(spherical_bytes).min(MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES))
}

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
    pub(crate) async fn create_composition_project_adopting_video(&self,name:&str,document:&Value,source_ids:&[String],owner:&str,space:Option<&str>,transfers:&[crate::handlers::concat_video_cache::PreparedTransfer])->Result<CompositionProject>{
        if let Some(space)=space {
            let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM collaboration_space_members WHERE space_id=? AND actor=? AND role IN ('owner','editor'))").bind(space).bind(owner).fetch_one(&self.pool).await?;
            ensure!(allowed,"space role cannot create project");
            self.create_composition_project_with_space(name,document,source_ids,owner,space,transfers).await
        }else{self.create_composition_project_with_owner(name,document,source_ids,Some(owner),transfers).await}
    }
    pub(crate) async fn update_composition_project_adopting_video(&self,id:&str,actor:&str,name:&str,document:&Value,source_ids:&[String],revision:u64,transfers:&[crate::handlers::concat_video_cache::PreparedTransfer])->Result<Option<CompositionProject>>{
        self.update_composition_project_at_revision(id,name,document,source_ids,Some(revision),Some(actor),transfers).await
    }

    pub async fn create_composition_project(
        &self,
        name: &str,
        document: &Value,
        source_ids: &[String],
    ) -> Result<CompositionProject> {
        self.create_composition_project_with_owner(name, document, source_ids, None, &[])
            .await
    }

    pub async fn create_owned_composition_project(
        &self,
        name: &str,
        document: &Value,
        source_ids: &[String],
        owner: &str,
    ) -> Result<CompositionProject> {
        self.create_composition_project_with_owner(name, document, source_ids, Some(owner), &[])
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
        self.create_composition_project_with_space(name, document, source_ids, owner, space_id, &[])
            .await
    }

    async fn create_composition_project_with_owner(
        &self,
        name: &str,
        document: &Value,
        source_ids: &[String],
        owner: Option<&str>,
        transfers: &[crate::handlers::concat_video_cache::PreparedTransfer],
    ) -> Result<CompositionProject> {
        let document_json = validate_storage_input(name, document, source_ids)?;
        let id = Uuid::new_v4().to_string();
        let now = now_secs() as i64;
        let mut transaction = self.pool.begin().await?;
        claim_project_sources(&mut transaction, None, source_ids, owner.unwrap_or("project-create"), &id).await?;
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
        for transfer in transfers {transfer.register(&mut transaction,&project.id,project.revision).await?;}
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
        transfers: &[crate::handlers::concat_video_cache::PreparedTransfer],
    ) -> Result<CompositionProject> {
        let document_json = validate_storage_input(name, document, source_ids)?;
        let id = Uuid::new_v4().to_string();
        let now = now_secs() as i64;
        let mut transaction = self.pool.begin().await?;
        claim_project_sources(&mut transaction, Some(space_id), source_ids, owner, &id).await?;
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
        for transfer in transfers {transfer.register(&mut transaction,&project.id,project.revision).await?;}
        let still_allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM collaboration_space_members WHERE space_id=? AND actor=? AND role IN ('owner','editor'))").bind(space_id).bind(owner).fetch_one(&mut *transaction).await?;
        ensure!(still_allowed,"space role cannot create project");
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
            Some(actor),
            &[],
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
        self.update_composition_project_at_revision(id, name, document, source_ids, None, None, &[])
            .await
    }

    async fn update_composition_project_at_revision(
        &self,
        id: &str,
        name: &str,
        document: &Value,
        source_ids: &[String],
        expected_revision: Option<u64>,
        editor: Option<&str>,
        transfers: &[crate::handlers::concat_video_cache::PreparedTransfer],
    ) -> Result<Option<CompositionProject>> {
        let document_json = validate_storage_input(name, document, source_ids)?;
        let now = now_secs() as i64;
        let mut transaction = self.pool.begin().await?;
        if let Some(actor)=editor{
            let role:Option<String>=sqlx::query_scalar("SELECT role FROM composition_project_members WHERE project_id=? AND actor=?").bind(id).bind(actor).fetch_optional(&mut *transaction).await?;
            ensure!(matches!(role.as_deref(),Some("owner"|"editor")),"role cannot edit composition project");
        }
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
                id,
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
        for transfer in transfers {transfer.register(&mut transaction,&project.id,project.revision).await?;}
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
        // Historical temporal reservations remain live ownership after Undo
        // removes a region from the current bin. Count projects, not rows.
        sqlx::query_scalar("SELECT COUNT(DISTINCT project_id) FROM (SELECT project_id FROM composition_project_sources WHERE source_id=? UNION SELECT r.project_id FROM concat_video_project_region_refs r JOIN composition_projects p ON p.id=r.project_id WHERE r.library_source_id=? UNION SELECT r.project_id FROM person_video_project_resources r JOIN composition_projects p ON p.id=r.project_id WHERE r.source_id=? AND r.role='sam-video-regions' UNION SELECT r.project_id FROM concat_video_project_source_refs r JOIN composition_projects p ON p.id=r.project_id WHERE r.library_source_id=?)")
            .bind(source_id)
            .bind(source_id)
            .bind(source_id)
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
    project_id: &str,
) -> Result<()> {
    for source_id in source_ids {
        let retired: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM composition_retired_media WHERE source_id=?)")
            .bind(source_id).fetch_one(&mut **transaction).await?;
        ensure!(!retired, "generated source has been retired");
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
            (None, Some(space)) => {
                let own_narration:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM composition_narration_spaces WHERE project_id=? AND space_id=?)").bind(project_id).bind(space).fetch_one(&mut **transaction).await?;
                ensure!(own_narration,"scoped source cannot be used by an unscoped project");
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
        document_json.len() <= composition_document_byte_limit(document)?,
        "composition project document is too large"
    );
    Ok(document_json)
}

pub(crate) fn extract_document_source_ids(document: &Value) -> Result<Vec<String>> {
    if document.get("compositions").is_some() {
        let project: crate::domain::nested_composition::NestedCompositionProject = serde_json::from_value(document.clone())?;
        project.dependency_order()?;
        let mut sources = BTreeMap::new();
        for composition in project.compositions.values() {
            for (id, source) in &composition.content.sources {
                let id = id.as_str().to_owned();
                ensure!(valid_source_id(&id), "invalid nested composition source id");
                if let Some(existing) = sources.insert(id, source) {
                    ensure!(existing == source, "conflicting nested source metadata");
                }
            }
        }
        for (id, entry) in &project.project_media {
            ensure!(valid_source_id(id.as_str()), "invalid project media source id");
            // A live source table is authoritative; the bin retains an unused
            // source descriptor so its ownership survives the last placement.
            sources.entry(id.as_str().to_owned()).or_insert(&entry.source);
        }
        ensure!(sources.len() <= MAX_COMPOSITION_PROJECT_SOURCES, "nested project has too many sources");
        return Ok(sources.keys().map(|id| {
            let logical=crate::domain::composition::SourceId::parse(id).expect("validated source ID");
            project.library_source_id(&logical).as_str().to_owned()
        }).collect::<BTreeSet<_>>().into_iter().collect());
    }

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
    async fn project_media_without_a_placement_remains_in_storage_reference_inventory() {
        let (db,_directory)=db().await;
        let content=crate::domain::composition::Composition::new(crate::domain::composition::CanvasSpec::default());
        let document=json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":content,"instances":[]}},"projectMedia":{"retained":{"source":{"id":"retained","kind":"image","durationTicks":0,"width":16,"height":16,"hasAudio":false},"name":"Retained slot","placeholder":true}}});
        assert_eq!(extract_document_source_ids(&document).unwrap(),vec!["retained"]);
        assert!(db.create_composition_project("Missing source inventory",&document,&[]).await.is_err());
        let saved=db.create_composition_project("Slot",&document,&["retained".into()]).await.unwrap();
        let restored=db.get_composition_project(&saved.id).await.unwrap().unwrap();
        assert_eq!(restored.document,document);assert_eq!(restored.source_ids,vec!["retained"]);
        assert_eq!(db.composition_source_reference_count("retained").await.unwrap(),1);
    }

    #[tokio::test]
    async fn nested_document_storage_roundtrips_and_rejects_dangling_root() {
        let (db, _directory) = db().await;
        let content = crate::domain::composition::Composition::new(crate::domain::composition::CanvasSpec::default());
        let document = json!({ "schemaVersion": 1, "root": "root", "compositions": { "root": { "content": content, "instances": [] } } });
        let saved = db.create_composition_project("Nested", &document, &[]).await.unwrap();
        let restored = db.get_composition_project(&saved.id).await.unwrap().unwrap();
        assert_eq!(restored.document, document);
        assert!(restored.source_ids.is_empty());
        let mut invalid = document;
        invalid["root"] = json!("missing");
        assert!(db.create_composition_project("Invalid", &invalid, &[]).await.is_err());
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
    async fn cached_original_retention_outlives_current_source_inventory(){
        let (db,_dir)=db().await;
        let mut document=serde_json::to_value(crate::domain::composition::Composition::new(Default::default())).unwrap();
        document["sources"]=serde_json::json!({"raw":{"id":"raw","kind":"video","durationTicks":100000,"width":2,"height":2,"hasAudio":false}});
        let first=db.create_owned_composition_project("Original",&document,&["raw".into()],"owner").await.unwrap();
        for hash in ["first","second"]{sqlx::query("INSERT INTO concat_video_project_source_refs VALUES(?,?,?,?)").bind(&first.id).bind("raw").bind(hash).bind("raw").execute(&db.pool).await.unwrap();}
        assert_eq!(db.composition_source_reference_count("raw").await.unwrap(),1);
        let empty=serde_json::to_value(crate::domain::composition::Composition::new(Default::default())).unwrap();
        db.update_composition_project_for(&first.id,"owner",&first.name,&empty,&[],first.revision).await.unwrap().unwrap();
        assert_eq!(db.composition_source_reference_count("raw").await.unwrap(),1);
        let second=db.create_owned_composition_project("Other history",&empty,&[],"owner").await.unwrap();
        sqlx::query("INSERT INTO concat_video_project_source_refs VALUES(?,?,?,?)").bind(&second.id).bind("slot").bind("first").bind("raw").execute(&db.pool).await.unwrap();
        assert_eq!(db.composition_source_reference_count("raw").await.unwrap(),2);
        db.delete_composition_project(&first.id).await.unwrap();assert_eq!(db.composition_source_reference_count("raw").await.unwrap(),1);
        db.delete_composition_project(&second.id).await.unwrap();assert_eq!(db.composition_source_reference_count("raw").await.unwrap(),0);
    }

    #[tokio::test]
    async fn temporal_history_counts_owners_without_double_counting_or_stale_legacy_rows(){
        let (db,_dir)=db().await;
        let document=serde_json::to_value(crate::domain::composition::Composition::new(Default::default())).unwrap();
        let first=db.create_owned_composition_project("First",&document,&[],"owner").await.unwrap();
        let second=db.create_owned_composition_project("Second",&document,&[],"owner").await.unwrap();
        for project in [&first.id,&second.id]{
            for hash in ["a","b"]{sqlx::query("INSERT INTO concat_video_project_region_refs(project_id,source_id,video_hash,mask_source_id,library_source_id,file_hash) VALUES(?,?,?,?,?,?)").bind(project).bind("slot").bind("video-hash").bind("mask-slot").bind("mask-file").bind(hash).execute(&db.pool).await.unwrap();}
        }
        sqlx::query("INSERT INTO person_video_project_resources(project_id,role,file_hash,source_id) VALUES(?,?,?,?)").bind(&first.id).bind("sam-video-regions").bind("legacy").bind("mask-file").execute(&db.pool).await.unwrap();
        assert_eq!(db.composition_source_reference_count("mask-file").await.unwrap(),2);
        db.delete_composition_project(&first.id).await.unwrap();assert_eq!(db.composition_source_reference_count("mask-file").await.unwrap(),1);
        db.delete_composition_project(&second.id).await.unwrap();assert_eq!(db.composition_source_reference_count("mask-file").await.unwrap(),0);
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
