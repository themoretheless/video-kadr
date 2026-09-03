use anyhow::{Context, Result};
use sqlx::SqlitePool;

use super::Db;
use crate::library::now_secs;
use crate::youtube::{TokenCipher, YouTubeUploadCheckpoint};

pub(super) async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS publish_uploads (job_id TEXT PRIMARY KEY, actor TEXT NOT NULL, output_id TEXT NOT NULL, encrypted_checkpoint BLOB NOT NULL, updated_at INTEGER NOT NULL); \
         CREATE INDEX IF NOT EXISTS idx_publish_uploads_actor ON publish_uploads(actor, job_id);",
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    pub async fn save_youtube_upload_checkpoint(
        &self,
        cipher: &TokenCipher,
        job_id: &str,
        actor: &str,
        output_id: &str,
        checkpoint: &YouTubeUploadCheckpoint,
    ) -> Result<()> {
        let context = checkpoint_context(actor, job_id);
        let encrypted = cipher.encrypt(&context, &serde_json::to_vec(checkpoint)?)?;
        sqlx::query(
            "INSERT INTO publish_uploads (job_id, actor, output_id, encrypted_checkpoint, updated_at) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(job_id) DO UPDATE SET actor = excluded.actor, output_id = excluded.output_id, encrypted_checkpoint = excluded.encrypted_checkpoint, updated_at = excluded.updated_at",
        )
        .bind(job_id)
        .bind(actor)
        .bind(output_id)
        .bind(encrypted)
        .bind(i64::try_from(now_secs())?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_youtube_upload_checkpoint(
        &self,
        cipher: &TokenCipher,
        job_id: &str,
        actor: &str,
        output_id: &str,
    ) -> Result<Option<YouTubeUploadCheckpoint>> {
        let encrypted: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT encrypted_checkpoint FROM publish_uploads WHERE job_id = ? AND actor = ? AND output_id = ?",
        )
        .bind(job_id).bind(actor).bind(output_id)
        .fetch_optional(&self.pool).await?;
        encrypted
            .map(|value| {
                let plaintext = cipher.decrypt(&checkpoint_context(actor, job_id), &value)?;
                serde_json::from_slice(&plaintext)
                    .context("decode encrypted YouTube upload checkpoint")
            })
            .transpose()
    }

    pub async fn delete_youtube_upload_checkpoint(&self, job_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM publish_uploads WHERE job_id = ?")
            .bind(job_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn checkpoint_context(actor: &str, job_id: &str) -> String {
    format!("youtube-upload\0{actor}\0{job_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn checkpoint_is_encrypted_and_bound_to_job_actor_and_output() {
        let directory = tempfile::tempdir().unwrap();
        let db = Db::open(directory.path()).await.unwrap();
        let cipher = TokenCipher::new([3; 32]).unwrap();
        let checkpoint = YouTubeUploadCheckpoint {
            session_url: "https://upload.example.test/secret-session".into(),
            total_bytes: 99,
            confirmed_offset: 42,
        };
        db.save_youtube_upload_checkpoint(&cipher, "job-1", "alice", "output-1", &checkpoint)
            .await
            .unwrap();
        let raw: Vec<u8> = sqlx::query_scalar("SELECT encrypted_checkpoint FROM publish_uploads")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("secret-session"));
        assert_eq!(
            db.load_youtube_upload_checkpoint(&cipher, "job-1", "alice", "output-1")
                .await
                .unwrap(),
            Some(checkpoint)
        );
        assert!(db
            .load_youtube_upload_checkpoint(&cipher, "job-1", "bob", "output-1")
            .await
            .unwrap()
            .is_none());
        assert!(db
            .load_youtube_upload_checkpoint(&cipher, "job-1", "alice", "other")
            .await
            .unwrap()
            .is_none());
    }
}
