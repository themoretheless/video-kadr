//! Generated narration reuses Space media ACLs, including existing file/search/archive gates.
use super::Db;
use anyhow::{ensure, Result};
use sqlx::SqlitePool;
pub(super) async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query("CREATE TABLE IF NOT EXISTS composition_narration_spaces(project_id TEXT PRIMARY KEY REFERENCES composition_projects(id) ON DELETE CASCADE,space_id TEXT NOT NULL UNIQUE REFERENCES collaboration_spaces(id) ON DELETE CASCADE);
 CREATE TRIGGER IF NOT EXISTS narration_project_delete BEFORE DELETE ON composition_projects BEGIN
 DELETE FROM collaboration_space_members WHERE role!='owner' AND space_id IN(SELECT space_id FROM composition_narration_spaces WHERE project_id=OLD.id); END;
 CREATE TRIGGER IF NOT EXISTS narration_member_insert AFTER INSERT ON composition_project_members BEGIN
 INSERT INTO collaboration_space_members(space_id,actor,role,created_at,updated_at) SELECT space_id,NEW.actor,CASE WHEN NEW.role IN ('owner','editor') THEN NEW.role ELSE 'viewer' END,NEW.created_at,NEW.updated_at FROM composition_narration_spaces WHERE project_id=NEW.project_id ON CONFLICT(space_id,actor) DO UPDATE SET role=excluded.role,updated_at=excluded.updated_at; END;
 CREATE TRIGGER IF NOT EXISTS narration_member_update AFTER UPDATE OF role ON composition_project_members BEGIN
 UPDATE collaboration_space_members SET role=CASE WHEN NEW.role IN ('owner','editor') THEN NEW.role ELSE 'viewer' END,updated_at=NEW.updated_at WHERE actor=NEW.actor AND space_id IN(SELECT space_id FROM composition_narration_spaces WHERE project_id=NEW.project_id); END;
 CREATE TRIGGER IF NOT EXISTS narration_member_delete AFTER DELETE ON composition_project_members BEGIN
 DELETE FROM collaboration_space_members WHERE actor=OLD.actor AND role!='owner' AND space_id IN(SELECT space_id FROM composition_narration_spaces WHERE project_id=OLD.project_id); END;").execute(pool).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS composition_retired_media(source_id TEXT PRIMARY KEY, retired_at INTEGER NOT NULL)").execute(pool).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS composition_freeze_publications(source_id TEXT PRIMARY KEY,space_id TEXT NOT NULL,project_id TEXT NOT NULL,created_at INTEGER NOT NULL)").execute(pool).await?;
    Ok(())
}
impl Db {
    /// Atomically prevent future project references before deleting an unused generated still.
    /// The first statement takes SQLite's writer lock, serializing with project saves.
    pub async fn retire_unused_freeze_media(&self, source: &str) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT OR IGNORE INTO composition_retired_media(source_id,retired_at) SELECT ?,? WHERE NOT EXISTS(SELECT 1 FROM composition_project_sources WHERE source_id=?)")
            .bind(source).bind(crate::library::now_secs() as i64).bind(source)
            .execute(&mut *tx).await?;
        let retired: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM composition_retired_media WHERE source_id=?)")
            .bind(source).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(retired)
    }
    /// Reusable private project-media claim, including retry of a reserved source ID.
    pub async fn claim_project_generated_media(&self,project:&str,actor:&str,source:&str)->Result<String>{
        if let Some(space)=self.source_space_id(source).await?{
            let role:Option<String>=sqlx::query_scalar("SELECT role FROM composition_project_members WHERE project_id=? AND actor=?").bind(project).bind(actor).fetch_optional(&self.pool).await?;
            ensure!(matches!(role.as_deref(),Some("owner"|"editor")),"project media requires editor access");
            let expected:Option<String>=sqlx::query_scalar::<_,Option<String>>("SELECT COALESCE(p.space_id,n.space_id) FROM composition_projects p LEFT JOIN composition_narration_spaces n ON n.project_id=p.id WHERE p.id=?").bind(project).fetch_optional(&self.pool).await?.flatten();
            ensure!(expected.as_deref()==Some(&space),"reserved source belongs to another project space");return Ok(space)
        }
        self.claim_project_narration(project,actor,source).await
    }
    /// Claim only a fresh host-generated source ID; project document edits remain explicit/undoable.
    pub async fn claim_project_narration(
        &self,
        project: &str,
        actor: &str,
        source: &str,
    ) -> Result<String> {
        self.claim_project_media_inner(project,actor,source,None).await
    }
    /// Fresh generated media may only reserve storage for the captured revision.
    pub async fn claim_project_freeze_media(&self,project:&str,actor:&str,source:&str,revision:u64)->Result<String>{
        self.claim_project_media_inner(project,actor,source,Some(revision)).await
    }
    async fn claim_project_media_inner(&self,project:&str,actor:&str,source:&str,revision:Option<u64>)->Result<String>{
        let mut tx = self.pool.begin().await?;
        if let Some(expected)=revision {
            ensure!(uuid::Uuid::parse_str(source)?.to_string()==source,"invalid freeze source ID");
            let actual:i64=sqlx::query_scalar("SELECT revision FROM composition_projects WHERE id=?").bind(project).fetch_one(&mut *tx).await?;
            ensure!(u64::try_from(actual)?==expected,"stale freeze project revision");
        }
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM composition_project_members WHERE project_id=? AND actor=?",
        )
        .bind(project)
        .bind(actor)
        .fetch_optional(&mut *tx)
        .await?;
        ensure!(
            matches!(role.as_deref(), Some("owner" | "editor")),
            "project narration requires editor access"
        );
        let space: Option<String> =
            sqlx::query_scalar("SELECT space_id FROM composition_projects WHERE id=?")
                .bind(project)
                .fetch_one(&mut *tx)
                .await?;
        let space = if let Some(space) = space {
            space
        } else {
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT space_id FROM composition_narration_spaces WHERE project_id=?",
            )
            .bind(project)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(space) = existing {
                space
            } else {
                let space = uuid::Uuid::new_v4().to_string();
                let now = crate::library::now_secs() as i64;
                sqlx::query("INSERT INTO collaboration_spaces(id,name,created_at,updated_at) VALUES(?,?,?,?)").bind(&space).bind(if revision.is_some(){"Ресурсы проекта"}else{"Озвучка проекта"}).bind(now).bind(now).execute(&mut *tx).await?;
                sqlx::query("INSERT INTO collaboration_space_members(space_id,actor,role,created_at,updated_at) SELECT ?,actor,CASE WHEN role IN ('owner','editor') THEN role ELSE 'viewer' END,created_at,updated_at FROM composition_project_members WHERE project_id=?").bind(&space).bind(project).execute(&mut *tx).await?;
                sqlx::query(
                    "INSERT INTO composition_narration_spaces(project_id,space_id) VALUES(?,?)",
                )
                .bind(project)
                .bind(&space)
                .execute(&mut *tx)
                .await?;
                space
            }
        };
        sqlx::query("INSERT INTO collaboration_space_media(space_id,source_id,created_by,created_at) VALUES(?,?,?,?)").bind(&space).bind(source).bind(actor).bind(crate::library::now_secs() as i64).execute(&mut *tx).await?;
        if revision.is_some() {
            sqlx::query("INSERT INTO composition_freeze_publications(source_id,space_id,project_id,created_at) VALUES(?,?,?,?)")
                .bind(source).bind(&space).bind(project).bind(crate::library::now_secs() as i64).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(space)
    }
}
