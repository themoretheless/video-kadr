use anyhow::{anyhow, Result};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::library::now_secs;

pub const AUTH_SESSION_TTL_SECS: u64 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSession {
    pub user: AuthUser,
    pub token: String,
    pub expires_at: u64,
}

pub(super) async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS auth_users (\
           id TEXT PRIMARY KEY, username TEXT NOT NULL UNIQUE COLLATE NOCASE, \
           password_hash TEXT NOT NULL, created_at INTEGER NOT NULL\
         ); \
         CREATE TABLE IF NOT EXISTS auth_sessions (\
           token_hash TEXT PRIMARY KEY, user_id TEXT NOT NULL, expires_at INTEGER NOT NULL, \
           revoked_at INTEGER, created_at INTEGER NOT NULL, \
           FOREIGN KEY(user_id) REFERENCES auth_users(id) ON DELETE CASCADE\
         ); \
         CREATE INDEX IF NOT EXISTS idx_auth_sessions_user ON auth_sessions(user_id, expires_at);",
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl super::Db {
    pub async fn register_auth_user(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<AuthSession>> {
        let username = username.to_owned();
        let password = password.to_owned();
        let password_hash = tokio::task::spawn_blocking(move || hash_password(&password))
            .await
            .map_err(|error| anyhow!("password hashing task failed: {error}"))??;
        let user = AuthUser {
            id: Uuid::new_v4().to_string(),
            username,
            created_at: now_secs(),
        };
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO auth_users (id, username, password_hash, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&user.id)
        .bind(&user.username)
        .bind(password_hash)
        .bind(i64::try_from(user.created_at)?)
        .execute(&self.pool)
        .await?;
        if inserted.rows_affected() == 0 {
            return Ok(None);
        }
        self.create_auth_session(user).await.map(Some)
    }

    pub async fn login_auth_user(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<AuthSession>> {
        let row = sqlx::query(
            "SELECT id, username, password_hash, created_at FROM auth_users WHERE username = ? COLLATE NOCASE",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            // Keep the unknown-user path memory-hard too, so response timing
            // does not become a cheap account-existence oracle.
            let password = password.to_owned();
            tokio::task::spawn_blocking(move || hash_password(&password))
                .await
                .map_err(|error| anyhow!("dummy password hashing task failed: {error}"))??;
            return Ok(None);
        };
        let hash: String = row.try_get("password_hash")?;
        let password = password.to_owned();
        let verified = tokio::task::spawn_blocking(move || verify_password(&hash, &password))
            .await
            .map_err(|error| anyhow!("password verification task failed: {error}"))??;
        if !verified {
            return Ok(None);
        }
        self.create_auth_session(AuthUser {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            created_at: u64::try_from(row.try_get::<i64, _>("created_at")?)?,
        })
        .await
        .map(Some)
    }

    pub async fn resolve_auth_session(&self, token: &str) -> Result<Option<AuthUser>> {
        if !valid_session_token(token) {
            return Ok(None);
        }
        let row = sqlx::query(
            "SELECT u.id, u.username, u.created_at FROM auth_sessions s \
             JOIN auth_users u ON u.id = s.user_id \
             WHERE s.token_hash = ? AND s.revoked_at IS NULL AND s.expires_at > ?",
        )
        .bind(hash_token(token))
        .bind(i64::try_from(now_secs())?)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            Ok(AuthUser {
                id: row.try_get("id")?,
                username: row.try_get("username")?,
                created_at: u64::try_from(row.try_get::<i64, _>("created_at")?)?,
            })
        })
        .transpose()
    }

    pub async fn revoke_auth_session(&self, token: &str) -> Result<bool> {
        if !valid_session_token(token) {
            return Ok(false);
        }
        Ok(sqlx::query(
            "UPDATE auth_sessions SET revoked_at = ? WHERE token_hash = ? AND revoked_at IS NULL",
        )
        .bind(i64::try_from(now_secs())?)
        .bind(hash_token(token))
        .execute(&self.pool)
        .await?
        .rows_affected()
            > 0)
    }

    async fn create_auth_session(&self, user: AuthUser) -> Result<AuthSession> {
        let token = format!("{}.{}", Uuid::new_v4(), Uuid::new_v4());
        let now = now_secs();
        let expires_at = now.saturating_add(AUTH_SESSION_TTL_SECS);
        sqlx::query("DELETE FROM auth_sessions WHERE expires_at <= ? OR revoked_at IS NOT NULL")
            .bind(i64::try_from(now)?)
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "INSERT INTO auth_sessions (token_hash, user_id, expires_at, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(hash_token(&token))
        .bind(&user.id)
        .bind(i64::try_from(expires_at)?)
        .bind(i64::try_from(now)?)
        .execute(&self.pool)
        .await?;
        Ok(AuthSession {
            user,
            token,
            expires_at,
        })
    }
}

fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes())
        .map_err(|error| anyhow!("create password salt: {error}"))?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| anyhow!("hash password: {error}"))
}

fn verify_password(hash: &str, password: &str) -> Result<bool> {
    let parsed =
        PasswordHash::new(hash).map_err(|error| anyhow!("parse password hash: {error}"))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn valid_session_token(token: &str) -> bool {
    token.len() <= 128
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn persists_only_password_and_session_hashes() {
        let directory = tempfile::tempdir().unwrap();
        let db = super::super::Db::open(directory.path()).await.unwrap();
        let session = db
            .register_auth_user("secure-user", "this password is long enough")
            .await
            .unwrap()
            .unwrap();

        let row = sqlx::query("SELECT password_hash FROM auth_users WHERE id = ?")
            .bind(&session.user.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        let password_hash: String = row.try_get("password_hash").unwrap();
        assert!(password_hash.starts_with("$argon2"));
        assert!(!password_hash.contains("this password is long enough"));

        let row = sqlx::query("SELECT token_hash FROM auth_sessions WHERE user_id = ?")
            .bind(&session.user.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        let token_hash: String = row.try_get("token_hash").unwrap();
        assert_eq!(token_hash.len(), 64);
        assert_ne!(token_hash, session.token);
        assert_eq!(
            db.resolve_auth_session(&session.token).await.unwrap(),
            Some(session.user.clone())
        );
        assert!(db.revoke_auth_session(&session.token).await.unwrap());
        assert_eq!(db.resolve_auth_session(&session.token).await.unwrap(), None);
    }
}
