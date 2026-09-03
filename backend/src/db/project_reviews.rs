use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use super::Db;
use crate::domain::project_collaboration::{
    ProjectMember, ProjectRole, ReviewAuditEvent, ReviewComment, ReviewShareCreated,
    ReviewShareGrant, ReviewThread, SharedReview,
};
use crate::library::now_secs;

pub(super) async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS composition_project_members ( \
           project_id TEXT NOT NULL, \
           actor TEXT NOT NULL, \
           role TEXT NOT NULL CHECK (role IN ('owner', 'editor', 'commenter', 'viewer')), \
           created_at INTEGER NOT NULL, \
           updated_at INTEGER NOT NULL, \
           PRIMARY KEY (project_id, actor), \
           FOREIGN KEY (project_id) REFERENCES composition_projects(id) ON DELETE CASCADE \
         )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS project_review_threads ( \
           id TEXT PRIMARY KEY, \
           project_id TEXT NOT NULL, \
           timeline_tick INTEGER NOT NULL CHECK (timeline_tick >= 0), \
           resolved_at INTEGER, \
           resolved_by TEXT, \
           created_at INTEGER NOT NULL, \
           updated_at INTEGER NOT NULL, \
           FOREIGN KEY (project_id) REFERENCES composition_projects(id) ON DELETE CASCADE \
         )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_project_review_threads_project \
         ON project_review_threads(project_id, created_at, id)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS project_review_comments ( \
           id TEXT PRIMARY KEY, \
           thread_id TEXT NOT NULL, \
           author TEXT NOT NULL, \
           body TEXT NOT NULL, \
           timeline_tick INTEGER NOT NULL CHECK (timeline_tick >= 0), \
           created_at INTEGER NOT NULL, \
           comment_order INTEGER NOT NULL, \
           UNIQUE (thread_id, comment_order), \
           FOREIGN KEY (thread_id) REFERENCES project_review_threads(id) ON DELETE CASCADE \
         )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS project_review_audit ( \
           id TEXT PRIMARY KEY, project_id TEXT NOT NULL, actor TEXT NOT NULL, \
           action TEXT NOT NULL, subject_id TEXT NOT NULL, created_at INTEGER NOT NULL, \
           event_order INTEGER NOT NULL, \
           FOREIGN KEY (project_id) REFERENCES composition_projects(id) ON DELETE CASCADE \
         )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_project_review_audit_project \
         ON project_review_audit(project_id, event_order)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS project_review_shares ( \
           id TEXT PRIMARY KEY, project_id TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE, \
           expires_at INTEGER NOT NULL, revoked_at INTEGER, created_by TEXT NOT NULL, \
           created_at INTEGER NOT NULL, \
           FOREIGN KEY (project_id) REFERENCES composition_projects(id) ON DELETE CASCADE \
         )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    pub async fn list_review_threads_for(
        &self,
        project_id: &str,
        actor: &str,
    ) -> Result<Vec<ReviewThread>> {
        self.require_review_member(project_id, actor).await?;
        self.list_review_threads(project_id).await
    }

    pub async fn create_review_thread(
        &self,
        project_id: &str,
        author: &str,
        body: &str,
        timeline_tick: u64,
    ) -> Result<Option<ReviewThread>> {
        if self.get_composition_project(project_id).await?.is_none() {
            return Ok(None);
        }
        let role = self
            .review_role(project_id, author, true)
            .await?
            .ok_or_else(|| anyhow::anyhow!("actor is not a project member"))?;
        ensure!(role.can_comment(), "role cannot comment");
        let now = now_secs();
        let comment = ReviewComment {
            id: Uuid::new_v4().to_string(),
            author: author.into(),
            body: body.into(),
            timeline_tick,
            parent_id: None,
            created_at: now,
        };
        let thread =
            ReviewThread::new(project_id.into(), comment.clone()).map_err(anyhow::Error::msg)?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO project_review_threads \
             (id, project_id, timeline_tick, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&thread.id)
        .bind(project_id)
        .bind(i64::try_from(timeline_tick)?)
        .bind(i64::try_from(now)?)
        .bind(i64::try_from(now)?)
        .execute(&mut *transaction)
        .await?;
        insert_comment(&mut transaction, &thread.id, &comment, 0).await?;
        insert_audit(
            &mut transaction,
            project_id,
            author,
            "thread.created",
            &thread.id,
            now,
        )
        .await?;
        transaction.commit().await?;
        Ok(Some(thread))
    }

    pub async fn list_review_threads(&self, project_id: &str) -> Result<Vec<ReviewThread>> {
        let rows = sqlx::query(
            "SELECT id, project_id, resolved_at, resolved_by FROM project_review_threads \
             WHERE project_id = ? ORDER BY created_at, id",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        let mut threads = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.try_get("id")?;
            threads.push(ReviewThread {
                comments: load_comments(&self.pool, &id).await?,
                id,
                project_id: row.try_get("project_id")?,
                resolved_at: row
                    .try_get::<Option<i64>, _>("resolved_at")?
                    .map(u64::try_from)
                    .transpose()?,
                resolved_by: row.try_get("resolved_by")?,
            });
        }
        Ok(threads)
    }

    pub async fn reply_to_review_thread(
        &self,
        thread_id: &str,
        author: &str,
        body: &str,
    ) -> Result<Option<ReviewThread>> {
        let Some(mut thread) = self.get_review_thread(thread_id).await? else {
            return Ok(None);
        };
        let role = self
            .review_role(&thread.project_id, author, false)
            .await?
            .ok_or_else(|| anyhow::anyhow!("actor is not a project member"))?;
        let root_tick = thread.comments[0].timeline_tick;
        let comment = ReviewComment {
            id: Uuid::new_v4().to_string(),
            author: author.into(),
            body: body.into(),
            timeline_tick: root_tick,
            parent_id: Some(thread.id.clone()),
            created_at: now_secs(),
        };
        thread
            .reply(role, comment.clone())
            .map_err(anyhow::Error::msg)?;
        let mut transaction = self.pool.begin().await?;
        let updated = sqlx::query(
            "UPDATE project_review_threads SET updated_at = ? \
             WHERE id = ? AND resolved_at IS NULL",
        )
        .bind(i64::try_from(comment.created_at)?)
        .bind(thread_id)
        .execute(&mut *transaction)
        .await?;
        ensure!(updated.rows_affected() == 1, "review thread is resolved");
        insert_reply(&mut transaction, thread_id, &comment).await?;
        insert_audit(
            &mut transaction,
            &thread.project_id,
            author,
            "comment.replied",
            &comment.id,
            comment.created_at,
        )
        .await?;
        transaction.commit().await?;
        Ok(Some(thread))
    }

    pub async fn resolve_review_thread(
        &self,
        thread_id: &str,
        actor: &str,
        resolved: bool,
    ) -> Result<Option<ReviewThread>> {
        let Some(mut thread) = self.get_review_thread(thread_id).await? else {
            return Ok(None);
        };
        let role = self
            .review_role(&thread.project_id, actor, false)
            .await?
            .ok_or_else(|| anyhow::anyhow!("actor is not a project member"))?;
        let now = now_secs();
        thread
            .set_resolved(role, actor, resolved, now)
            .map_err(anyhow::Error::msg)?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "UPDATE project_review_threads SET resolved_at = ?, resolved_by = ?, updated_at = ? \
             WHERE id = ?",
        )
        .bind(
            thread
                .resolved_at
                .map(|value| i64::try_from(value).unwrap_or(i64::MAX)),
        )
        .bind(&thread.resolved_by)
        .bind(i64::try_from(now)?)
        .bind(thread_id)
        .execute(&mut *transaction)
        .await?;
        insert_audit(
            &mut transaction,
            &thread.project_id,
            actor,
            if resolved {
                "thread.resolved"
            } else {
                "thread.reopened"
            },
            thread_id,
            now,
        )
        .await?;
        transaction.commit().await?;
        Ok(Some(thread))
    }

    pub async fn list_review_members(&self, project_id: &str) -> Result<Vec<ProjectMember>> {
        let rows = sqlx::query(
            "SELECT actor, role FROM composition_project_members WHERE project_id = ? ORDER BY actor",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let role: String = row.try_get("role")?;
                Ok(ProjectMember {
                    actor: row.try_get("actor")?,
                    role: ProjectRole::try_from(role.as_str()).map_err(anyhow::Error::msg)?,
                })
            })
            .collect()
    }

    pub async fn list_review_members_for(
        &self,
        project_id: &str,
        actor: &str,
    ) -> Result<Vec<ProjectMember>> {
        self.require_review_member(project_id, actor).await?;
        self.list_review_members(project_id).await
    }

    pub async fn set_review_member(
        &self,
        project_id: &str,
        requester: &str,
        actor: &str,
        role: ProjectRole,
    ) -> Result<Option<ProjectMember>> {
        let Some(requester_role) = self.review_role(project_id, requester, false).await? else {
            return Ok(None);
        };
        ensure!(
            requester_role.can_manage_members(),
            "role cannot manage project members"
        );
        ensure!(valid_actor(actor), "invalid project member");
        ensure!(
            role != ProjectRole::Owner || requester == actor,
            "use ownership transfer to assign owner"
        );
        ensure!(
            requester != actor || role == ProjectRole::Owner,
            "owner cannot demote itself"
        );
        let now = i64::try_from(now_secs())?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO composition_project_members (project_id, actor, role, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?) ON CONFLICT(project_id, actor) DO UPDATE SET \
             role = excluded.role, updated_at = excluded.updated_at",
        )
        .bind(project_id)
        .bind(actor)
        .bind(role.as_str())
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        insert_audit(
            &mut transaction,
            project_id,
            requester,
            "member.role_set",
            actor,
            u64::try_from(now)?,
        )
        .await?;
        transaction.commit().await?;
        Ok(Some(ProjectMember {
            actor: actor.into(),
            role,
        }))
    }

    pub async fn transfer_project_ownership(
        &self,
        project_id: &str,
        requester: &str,
        target: &str,
    ) -> Result<Option<ProjectMember>> {
        ensure!(requester != target, "owner already owns project");
        ensure!(valid_actor(target), "invalid project member");
        let now = i64::try_from(now_secs())?;
        let mut transaction = self.pool.begin().await?;
        let requester_role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM composition_project_members WHERE project_id = ? AND actor = ?",
        )
        .bind(project_id)
        .bind(requester)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(requester_role) = requester_role else {
            transaction.rollback().await?;
            return Ok(None);
        };
        ensure!(requester_role == "owner", "role cannot transfer ownership");
        let target_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM composition_project_members WHERE project_id = ? AND actor = ?)",
        )
        .bind(project_id)
        .bind(target)
        .fetch_one(&mut *transaction)
        .await?;
        ensure!(target_exists, "target is not a project member");
        sqlx::query(
            "UPDATE composition_project_members SET role = CASE actor WHEN ? THEN 'owner' ELSE 'editor' END, updated_at = ? WHERE project_id = ? AND actor IN (?, ?)",
        )
        .bind(target)
        .bind(now)
        .bind(project_id)
        .bind(requester)
        .bind(target)
        .execute(&mut *transaction)
        .await?;
        insert_audit(
            &mut transaction,
            project_id,
            requester,
            "ownership.transferred",
            target,
            u64::try_from(now)?,
        )
        .await?;
        transaction.commit().await?;
        Ok(Some(ProjectMember {
            actor: target.into(),
            role: ProjectRole::Owner,
        }))
    }

    pub async fn list_review_audit(&self, project_id: &str) -> Result<Vec<ReviewAuditEvent>> {
        let rows = sqlx::query(
            "SELECT id, project_id, actor, action, subject_id, created_at \
             FROM project_review_audit WHERE project_id = ? ORDER BY event_order",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(ReviewAuditEvent {
                    id: row.try_get("id")?,
                    project_id: row.try_get("project_id")?,
                    actor: row.try_get("actor")?,
                    action: row.try_get("action")?,
                    subject_id: row.try_get("subject_id")?,
                    created_at: u64::try_from(row.try_get::<i64, _>("created_at")?)?,
                })
            })
            .collect()
    }

    pub async fn list_review_audit_for(
        &self,
        project_id: &str,
        actor: &str,
    ) -> Result<Vec<ReviewAuditEvent>> {
        self.require_review_member(project_id, actor).await?;
        self.list_review_audit(project_id).await
    }

    pub async fn create_review_share(
        &self,
        project_id: &str,
        requester: &str,
        ttl_seconds: u64,
    ) -> Result<Option<ReviewShareCreated>> {
        let Some(role) = self.review_role(project_id, requester, false).await? else {
            return Ok(None);
        };
        ensure!(
            role == ProjectRole::Owner,
            "role cannot create review share"
        );
        ensure!(
            (60..=30 * 24 * 60 * 60).contains(&ttl_seconds),
            "invalid review share TTL"
        );
        let now = now_secs();
        let expires_at = now
            .checked_add(ttl_seconds)
            .ok_or_else(|| anyhow::anyhow!("invalid review share TTL"))?;
        let id = Uuid::new_v4().to_string();
        let token = format!("{}.{}", Uuid::new_v4(), Uuid::new_v4());
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO project_review_shares \
             (id, project_id, token_hash, expires_at, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(project_id)
        .bind(hash_share_token(&token))
        .bind(i64::try_from(expires_at)?)
        .bind(requester)
        .bind(i64::try_from(now)?)
        .execute(&mut *transaction)
        .await?;
        insert_audit(
            &mut transaction,
            project_id,
            requester,
            "share.created",
            &id,
            now,
        )
        .await?;
        transaction.commit().await?;
        Ok(Some(ReviewShareCreated {
            grant: ReviewShareGrant {
                id,
                project_id: project_id.into(),
                expires_at,
                revoked_at: None,
            },
            token,
        }))
    }

    pub async fn open_shared_review(&self, token: &str) -> Result<Option<SharedReview>> {
        if token.len() > 128
            || !token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
        {
            return Ok(None);
        }
        let row = sqlx::query(
            "SELECT project_id, expires_at FROM project_review_shares \
             WHERE token_hash = ? AND revoked_at IS NULL AND expires_at > ?",
        )
        .bind(hash_share_token(token))
        .bind(i64::try_from(now_secs())?)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let project_id: String = row.try_get("project_id")?;
        let Some(project) = self.get_composition_project(&project_id).await? else {
            return Ok(None);
        };
        Ok(Some(SharedReview {
            project_id: project_id.clone(),
            project_name: project.name,
            expires_at: u64::try_from(row.try_get::<i64, _>("expires_at")?)?,
            threads: self.list_review_threads(&project_id).await?,
        }))
    }

    pub async fn revoke_review_share(
        &self,
        project_id: &str,
        share_id: &str,
        requester: &str,
    ) -> Result<Option<bool>> {
        let Some(role) = self.review_role(project_id, requester, false).await? else {
            return Ok(None);
        };
        ensure!(
            role == ProjectRole::Owner,
            "role cannot revoke review share"
        );
        let now = now_secs();
        let mut transaction = self.pool.begin().await?;
        let result = sqlx::query(
            "UPDATE project_review_shares SET revoked_at = ? \
             WHERE id = ? AND project_id = ? AND revoked_at IS NULL",
        )
        .bind(i64::try_from(now)?)
        .bind(share_id)
        .bind(project_id)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() == 1 {
            insert_audit(
                &mut transaction,
                project_id,
                requester,
                "share.revoked",
                share_id,
                now,
            )
            .await?;
        }
        transaction.commit().await?;
        Ok(Some(result.rows_affected() == 1))
    }

    async fn review_role(
        &self,
        project_id: &str,
        actor: &str,
        bootstrap_owner: bool,
    ) -> Result<Option<ProjectRole>> {
        ensure!(valid_actor(actor), "invalid project member");
        let mut transaction = self.pool.begin().await?;
        let role = sqlx::query_scalar::<_, String>(
            "SELECT role FROM composition_project_members WHERE project_id = ? AND actor = ?",
        )
        .bind(project_id)
        .bind(actor)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(role) = role {
            transaction.commit().await?;
            return Ok(Some(
                ProjectRole::try_from(role.as_str()).map_err(anyhow::Error::msg)?,
            ));
        }
        let members = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM composition_project_members WHERE project_id = ?",
        )
        .bind(project_id)
        .fetch_one(&mut *transaction)
        .await?;
        if members == 0 && bootstrap_owner {
            let now = i64::try_from(now_secs())?;
            sqlx::query(
                "INSERT INTO composition_project_members (project_id, actor, role, created_at, updated_at) \
                 VALUES (?, ?, 'owner', ?, ?)",
            )
            .bind(project_id)
            .bind(actor)
            .bind(now)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            return Ok(Some(ProjectRole::Owner));
        }
        transaction.commit().await?;
        Ok(None)
    }

    async fn require_review_member(&self, project_id: &str, actor: &str) -> Result<ProjectRole> {
        self.review_role(project_id, actor, false)
            .await?
            .ok_or_else(|| anyhow::anyhow!("actor is not a project member"))
    }

    async fn get_review_thread(&self, thread_id: &str) -> Result<Option<ReviewThread>> {
        let row = sqlx::query(
            "SELECT id, project_id, resolved_at, resolved_by FROM project_review_threads WHERE id = ?",
        )
        .bind(thread_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let comments = load_comments(&self.pool, thread_id).await?;
        ensure!(!comments.is_empty(), "review thread has no root comment");
        Ok(Some(ReviewThread {
            id: row.try_get("id")?,
            project_id: row.try_get("project_id")?,
            comments,
            resolved_at: row
                .try_get::<Option<i64>, _>("resolved_at")?
                .map(u64::try_from)
                .transpose()?,
            resolved_by: row.try_get("resolved_by")?,
        }))
    }
}

fn valid_actor(actor: &str) -> bool {
    !actor.is_empty()
        && actor.len() <= 64
        && actor
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

fn hash_share_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

async fn insert_comment(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    thread_id: &str,
    comment: &ReviewComment,
    order: i64,
) -> Result<()> {
    ensure!(comment.validate(), "invalid review comment");
    sqlx::query(
        "INSERT INTO project_review_comments \
         (id, thread_id, author, body, timeline_tick, created_at, comment_order) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&comment.id)
    .bind(thread_id)
    .bind(&comment.author)
    .bind(&comment.body)
    .bind(i64::try_from(comment.timeline_tick)?)
    .bind(i64::try_from(comment.created_at)?)
    .bind(order)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn load_comments(pool: &SqlitePool, thread_id: &str) -> Result<Vec<ReviewComment>> {
    let rows = sqlx::query(
        "SELECT id, author, body, timeline_tick, created_at, comment_order \
         FROM project_review_comments WHERE thread_id = ? ORDER BY comment_order",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let order: i64 = row.try_get("comment_order")?;
            Ok(ReviewComment {
                id: row.try_get("id")?,
                author: row.try_get("author")?,
                body: row.try_get("body")?,
                timeline_tick: u64::try_from(row.try_get::<i64, _>("timeline_tick")?)?,
                parent_id: (order > 0).then(|| thread_id.into()),
                created_at: u64::try_from(row.try_get::<i64, _>("created_at")?)?,
            })
        })
        .collect()
}

async fn insert_reply(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    thread_id: &str,
    comment: &ReviewComment,
) -> Result<()> {
    ensure!(comment.validate(), "invalid review comment");
    sqlx::query(
        "INSERT INTO project_review_comments \
         (id, thread_id, author, body, timeline_tick, created_at, comment_order) \
         SELECT ?, ?, ?, ?, ?, ?, COALESCE(MAX(comment_order) + 1, 1) \
         FROM project_review_comments WHERE thread_id = ?",
    )
    .bind(&comment.id)
    .bind(thread_id)
    .bind(&comment.author)
    .bind(&comment.body)
    .bind(i64::try_from(comment.timeline_tick)?)
    .bind(i64::try_from(comment.created_at)?)
    .bind(thread_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn insert_audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    project_id: &str,
    actor: &str,
    action: &str,
    subject_id: &str,
    created_at: u64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO project_review_audit \
         (id, project_id, actor, action, subject_id, created_at, event_order) \
         VALUES (?, ?, ?, ?, ?, ?, COALESCE((SELECT MAX(event_order) + 1 FROM project_review_audit WHERE project_id = ?), 1))",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(project_id)
    .bind(actor)
    .bind(action)
    .bind(subject_id)
    .bind(i64::try_from(created_at)?)
    .bind(project_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
