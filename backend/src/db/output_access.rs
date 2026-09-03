use anyhow::Result;
use sqlx::SqlitePool;

use super::Db;
use crate::library::now_secs;

pub(super) async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS output_access (output_id TEXT NOT NULL, actor TEXT NOT NULL, created_at INTEGER NOT NULL, PRIMARY KEY(output_id, actor)); \
         CREATE INDEX IF NOT EXISTS idx_output_access_actor ON output_access(actor, output_id);",
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl Db {
    pub async fn grant_output_access(&self, output_id: &str, actor: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO output_access (output_id, actor, created_at) VALUES (?, ?, ?) ON CONFLICT(output_id, actor) DO NOTHING",
        )
        .bind(output_id)
        .bind(actor)
        .bind(i64::try_from(now_secs())?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn can_access_output(&self, output_id: &str, actor: &str) -> Result<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM output_access WHERE output_id = ? AND actor = ?)",
        )
        .bind(output_id)
        .bind(actor)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn revoke_output_access(&self, output_id: &str) -> Result<u64> {
        Ok(sqlx::query("DELETE FROM output_access WHERE output_id = ?")
            .bind(output_id)
            .execute(&self.pool)
            .await?
            .rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn grants_are_idempotent_and_revoked_with_output() {
        let directory = tempfile::tempdir().unwrap();
        let db = Db::open(directory.path()).await.unwrap();
        db.grant_output_access("output-1", "alice").await.unwrap();
        db.grant_output_access("output-1", "alice").await.unwrap();
        assert!(db.can_access_output("output-1", "alice").await.unwrap());
        assert!(!db.can_access_output("output-1", "bob").await.unwrap());
        assert_eq!(db.revoke_output_access("output-1").await.unwrap(), 1);
        assert!(!db.can_access_output("output-1", "alice").await.unwrap());
    }
}
