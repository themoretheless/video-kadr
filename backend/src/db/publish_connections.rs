use anyhow::{Context, Result};
use sqlx::SqlitePool;

use super::Db;
use crate::library::now_secs;
use crate::youtube::{TokenCipher, YouTubeTokens};

pub(super) async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS publish_connections (provider TEXT NOT NULL, actor TEXT NOT NULL, encrypted_tokens BLOB NOT NULL, token_expires_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, PRIMARY KEY(provider, actor)); \
         CREATE INDEX IF NOT EXISTS idx_publish_connections_actor ON publish_connections(actor, provider);",
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    pub async fn save_youtube_tokens(
        &self,
        cipher: &TokenCipher,
        actor: &str,
        mut tokens: YouTubeTokens,
    ) -> Result<()> {
        if tokens.refresh_token.is_none() {
            if let Some(previous) = self.load_youtube_tokens(cipher, actor).await? {
                tokens.refresh_token = previous.refresh_token;
            }
        }
        let plaintext = serde_json::to_vec(&tokens)?;
        let encrypted = cipher.encrypt(actor, &plaintext)?;
        sqlx::query(
            "INSERT INTO publish_connections (provider, actor, encrypted_tokens, token_expires_at, updated_at) VALUES ('youtube', ?, ?, ?, ?) \
             ON CONFLICT(provider, actor) DO UPDATE SET encrypted_tokens = excluded.encrypted_tokens, token_expires_at = excluded.token_expires_at, updated_at = excluded.updated_at",
        )
        .bind(actor)
        .bind(encrypted)
        .bind(i64::try_from(tokens.expires_at)?)
        .bind(i64::try_from(now_secs())?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_youtube_tokens(
        &self,
        cipher: &TokenCipher,
        actor: &str,
    ) -> Result<Option<YouTubeTokens>> {
        let encrypted: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT encrypted_tokens FROM publish_connections WHERE provider = 'youtube' AND actor = ?",
        )
        .bind(actor)
        .fetch_optional(&self.pool)
        .await?;
        encrypted
            .map(|value| {
                let plaintext = cipher.decrypt(actor, &value)?;
                serde_json::from_slice(&plaintext).context("decode encrypted YouTube tokens")
            })
            .transpose()
    }

    pub async fn has_youtube_connection(&self, actor: &str) -> Result<bool> {
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM publish_connections WHERE provider = 'youtube' AND actor = ?)",
        )
        .bind(actor)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists != 0)
    }

    pub async fn delete_youtube_connection(&self, actor: &str) -> Result<()> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("DELETE FROM publish_uploads WHERE actor = ?")
            .bind(actor)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM publish_connections WHERE provider = 'youtube' AND actor = ?")
            .bind(actor)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tokens_are_encrypted_bound_to_actor_and_refresh_is_preserved() {
        let directory = tempfile::tempdir().unwrap();
        let db = Db::open(directory.path()).await.unwrap();
        let cipher = TokenCipher::new([9; 32]).unwrap();
        let tokens = YouTubeTokens {
            access_token: "access-secret".into(),
            refresh_token: Some("refresh-secret".into()),
            expires_at: 100,
            scope: crate::youtube::YOUTUBE_UPLOAD_SCOPE.into(),
            token_type: "Bearer".into(),
        };
        db.save_youtube_tokens(&cipher, "alice", tokens)
            .await
            .unwrap();
        let raw: Vec<u8> = sqlx::query_scalar("SELECT encrypted_tokens FROM publish_connections")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("secret"));
        let replacement = YouTubeTokens {
            access_token: "next-access".into(),
            refresh_token: None,
            expires_at: 200,
            scope: crate::youtube::YOUTUBE_UPLOAD_SCOPE.into(),
            token_type: "Bearer".into(),
        };
        db.save_youtube_tokens(&cipher, "alice", replacement)
            .await
            .unwrap();
        let loaded = db
            .load_youtube_tokens(&cipher, "alice")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.access_token, "next-access");
        assert_eq!(loaded.refresh_token.as_deref(), Some("refresh-secret"));
        assert!(db
            .load_youtube_tokens(&cipher, "bob")
            .await
            .unwrap()
            .is_none());
        let checkpoint = crate::youtube::YouTubeUploadCheckpoint {
            session_url: "https://upload.example.test/session".into(),
            total_bytes: 10,
            confirmed_offset: 5,
        };
        db.save_youtube_upload_checkpoint(&cipher, "job", "alice", "output", &checkpoint)
            .await
            .unwrap();
        db.delete_youtube_connection("alice").await.unwrap();
        assert!(!db.has_youtube_connection("alice").await.unwrap());
        assert!(db
            .load_youtube_upload_checkpoint(&cipher, "job", "alice", "output")
            .await
            .unwrap()
            .is_none());
    }
}
