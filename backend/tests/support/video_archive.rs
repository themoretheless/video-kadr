//! Cross-storage HTTP qualification, with no original projects or inference engines.
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use video_kadr_backend::{handlers::concat_video_cache, project_archive::*};

pub async fn import_and_preview(
    bytes: &[u8],
    composition: Option<&str>,
    time: u64,
    expected: &[u8],
    regions: bool,
) {
    let (state, _dir) = super::make_state(true, false).await;
    let state = state
        .with_person_config(None)
        .with_selfie_config(None)
        .with_sam_config(None);
    let user = state
        .db
        .register_auth_user("archive-owner", "synthetic qualification password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space("archive-owner", "Imported masks")
        .await
        .unwrap();
    let app = super::router(state.clone());
    let upload = |bytes: &[u8]| {
        let boundary = "MASK_ARCHIVE_TEST";
        let mut body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"project.veproj\"\r\nContent-Type: application/vnd.video-editor.project\r\n\r\n").into_bytes();
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        Request::builder()
            .method("POST")
            .uri("/api/composition-projects/import")
            .header("authorization", format!("Bearer {}", user.token))
            .header("x-space-id", &space.id)
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .unwrap()
    };
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(state.storage.join("app.db"))
                .foreign_keys(true),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE collaboration_space_members SET role='viewer' WHERE space_id=? AND actor='archive-owner'").bind(&space.id).execute(&pool).await.unwrap();
    assert_eq!(
        app.clone().oneshot(upload(bytes)).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    assert_clean(&state, &pool).await;
    sqlx::query("UPDATE collaboration_space_members SET role='owner' WHERE space_id=? AND actor='archive-owner'").bind(&space.id).execute(&pool).await.unwrap();
    // Outer checksums are valid while the portable mask bytes have been corrupted.
    let forge = tempfile::tempdir().unwrap();
    let parsed = read_project_archive(bytes, forge.path()).await.unwrap();
    let mut manifest = parsed.manifest.clone();
    let mut corrupted = std::fs::read(parsed.video_caches[0].path()).unwrap();
    *corrupted.last_mut().unwrap() ^= 1;
    let altered = forge.path().join("corrupted-cache");
    std::fs::write(&altered, &corrupted).unwrap();
    manifest.video_caches[0].sha256 = format!("{:x}", Sha256::digest(&corrupted));
    let media: Vec<_> = manifest
        .sources
        .iter()
        .zip(&parsed.media)
        .map(|(source, payload)| ArchiveMediaInput {
            source_id: source.source_id.clone(),
            filename: source.filename.clone(),
            path: payload.path().to_owned(),
        })
        .collect();
    let caches: Vec<_> = manifest
        .video_caches
        .iter()
        .zip(&parsed.video_caches)
        .enumerate()
        .map(|(index, (entry, payload))| ArchiveVideoCacheInput {
            key: entry.key.clone(),
            path: if index == 0 {
                altered.clone()
            } else {
                payload.path().to_owned()
            },
        })
        .collect();
    let forged = forge.path().join("bad.veproj");
    write_project_archive_with_video_caches(
        tokio::fs::File::create(&forged).await.unwrap(),
        &manifest,
        &media,
        &caches,
    )
    .await
    .unwrap();
    let response = app
        .clone()
        .oneshot(upload(&std::fs::read(forged).unwrap()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_clean(&state, &pool).await;
    sqlx::query("CREATE TRIGGER revoke_import_space AFTER INSERT ON composition_projects BEGIN UPDATE collaboration_space_members SET role='viewer' WHERE actor='archive-owner'; END").execute(&pool).await.unwrap();
    assert_eq!(
        app.clone().oneshot(upload(bytes)).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    assert_clean(&state, &pool).await;
    sqlx::query("DROP TRIGGER revoke_import_space")
        .execute(&pool)
        .await
        .unwrap();
    let trigger = if regions {
        "CREATE TRIGGER reject_imported_masks BEFORE INSERT ON concat_video_project_region_refs BEGIN SELECT RAISE(ABORT,'controlled archived region rejection'); END"
    } else {
        "CREATE TRIGGER reject_imported_masks BEFORE INSERT ON concat_video_model_choices BEGIN SELECT RAISE(ABORT,'controlled archived cache rejection'); END"
    };
    sqlx::query(trigger).execute(&pool).await.unwrap();
    assert!(!app
        .clone()
        .oneshot(upload(bytes))
        .await
        .unwrap()
        .status()
        .is_success());
    assert_clean(&state, &pool).await;
    sqlx::query("DROP TRIGGER reject_imported_masks")
        .execute(&pool)
        .await
        .unwrap();
    let response = app.clone().oneshot(upload(bytes)).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&body)
    );
    let imported: Value = serde_json::from_slice(&body).unwrap();
    let id = imported["project"]["id"].as_str().unwrap();
    let saved = state
        .db
        .get_composition_project_for(id, "archive-owner")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        state
            .db
            .list_composition_projects_for("archive-owner")
            .await
            .unwrap()
            .len(),
        1
    );
    let mut project = if saved.document.get("compositions").is_some() {
        saved.document.clone()
    } else {
        json!({"schemaVersion":1,"root":"archive","compositions":{"archive":{"content":saved.document,"instances":[]}}})
    };
    let node = composition.unwrap_or("archive");
    project["compositions"][node]["content"]["tracks"][0]["clips"][0]["enabled"] = json!(true);
    let source = project["compositions"][node]["content"]["tracks"][0]["clips"][0]["sourceId"]
        .as_str()
        .unwrap();
    let binding =
        &project["compositions"][node]["content"]["tracks"][0]["clips"][0]["concatCutout"];
    assert!(binding.get("origin").is_none());
    let cancel = tokio_util::sync::CancellationToken::new();
    let cache = concat_video_cache::restore_for_model(
        &state,
        "archive-owner",
        id,
        source,
        saved.revision,
        binding["runtimeHash"].as_str().unwrap(),
        binding["modelHash"].as_str().unwrap(),
        &cancel,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        cache.identity().unwrap(),
        parsed.manifest.video_caches[0].identity.cache_identity
    );
    let refs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM concat_video_project_region_refs WHERE project_id=?",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(refs, if regions { 1 } else { 0 });
    let request=Request::builder().method("POST").uri("/api/compositions/nested-preview").header("content-type","application/json").header("authorization",format!("Bearer {}",user.token)).body(Body::from(json!({"project":project,"compositionId":node,"timeUs":time,"renderedOutputs":{},"savedProject":{"projectId":id,"expectedRevision":saved.revision}}).to_string())).unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let pixels = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&pixels)
    );
    assert_eq!(pixels.as_ref(), expected);
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
}
async fn assert_clean(state: &video_kadr_backend::state::AppState, pool: &sqlx::SqlitePool) {
    assert!(state.library.list().await.is_empty());
    assert_eq!(count_files(&state.sources_dir()), 0);
    for table in [
        "composition_projects",
        "concat_video_project_caches",
        "concat_video_model_choices",
        "concat_video_project_region_refs",
        "library_metadata",
    ] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
    for path in [
        state.storage.join("concat-video-caches"),
        state.storage.join("concat-video-region-stages"),
        state.staging_dir(),
    ] {
        if path.exists() {
            assert_eq!(std::fs::read_dir(path).unwrap().count(), 0);
        }
    }
}

fn count_files(root: &std::path::Path) -> usize {
    std::fs::read_dir(root)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                count_files(&entry.path())
            } else {
                1
            }
        })
        .sum()
}
