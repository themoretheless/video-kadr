use anyhow::Result;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use super::Db;
use crate::library::now_secs;

const OAUTH_STATE_TTL_SECS: u64 = 10 * 60;

pub(super) async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS publish_oauth_states (state_hash TEXT PRIMARY KEY, provider TEXT NOT NULL, actor TEXT NOT NULL, expires_at INTEGER NOT NULL); \
         CREATE INDEX IF NOT EXISTS idx_publish_oauth_states_expiry ON publish_oauth_states(expires_at);",
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    pub async fn create_publish_oauth_state(
        &self,
        provider: &str,
        actor: &str,
        state: &str,
    ) -> Result<()> {
        let now = now_secs();
        let mut transaction = self.pool.begin().await?;
        sqlx::query("DELETE FROM publish_oauth_states WHERE expires_at <= ?")
            .bind(i64::try_from(now)?)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("INSERT INTO publish_oauth_states (state_hash, provider, actor, expires_at) VALUES (?, ?, ?, ?)")
            .bind(hash_state(state))
            .bind(provider)
            .bind(actor)
            .bind(i64::try_from(now + OAUTH_STATE_TTL_SECS)?)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn consume_publish_oauth_state(
        &self,
        provider: &str,
        state: &str,
    ) -> Result<Option<String>> {
        let row: Option<String> = sqlx::query_scalar(
            "DELETE FROM publish_oauth_states WHERE state_hash = ? AND provider = ? AND expires_at > ? RETURNING actor",
        )
        .bind(hash_state(state))
        .bind(provider)
        .bind(i64::try_from(now_secs())?)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }
}

fn hash_state(state: &str) -> String {
    format!("{:x}", Sha256::digest(state.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn oauth_state_is_hashed_and_consumed_once() {
        let directory = tempfile::tempdir().unwrap();
        let db = Db::open(directory.path()).await.unwrap();
        db.create_publish_oauth_state("youtube", "alice", "raw-secret-state")
            .await
            .unwrap();
        assert_eq!(
            db.consume_publish_oauth_state("youtube", "raw-secret-state")
                .await
                .unwrap()
                .as_deref(),
            Some("alice")
        );
        assert_eq!(
            db.consume_publish_oauth_state("youtube", "raw-secret-state")
                .await
                .unwrap(),
            None
        );
    }
}
