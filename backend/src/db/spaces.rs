use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::collections::HashSet;
use uuid::Uuid;

use super::Db;
use crate::library::now_secs;

pub const MAX_SPACE_NAME_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpaceRole {
    Owner,
    Editor,
    Viewer,
}

impl SpaceRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Editor => "editor",
            Self::Viewer => "viewer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Space {
    pub id: String,
    pub name: String,
    pub role: SpaceRole,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceMember {
    pub actor: String,
    pub role: SpaceRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceInviteCreated {
    pub id: String,
    pub space_id: String,
    pub role: SpaceRole,
    pub expires_at: u64,
    pub token: String,
}

pub(super) async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS collaboration_spaces (id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL); \
         CREATE TABLE IF NOT EXISTS collaboration_space_members (space_id TEXT NOT NULL, actor TEXT NOT NULL, role TEXT NOT NULL CHECK(role IN ('owner','editor','viewer')), created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, PRIMARY KEY(space_id, actor), FOREIGN KEY(space_id) REFERENCES collaboration_spaces(id) ON DELETE CASCADE); \
         CREATE TABLE IF NOT EXISTS collaboration_space_media (space_id TEXT NOT NULL, source_id TEXT NOT NULL UNIQUE, created_by TEXT NOT NULL, created_at INTEGER NOT NULL, PRIMARY KEY(space_id, source_id), FOREIGN KEY(space_id) REFERENCES collaboration_spaces(id) ON DELETE RESTRICT); \
         CREATE TABLE IF NOT EXISTS collaboration_space_invites (id TEXT PRIMARY KEY, space_id TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE, role TEXT NOT NULL CHECK(role IN ('editor','viewer')), expires_at INTEGER NOT NULL, created_by TEXT NOT NULL, created_at INTEGER NOT NULL, FOREIGN KEY(space_id) REFERENCES collaboration_spaces(id) ON DELETE CASCADE); \
         CREATE INDEX IF NOT EXISTS idx_collaboration_space_members_actor ON collaboration_space_members(actor, space_id); \
         CREATE INDEX IF NOT EXISTS idx_collaboration_space_invites_expiry ON collaboration_space_invites(expires_at);",
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    pub async fn create_space_invite(
        &self,
        space_id: &str,
        requester: &str,
        role: SpaceRole,
        ttl_seconds: u64,
    ) -> Result<Option<SpaceInviteCreated>> {
        ensure!(role != SpaceRole::Owner, "cannot invite a space owner");
        ensure!(
            (60..=30 * 24 * 60 * 60).contains(&ttl_seconds),
            "invalid space invite TTL"
        );
        let requester_role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM collaboration_space_members WHERE space_id = ? AND actor = ?",
        )
        .bind(space_id)
        .bind(requester)
        .fetch_optional(&self.pool)
        .await?;
        let Some(requester_role) = requester_role else {
            return Ok(None);
        };
        ensure!(requester_role == "owner", "role cannot create space invite");
        let now = now_secs();
        let expires_at = now
            .checked_add(ttl_seconds)
            .ok_or_else(|| anyhow::anyhow!("invalid space invite TTL"))?;
        let id = Uuid::new_v4().to_string();
        let token = format!("{}.{}", Uuid::new_v4(), Uuid::new_v4());
        sqlx::query("DELETE FROM collaboration_space_invites WHERE expires_at <= ?")
            .bind(i64::try_from(now)?)
            .execute(&self.pool)
            .await?;
        sqlx::query("INSERT INTO collaboration_space_invites (id, space_id, token_hash, role, expires_at, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(&id).bind(space_id).bind(hash_space_invite_token(&token)).bind(role.as_str())
            .bind(i64::try_from(expires_at)?).bind(requester).bind(i64::try_from(now)?)
            .execute(&self.pool).await?;
        Ok(Some(SpaceInviteCreated {
            id,
            space_id: space_id.into(),
            role,
            expires_at,
            token,
        }))
    }

    pub async fn accept_space_invite(&self, token: &str, actor: &str) -> Result<Option<Space>> {
        if token.len() > 128
            || !token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
        {
            return Ok(None);
        }
        let now = now_secs();
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query("DELETE FROM collaboration_space_invites WHERE token_hash = ? AND expires_at > ? RETURNING space_id, role")
            .bind(hash_space_invite_token(token)).bind(i64::try_from(now)?)
            .fetch_optional(&mut *transaction).await?;
        let Some(row) = row else { return Ok(None) };
        let space_id: String = row.try_get("space_id")?;
        let invited_role = parse_role(&row.try_get::<String, _>("role")?)?;
        let inherited_role = if invited_role == SpaceRole::Editor {
            "editor"
        } else {
            "viewer"
        };
        let now_i64 = i64::try_from(now)?;
        sqlx::query("INSERT INTO collaboration_space_members (space_id, actor, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?) ON CONFLICT(space_id, actor) DO UPDATE SET role = excluded.role, updated_at = excluded.updated_at WHERE collaboration_space_members.role != 'owner'")
            .bind(&space_id).bind(actor).bind(invited_role.as_str()).bind(now_i64).bind(now_i64)
            .execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO composition_project_members (project_id, actor, role, created_at, updated_at) SELECT id, ?, ?, ?, ? FROM composition_projects WHERE space_id = ? ON CONFLICT(project_id, actor) DO UPDATE SET role = excluded.role, updated_at = excluded.updated_at WHERE composition_project_members.role != 'owner'")
            .bind(actor).bind(inherited_role).bind(now_i64).bind(now_i64).bind(&space_id)
            .execute(&mut *transaction).await?;
        let row = sqlx::query("SELECT s.name, s.created_at, s.updated_at, m.role FROM collaboration_spaces s JOIN collaboration_space_members m ON m.space_id = s.id WHERE s.id = ? AND m.actor = ?")
            .bind(&space_id).bind(actor).fetch_one(&mut *transaction).await?;
        let space = Space {
            id: space_id,
            name: row.try_get("name")?,
            role: parse_role(&row.try_get::<String, _>("role")?)?,
            created_at: u64::try_from(row.try_get::<i64, _>("created_at")?)?,
            updated_at: u64::try_from(row.try_get::<i64, _>("updated_at")?)?,
        };
        transaction.commit().await?;
        Ok(Some(space))
    }

    pub async fn source_space_id(&self, source_id: &str) -> Result<Option<String>> {
        sqlx::query_scalar("SELECT space_id FROM collaboration_space_media WHERE source_id = ?")
            .bind(source_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn is_space_member(&self, space_id: &str, actor: &str) -> Result<bool> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM collaboration_space_members WHERE space_id = ? AND actor = ?)")
            .bind(space_id)
            .bind(actor)
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn accessible_space_source_ids(&self, actor: &str) -> Result<HashSet<String>> {
        let source_ids: Vec<String> = sqlx::query_scalar(
            "SELECT media.source_id FROM collaboration_space_media media \
             JOIN collaboration_space_members member ON member.space_id = media.space_id \
             WHERE member.actor = ?",
        )
        .bind(actor)
        .fetch_all(&self.pool)
        .await?;
        Ok(source_ids.into_iter().collect())
    }

    pub async fn space_source_ids(&self, space_id: &str) -> Result<HashSet<String>> {
        let source_ids: Vec<String> = sqlx::query_scalar(
            "SELECT source_id FROM collaboration_space_media WHERE space_id = ?",
        )
        .bind(space_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(source_ids.into_iter().collect())
    }

    pub async fn all_space_source_ids(&self) -> Result<HashSet<String>> {
        let source_ids: Vec<String> =
            sqlx::query_scalar("SELECT source_id FROM collaboration_space_media")
                .fetch_all(&self.pool)
                .await?;
        Ok(source_ids.into_iter().collect())
    }

    pub async fn release_space_source_claim(&self, source_id: &str) -> Result<bool> {
        Ok(
            sqlx::query("DELETE FROM collaboration_space_media WHERE source_id = ?")
                .bind(source_id)
                .execute(&self.pool)
                .await?
                .rows_affected()
                > 0,
        )
    }

    pub async fn create_space(&self, owner: &str, name: &str) -> Result<Space> {
        let name = name.trim();
        ensure!(
            !name.is_empty() && name.len() <= MAX_SPACE_NAME_BYTES,
            "invalid space name"
        );
        let id = Uuid::new_v4().to_string();
        let now = now_secs();
        let mut transaction = self.pool.begin().await?;
        sqlx::query("INSERT INTO collaboration_spaces (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(&id).bind(name).bind(i64::try_from(now)?).bind(i64::try_from(now)?)
            .execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO collaboration_space_members (space_id, actor, role, created_at, updated_at) VALUES (?, ?, 'owner', ?, ?)")
            .bind(&id).bind(owner).bind(i64::try_from(now)?).bind(i64::try_from(now)?)
            .execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(Space {
            id,
            name: name.into(),
            role: SpaceRole::Owner,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn list_spaces_for(&self, actor: &str) -> Result<Vec<Space>> {
        let rows = sqlx::query("SELECT s.id, s.name, m.role, s.created_at, s.updated_at FROM collaboration_spaces s JOIN collaboration_space_members m ON m.space_id = s.id WHERE m.actor = ? ORDER BY s.updated_at DESC, s.id")
            .bind(actor).fetch_all(&self.pool).await?;
        rows.into_iter().map(space_from_row).collect()
    }

    pub async fn rename_space(
        &self,
        space_id: &str,
        requester: &str,
        name: &str,
        base_updated_at: u64,
    ) -> Result<Option<Space>> {
        let name = name.trim();
        ensure!(
            !name.is_empty() && name.len() <= MAX_SPACE_NAME_BYTES,
            "invalid space name"
        );
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM collaboration_space_members WHERE space_id = ? AND actor = ?",
        )
        .bind(space_id)
        .bind(requester)
        .fetch_optional(&self.pool)
        .await?;
        let Some(role) = role else {
            return Ok(None);
        };
        ensure!(role == "owner", "role cannot manage space");
        let updated_at = now_secs().max(base_updated_at.saturating_add(1));
        let changed = sqlx::query(
            "UPDATE collaboration_spaces SET name = ?, updated_at = ? WHERE id = ? AND updated_at = ?",
        )
        .bind(name)
        .bind(i64::try_from(updated_at)?)
        .bind(space_id)
        .bind(i64::try_from(base_updated_at)?)
        .execute(&self.pool)
        .await?
        .rows_affected();
        ensure!(changed == 1, "space revision conflict");
        let created_at: i64 =
            sqlx::query_scalar("SELECT created_at FROM collaboration_spaces WHERE id = ?")
                .bind(space_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(Some(Space {
            id: space_id.into(),
            name: name.into(),
            role: SpaceRole::Owner,
            created_at: u64::try_from(created_at)?,
            updated_at,
        }))
    }

    /// Delete only an empty Space. Projects and original media require their
    /// normal explicit deletion flows so this operation never becomes a hidden
    /// destructive cascade.
    pub async fn delete_empty_space(
        &self,
        space_id: &str,
        requester: &str,
    ) -> Result<Option<bool>> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM collaboration_space_members WHERE space_id = ? AND actor = ?",
        )
        .bind(space_id)
        .bind(requester)
        .fetch_optional(&self.pool)
        .await?;
        let Some(role) = role else {
            return Ok(None);
        };
        ensure!(role == "owner", "role cannot manage space");
        let project_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM composition_projects WHERE space_id = ?")
                .bind(space_id)
                .fetch_one(&self.pool)
                .await?;
        let media_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM collaboration_space_media WHERE space_id = ?")
                .bind(space_id)
                .fetch_one(&self.pool)
                .await?;
        ensure!(
            project_count == 0 && media_count == 0,
            "space not empty: {project_count} projects, {media_count} media"
        );
        Ok(Some(
            sqlx::query("DELETE FROM collaboration_spaces WHERE id = ?")
                .bind(space_id)
                .execute(&self.pool)
                .await?
                .rows_affected()
                > 0,
        ))
    }

    pub async fn list_space_members_for(
        &self,
        space_id: &str,
        actor: &str,
    ) -> Result<Vec<SpaceMember>> {
        let member: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM collaboration_space_members WHERE space_id = ? AND actor = ?)")
            .bind(space_id).bind(actor).fetch_one(&self.pool).await?;
        ensure!(member, "actor is not a space member");
        let rows = sqlx::query(
            "SELECT actor, role FROM collaboration_space_members WHERE space_id = ? ORDER BY actor",
        )
        .bind(space_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(SpaceMember {
                    actor: row.try_get("actor")?,
                    role: parse_role(&row.try_get::<String, _>("role")?)?,
                })
            })
            .collect()
    }

    pub async fn set_space_member(
        &self,
        space_id: &str,
        requester: &str,
        actor: &str,
        role: SpaceRole,
    ) -> Result<Option<SpaceMember>> {
        ensure!(
            role != SpaceRole::Owner,
            "space owner assignment requires transfer"
        );
        let requester_role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM collaboration_space_members WHERE space_id = ? AND actor = ?",
        )
        .bind(space_id)
        .bind(requester)
        .fetch_optional(&self.pool)
        .await?;
        let Some(requester_role) = requester_role else {
            return Ok(None);
        };
        ensure!(
            requester_role == "owner",
            "role cannot manage space members"
        );
        ensure!(
            !actor.is_empty()
                && actor.len() <= 64
                && actor
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte)),
            "invalid space member"
        );
        let now = i64::try_from(now_secs())?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query("INSERT INTO collaboration_space_members (space_id, actor, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?) ON CONFLICT(space_id, actor) DO UPDATE SET role = excluded.role, updated_at = excluded.updated_at")
            .bind(space_id).bind(actor).bind(role.as_str()).bind(now).bind(now).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO composition_project_members (project_id, actor, role, created_at, updated_at) SELECT id, ?, ?, ?, ? FROM composition_projects WHERE space_id = ? ON CONFLICT(project_id, actor) DO NOTHING")
            .bind(actor)
            .bind(if role == SpaceRole::Editor { "editor" } else { "viewer" })
            .bind(now).bind(now).bind(space_id).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(Some(SpaceMember {
            actor: actor.into(),
            role,
        }))
    }

    /// Revoke a non-owner's Space membership and every project membership
    /// inherited from that Space in the same transaction.
    pub async fn remove_space_member(
        &self,
        space_id: &str,
        requester: &str,
        actor: &str,
    ) -> Result<Option<bool>> {
        let requester_role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM collaboration_space_members WHERE space_id = ? AND actor = ?",
        )
        .bind(space_id)
        .bind(requester)
        .fetch_optional(&self.pool)
        .await?;
        let Some(requester_role) = requester_role else {
            return Ok(None);
        };
        ensure!(
            requester_role == "owner",
            "role cannot manage space members"
        );

        let target_role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM collaboration_space_members WHERE space_id = ? AND actor = ?",
        )
        .bind(space_id)
        .bind(actor)
        .fetch_optional(&self.pool)
        .await?;
        let Some(target_role) = target_role else {
            return Ok(Some(false));
        };
        ensure!(
            target_role != "owner",
            "space owner removal requires transfer"
        );

        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "DELETE FROM composition_project_members WHERE actor = ? AND project_id IN (SELECT id FROM composition_projects WHERE space_id = ?)",
        )
        .bind(actor)
        .bind(space_id)
        .execute(&mut *transaction)
        .await?;
        let deleted = sqlx::query(
            "DELETE FROM collaboration_space_members WHERE space_id = ? AND actor = ? AND role != 'owner'",
        )
        .bind(space_id)
        .bind(actor)
        .execute(&mut *transaction)
        .await?
        .rows_affected()
            == 1;
        transaction.commit().await?;
        Ok(Some(deleted))
    }

    pub async fn transfer_space_ownership(
        &self,
        space_id: &str,
        requester: &str,
        target: &str,
    ) -> Result<Option<SpaceMember>> {
        ensure!(
            requester != target,
            "space owner transfer target must differ"
        );
        let mut transaction = self.pool.begin().await?;
        let requester_role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM collaboration_space_members WHERE space_id = ? AND actor = ?",
        )
        .bind(space_id)
        .bind(requester)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(requester_role) = requester_role else {
            return Ok(None);
        };
        ensure!(requester_role == "owner", "role cannot manage space");
        let target_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM collaboration_space_members WHERE space_id = ? AND actor = ?)",
        )
        .bind(space_id)
        .bind(target)
        .fetch_one(&mut *transaction)
        .await?;
        ensure!(target_exists, "target must be a space member");
        let now = i64::try_from(now_secs())?;
        sqlx::query(
            "UPDATE collaboration_space_members SET role = 'editor', updated_at = ? WHERE space_id = ? AND actor = ? AND role = 'owner'",
        )
        .bind(now)
        .bind(space_id)
        .bind(requester)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE collaboration_space_members SET role = 'owner', updated_at = ? WHERE space_id = ? AND actor = ?",
        )
        .bind(now)
        .bind(space_id)
        .bind(target)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE collaboration_spaces SET updated_at = MAX(updated_at + 1, ?) WHERE id = ?",
        )
        .bind(now)
        .bind(space_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(Some(SpaceMember {
            actor: target.into(),
            role: SpaceRole::Owner,
        }))
    }
}

fn parse_role(value: &str) -> Result<SpaceRole> {
    match value {
        "owner" => Ok(SpaceRole::Owner),
        "editor" => Ok(SpaceRole::Editor),
        "viewer" => Ok(SpaceRole::Viewer),
        _ => anyhow::bail!("invalid space role"),
    }
}

fn space_from_row(row: sqlx::sqlite::SqliteRow) -> Result<Space> {
    Ok(Space {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        role: parse_role(&row.try_get::<String, _>("role")?)?,
        created_at: u64::try_from(row.try_get::<i64, _>("created_at")?)?,
        updated_at: u64::try_from(row.try_get::<i64, _>("updated_at")?)?,
    })
}

fn hash_space_invite_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invite_is_hashed_consumed_once_and_cannot_demote_owner() {
        let directory = tempfile::tempdir().unwrap();
        let db = Db::open(directory.path()).await.unwrap();
        let space = db.create_space("alice", "Studio").await.unwrap();
        let invite = db
            .create_space_invite(&space.id, "alice", SpaceRole::Editor, 3600)
            .await
            .unwrap()
            .unwrap();
        let stored: String =
            sqlx::query_scalar("SELECT token_hash FROM collaboration_space_invites WHERE id = ?")
                .bind(&invite.id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(stored, hash_space_invite_token(&invite.token));
        assert_ne!(stored, invite.token);

        let accepted = db
            .accept_space_invite(&invite.token, "bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(accepted.role, SpaceRole::Editor);
        assert!(db
            .accept_space_invite(&invite.token, "charlie")
            .await
            .unwrap()
            .is_none());

        let owner_invite = db
            .create_space_invite(&space.id, "alice", SpaceRole::Viewer, 3600)
            .await
            .unwrap()
            .unwrap();
        let accepted_owner = db
            .accept_space_invite(&owner_invite.token, "alice")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(accepted_owner.role, SpaceRole::Owner);
        assert!(db
            .create_space_invite(&space.id, "bob", SpaceRole::Viewer, 3600)
            .await
            .is_err());
        assert!(db
            .create_space_invite(&space.id, "alice", SpaceRole::Owner, 3600)
            .await
            .is_err());
    }
}
