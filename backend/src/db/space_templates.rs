use anyhow::{ensure, Result};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use super::Db;
use crate::library::now_secs;

pub const MAX_SPACE_TEMPLATE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_SPACE_TEMPLATES: i64 = 128;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceTemplate {
    pub id: String,
    pub space_id: String,
    pub template: Value,
    pub created_by: String,
    pub revision: u64,
    pub created_at: u64,
    pub updated_at: u64,
}

pub(super) async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS collaboration_space_templates (id TEXT PRIMARY KEY, space_id TEXT NOT NULL, template_json TEXT NOT NULL, created_by TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, FOREIGN KEY(space_id) REFERENCES collaboration_spaces(id) ON DELETE CASCADE); \
         CREATE INDEX IF NOT EXISTS idx_space_templates_space_updated ON collaboration_space_templates(space_id, updated_at DESC, id);",
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    pub async fn list_space_templates_for(
        &self,
        space_id: &str,
        actor: &str,
    ) -> Result<Vec<SpaceTemplate>> {
        ensure!(
            self.is_space_member(space_id, actor).await?,
            "actor is not a space member"
        );
        let rows = sqlx::query("SELECT id, space_id, template_json, created_by, revision, created_at, updated_at FROM collaboration_space_templates WHERE space_id = ? ORDER BY updated_at DESC, id")
            .bind(space_id)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(space_template_from_row).collect()
    }

    pub async fn create_space_template(
        &self,
        space_id: &str,
        actor: &str,
        template: &Value,
    ) -> Result<Option<SpaceTemplate>> {
        if !self.can_edit_space(space_id, actor).await? {
            return Ok(None);
        }
        let template_json = validate_template(template)?;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM collaboration_space_templates WHERE space_id = ?",
        )
        .bind(space_id)
        .fetch_one(&self.pool)
        .await?;
        ensure!(count < MAX_SPACE_TEMPLATES, "space template quota exceeded");
        let id = Uuid::new_v4().to_string();
        let now = now_secs();
        sqlx::query("INSERT INTO collaboration_space_templates (id, space_id, template_json, created_by, revision, created_at, updated_at) VALUES (?, ?, ?, ?, 1, ?, ?)")
            .bind(&id)
            .bind(space_id)
            .bind(template_json)
            .bind(actor)
            .bind(i64::try_from(now)?)
            .bind(i64::try_from(now)?)
            .execute(&self.pool)
            .await?;
        Ok(Some(SpaceTemplate {
            id,
            space_id: space_id.into(),
            template: template.clone(),
            created_by: actor.into(),
            revision: 1,
            created_at: now,
            updated_at: now,
        }))
    }

    pub async fn update_space_template(
        &self,
        space_id: &str,
        template_id: &str,
        actor: &str,
        base_revision: u64,
        template: &Value,
    ) -> Result<Option<SpaceTemplate>> {
        if !self.can_edit_space(space_id, actor).await? {
            return Ok(None);
        }
        ensure!(base_revision > 0, "invalid template revision");
        let template_json = validate_template(template)?;
        let now = now_secs();
        let result = sqlx::query("UPDATE collaboration_space_templates SET template_json = ?, revision = revision + 1, updated_at = ? WHERE id = ? AND space_id = ? AND revision = ?")
            .bind(template_json)
            .bind(i64::try_from(now)?)
            .bind(template_id)
            .bind(space_id)
            .bind(i64::try_from(base_revision)?)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            let current: Option<i64> = sqlx::query_scalar(
                "SELECT revision FROM collaboration_space_templates WHERE id = ? AND space_id = ?",
            )
            .bind(template_id)
            .bind(space_id)
            .fetch_optional(&self.pool)
            .await?;
            if current.is_some() {
                anyhow::bail!("space template revision conflict");
            }
            return Ok(None);
        }
        self.get_space_template(space_id, template_id)
            .await
            .map(Some)
    }

    pub async fn delete_space_template(
        &self,
        space_id: &str,
        template_id: &str,
        actor: &str,
    ) -> Result<bool> {
        if !self.can_edit_space(space_id, actor).await? {
            return Ok(false);
        }
        let result =
            sqlx::query("DELETE FROM collaboration_space_templates WHERE id = ? AND space_id = ?")
                .bind(template_id)
                .bind(space_id)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() == 1)
    }

    pub(crate) async fn can_edit_space(&self, space_id: &str, actor: &str) -> Result<bool> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM collaboration_space_members WHERE space_id = ? AND actor = ?",
        )
        .bind(space_id)
        .bind(actor)
        .fetch_optional(&self.pool)
        .await?;
        Ok(matches!(role.as_deref(), Some("owner" | "editor")))
    }

    async fn get_space_template(&self, space_id: &str, template_id: &str) -> Result<SpaceTemplate> {
        let row = sqlx::query("SELECT id, space_id, template_json, created_by, revision, created_at, updated_at FROM collaboration_space_templates WHERE id = ? AND space_id = ?")
            .bind(template_id)
            .bind(space_id)
            .fetch_one(&self.pool)
            .await?;
        space_template_from_row(row)
    }
}

fn validate_template(template: &Value) -> Result<String> {
    let object = template
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("invalid space template"))?;
    ensure!(
        object.get("schemaVersion").and_then(Value::as_u64) == Some(1),
        "invalid space template"
    );
    let id = object.get("id").and_then(Value::as_str).unwrap_or_default();
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    ensure!(
        !id.is_empty() && id.len() <= 128 && !name.is_empty() && name.len() <= 128,
        "invalid space template"
    );
    ensure!(
        object.get("composition").is_some_and(Value::is_object),
        "invalid space template"
    );
    ensure!(
        object.get("slots").is_some_and(Value::is_array),
        "invalid space template"
    );
    let serialized = serde_json::to_string(template)?;
    ensure!(
        serialized.len() <= MAX_SPACE_TEMPLATE_BYTES,
        "space template exceeds size limit"
    );
    Ok(serialized)
}

fn space_template_from_row(row: sqlx::sqlite::SqliteRow) -> Result<SpaceTemplate> {
    Ok(SpaceTemplate {
        id: row.try_get("id")?,
        space_id: row.try_get("space_id")?,
        template: serde_json::from_str(&row.try_get::<String, _>("template_json")?)?,
        created_by: row.try_get("created_by")?,
        revision: u64::try_from(row.try_get::<i64, _>("revision")?)?,
        created_at: u64::try_from(row.try_get::<i64, _>("created_at")?)?,
        updated_at: u64::try_from(row.try_get::<i64, _>("updated_at")?)?,
    })
}
