mod support;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use video_kadr_backend::{
    domain::composition::Composition,
    services::concat_video_analysis::cleanup_retired_caches,
};

fn generation(root: &Path, parent: Option<Value>) -> (String, String) {
    let id = format!("concat-video-cache-{}", uuid::Uuid::new_v4());
    let directory = root.join(&id);
    fs::create_dir(&directory).unwrap();
    let mut manifest = json!({
        "schemaVersion":1, "videoHash":"fixture", "runtimeHash":"fixture", "runs":[],
    });
    if let Some(parent) = parent {
        manifest["schemaVersion"] = json!(2);
        manifest["totalRuns"] = json!(1);
        manifest["parent"] = parent;
    }
    let bytes = serde_json::to_vec(&manifest).unwrap();
    fs::write(directory.join("manifest.json"), &bytes).unwrap();
    (id, format!("{:x}", Sha256::digest(bytes)))
}

#[tokio::test]
async fn project_deletion_reclaims_only_generations_outside_live_cache_chains() {
    let (state, _dir) = support::make_state(false, false).await;
    let content = serde_json::to_value(Composition::new(Default::default())).unwrap();
    let document = json!({
        "schemaVersion":1,"root":"root","compositions":{
            "root":{"content":content,"instances":[]}
        }
    });
    let retired = state.db.create_composition_project("Retired", &document, &[]).await.unwrap();
    let current = state.db.create_composition_project("Current", &document, &[]).await.unwrap();
    let root = state.storage.join("concat-video-caches");
    fs::create_dir(&root).unwrap();
    let (prefix, prefix_hash) = generation(&root, None);
    let (child, child_hash) = generation(&root, Some(json!({
        "generation":prefix, "manifestHash":prefix_hash,
    })));
    let (old, old_hash) = generation(&root, None);
    let (unregistered, _) = generation(&root, None);
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")))
        .await.unwrap();
    for (project, id, hash) in [
        (&retired.id, &prefix, &prefix_hash),
        (&retired.id, &old, &old_hash),
        (&retired.id, &child, &child_hash),
        (&current.id, &child, &child_hash),
    ] {
        sqlx::query("INSERT INTO concat_video_project_caches
            (id,project_id,source_id,video_hash,runtime_hash,manifest_hash,generation)
            VALUES(?,?,'source','fixture','fixture',?,?)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(project).bind(hash).bind(id)
            .execute(&db).await.unwrap();
    }

    assert!(state.db.delete_composition_project(&retired.id).await.unwrap());
    // A corrupt live manifest must fail closed before any deletion.
    let child_manifest = root.join(&child).join("manifest.json");
    let saved = fs::read(&child_manifest).unwrap();
    fs::write(&child_manifest, b"corrupt").unwrap();
    assert!(cleanup_retired_caches(&state).await.is_err());
    assert!(root.join(&old).is_dir());
    fs::write(child_manifest, saved).unwrap();

    cleanup_retired_caches(&state).await.unwrap();
    assert!(!root.join(&old).exists());
    assert!(root.join(&prefix).is_dir(), "a live incremental child still needs its prefix");
    assert!(root.join(&child).is_dir(), "the second project holds a direct reference");
    assert!(root.join(&unregistered).is_dir(), "publication may still be in progress");

    assert!(state.db.delete_composition_project(&current.id).await.unwrap());
    cleanup_retired_caches(&state).await.unwrap();
    cleanup_retired_caches(&state).await.unwrap();
    assert!(!root.join(&prefix).exists());
    assert!(!root.join(&child).exists());
    assert!(root.join(&unregistered).is_dir());
}
