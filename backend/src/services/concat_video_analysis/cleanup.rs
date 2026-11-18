use super::{read_manifest, valid_generation, MAX_CACHE_MANIFEST};
use crate::state::AppState;
use anyhow::{ensure, Result};
use std::{collections::BTreeMap, io::ErrorKind};

/// Reclaim registered generations retired by project deletion. A live cache
/// can still depend on a retired generation through its incremental chain.
pub async fn cleanup_retired_caches(state: &AppState) -> Result<()> {
    let pool = state.db.pool();
    let retired: Vec<String> = sqlx::query_scalar(
        "SELECT r.generation FROM concat_video_retired_caches r
         WHERE NOT EXISTS(SELECT 1 FROM concat_video_project_caches c WHERE c.generation=r.generation)
         ORDER BY r.generation",
    )
    .fetch_all(pool)
    .await?;
    if retired.is_empty() {
        return Ok(());
    }
    let root = state.storage.join("concat-video-caches");
    match tokio::fs::symlink_metadata(&root).await {
        Ok(metadata) => ensure!(metadata.is_dir(), "video cache root must be a directory"),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    const ROOTS: &str = "SELECT DISTINCT generation,manifest_hash FROM concat_video_project_caches ORDER BY generation,manifest_hash";
    let registered: Vec<(String, String)> = sqlx::query_as(ROOTS).fetch_all(pool).await?;
    let mut pending = registered.clone();
    let mut live = BTreeMap::new();
    while let Some((generation, hash)) = pending.pop() {
        if let Some(existing) = live.get(&generation) {
            ensure!(existing == &hash, "conflicting live video cache identities");
            continue;
        }
        valid_generation(&generation)?;
        let (manifest, _) = read_manifest(&root.join(&generation), &hash, MAX_CACHE_MANIFEST).await?;
        if let Some(parent) = manifest.parent {
            pending.push((parent.generation, parent.manifest_hash));
        }
        live.insert(generation, hash);
    }
    let removable: Vec<_> = retired.iter().filter(|id| !live.contains_key(*id)).take(32).collect();
    if removable.is_empty() {
        return Ok(());
    }
    for generation in &removable {
        valid_generation(generation)?;
    }
    // Cache registration must stay stable between verifying ancestry and
    // deleting files. Do the filesystem reads before reserving the DB writer.
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let current: Vec<(String, String)> = sqlx::query_as(ROOTS).fetch_all(&mut *tx).await?;
    if current != registered {
        return Ok(());
    }
    for generation in removable {
        let path = root.join(generation);
        match tokio::fs::symlink_metadata(&path).await {
            Ok(metadata) => {
                ensure!(metadata.is_dir(), "retired video cache must be a directory");
                tokio::fs::remove_dir_all(path).await?;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        sqlx::query("DELETE FROM concat_video_retired_caches WHERE generation=?")
            .bind(generation)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}
