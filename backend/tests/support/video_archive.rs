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
    // Simulate a pre-reservation cache. Never backfill from a failed read.
    // Duplicate receipt registrations are legal in the older cache schema.
    sqlx::query("INSERT INTO concat_video_project_caches(id,project_id,source_id,video_hash,model_hash,runtime_hash,manifest_hash,generation) SELECT ?,project_id,source_id,video_hash,model_hash,runtime_hash,manifest_hash,generation FROM concat_video_project_caches WHERE project_id=? LIMIT 1").bind(format!("legacy-copy-{}",uuid::Uuid::new_v4())).bind(id).execute(&pool).await.unwrap();

    sqlx::query("DELETE FROM concat_video_project_source_refs WHERE project_id=?")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let preview = |project: Value| {
        Request::builder().method("POST").uri("/api/compositions/nested-preview").header("content-type","application/json").header("authorization",format!("Bearer {}",user.token)).body(Body::from(json!({"project":project,"compositionId":node,"timeUs":time,"renderedOutputs":{},"savedProject":{"projectId":id,"expectedRevision":saved.revision}}).to_string())).unwrap()
    };
    let mut wrong = project.clone();
    wrong["compositions"][node]["content"]["tracks"][0]["clips"][0]["concatCutout"]["sourceHash"] =
        json!("0".repeat(64));
    assert!(!app
        .clone()
        .oneshot(preview(wrong))
        .await
        .unwrap()
        .status()
        .is_success());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM concat_video_project_source_refs WHERE project_id=?"
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    sqlx::query("CREATE TRIGGER reject_legacy_original BEFORE INSERT ON concat_video_project_source_refs BEGIN SELECT RAISE(ABORT,'controlled legacy adoption rejection'); END").execute(&pool).await.unwrap();
    assert!(!app
        .clone()
        .oneshot(preview(project.clone()))
        .await
        .unwrap()
        .status()
        .is_success());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM concat_video_project_source_refs WHERE project_id=?"
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER reject_legacy_original")
        .execute(&pool)
        .await
        .unwrap();
    let request=Request::builder().method("POST").uri("/api/compositions/nested-preview").header("content-type","application/json").header("authorization",format!("Bearer {}",user.token)).body(Body::from(json!({"project":project,"compositionId":node,"timeUs":time,"renderedOutputs":{},"savedProject":{"projectId":id,"expectedRevision":saved.revision}}).to_string())).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
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
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM concat_video_project_source_refs WHERE project_id=?"
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );

    if regions {
        Box::pin(history_after_import(
            &state,
            &app,
            &user.token,
            &saved,
            &project,
            node,
            time,
            expected,
        ))
        .await;
    } else {
        Box::pin(original_after_import(
            &state,
            &app,
            &user.token,
            &saved,
            &project,
            node,
            time,
            expected,
        ))
        .await;
    }
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
}
async fn assert_clean(state: &video_kadr_backend::state::AppState, pool: &sqlx::SqlitePool) {
    assert!(state.library.list().await.is_empty());
    assert_eq!(count_files(&state.sources_dir()), 0);
    for table in [
        "composition_projects",
        "concat_video_project_caches",
        "concat_video_project_source_refs",
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

async fn history_after_import(
    state: &video_kadr_backend::state::AppState,
    app: &axum::Router,
    token: &str,
    saved: &video_kadr_backend::db::CompositionProject,
    snapshot: &Value,
    node: &str,
    time: u64,
    expected: &[u8],
) {
    use video_kadr_backend::domain::{
        composition::SourceId, nested_composition::NestedCompositionProject,
    };
    let first = &snapshot["compositions"][node]["content"]["tracks"][0]["clips"][0];
    let logical = first["concatCutout"]["regionTracks"][0]["snapshot"]["maskSourceId"]
        .as_str()
        .unwrap();
    let original = first["sourceId"].as_str().unwrap();
    let typed: NestedCompositionProject = serde_json::from_value(snapshot.clone()).unwrap();
    let physical = typed
        .library_source_id(&SourceId::parse(logical).unwrap())
        .as_str()
        .to_owned();
    assert_eq!(
        state
            .db
            .composition_source_reference_count(&physical)
            .await
            .unwrap(),
        1
    );
    let mut undone = saved.document.clone();
    for node in undone["compositions"].as_object_mut().unwrap().values_mut() {
        for track in node["content"]["tracks"].as_array_mut().unwrap() {
            if let Some(clips) = track.get_mut("clips").and_then(Value::as_array_mut) {
                for clip in clips {
                    clip.as_object_mut().unwrap().remove("concatCutout");
                }
            }
        }
    }
    // Rebind the inactive bin slot to the original, keeping the historical file
    // reachable only through its exact imported reservation.
    let mut replacement = undone["compositions"][node]["content"]["sources"][original].clone();
    replacement["id"] = json!(logical);
    undone["compositions"][node]["content"]["sources"][logical] = replacement.clone();
    let raw = typed
        .library_source_id(&SourceId::parse(original).unwrap())
        .as_str()
        .to_owned();
    undone["projectMedia"][logical] = json!({"source":replacement,"librarySourceId":raw,"name":"Other live source","placeholder":false});
    let current: NestedCompositionProject = serde_json::from_value(undone.clone()).unwrap();
    current.dependency_order().unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for document in current.compositions.values() {
        for source in document.content.sources.keys() {
            ids.insert(current.library_source_id(source).as_str().to_owned());
        }
    }
    for source in current.project_media.keys() {
        ids.insert(current.library_source_id(source).as_str().to_owned());
    }
    let ids: Vec<_> = ids.into_iter().collect();
    assert!(!ids.contains(&physical));
    let current = state
        .db
        .update_composition_project_for(
            &saved.id,
            "archive-owner",
            &saved.name,
            &undone,
            &ids,
            saved.revision,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        state
            .db
            .composition_source_reference_count(&physical)
            .await
            .unwrap(),
        1
    );
    let delete = || {
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/library/{physical}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        app.clone().oneshot(delete()).await.unwrap().status(),
        StatusCode::CONFLICT
    );
    assert!(state.library.get(&physical).await.is_some());
    let request=Request::builder().method("POST").uri("/api/compositions/nested-preview").header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(Body::from(json!({"project":snapshot,"compositionId":node,"timeUs":time,"renderedOutputs":{},"savedProject":{"projectId":saved.id,"expectedRevision":current.revision}}).to_string())).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
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
    let empty = serde_json::to_value(video_kadr_backend::domain::composition::Composition::new(
        Default::default(),
    ))
    .unwrap();
    let cleared = state
        .db
        .update_composition_project_for(
            &saved.id,
            "archive-owner",
            &saved.name,
            &empty,
            &[],
            current.revision,
        )
        .await
        .unwrap()
        .unwrap();
    Box::pin(assert_historical_preview(
        state, app, token, &cleared, snapshot, node, time, expected,
    ))
    .await;
    assert!(state
        .db
        .delete_composition_project_for(&saved.id, "archive-owner")
        .await
        .unwrap());
    assert_eq!(
        state
            .db
            .composition_source_reference_count(&physical)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        app.clone().oneshot(delete()).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    assert!(state.library.get(&physical).await.is_none());
}

async fn original_after_import(
    state: &video_kadr_backend::state::AppState,
    app: &axum::Router,
    token: &str,
    saved: &video_kadr_backend::db::CompositionProject,
    snapshot: &Value,
    node: &str,
    time: u64,
    expected: &[u8],
) {
    let physical = saved.source_ids[0].clone();
    assert_eq!(saved.source_ids.len(), 1);
    let empty = serde_json::to_value(video_kadr_backend::domain::composition::Composition::new(
        Default::default(),
    ))
    .unwrap();
    let current = state
        .db
        .update_composition_project_for(
            &saved.id,
            "archive-owner",
            &saved.name,
            &empty,
            &[],
            saved.revision,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        state
            .db
            .composition_source_reference_count(&physical)
            .await
            .unwrap(),
        1
    );
    Box::pin(assert_historical_preview(
        state, app, token, &current, snapshot, node, time, expected,
    ))
    .await;
    let delete = || {
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/library/{physical}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        app.clone().oneshot(delete()).await.unwrap().status(),
        StatusCode::CONFLICT
    );
    assert!(state
        .db
        .delete_composition_project_for(&saved.id, "archive-owner")
        .await
        .unwrap());
    assert_eq!(
        state
            .db
            .composition_source_reference_count(&physical)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        app.clone().oneshot(delete()).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
}

async fn assert_historical_preview(
    state: &video_kadr_backend::state::AppState,
    app: &axum::Router,
    token: &str,
    current: &video_kadr_backend::db::CompositionProject,
    snapshot: &Value,
    node: &str,
    time: u64,
    expected: &[u8],
) {
    let preview = |project: Value, revision: u64| {
        Request::builder().method("POST").uri("/api/compositions/nested-preview").header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(Body::from(json!({"project":project,"compositionId":node,"timeUs":time,"renderedOutputs":{},"savedProject":{"projectId":current.id,"expectedRevision":revision}}).to_string())).unwrap()
    };
    let response = app
        .clone()
        .oneshot(preview(snapshot.clone(), current.revision))
        .await
        .unwrap();
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
    assert!(!app
        .clone()
        .oneshot(preview(snapshot.clone(), current.revision + 1))
        .await
        .unwrap()
        .status()
        .is_success());
    let mut wrong = snapshot.clone();
    wrong["compositions"][node]["content"]["tracks"][0]["clips"][0]["concatCutout"]["sourceHash"] =
        json!("0".repeat(64));
    assert!(!app
        .clone()
        .oneshot(preview(wrong, current.revision))
        .await
        .unwrap()
        .status()
        .is_success());
    let source = snapshot["compositions"][node]["content"]["tracks"][0]["clips"][0]["sourceId"]
        .as_str()
        .unwrap();
    let native: video_kadr_backend::domain::nested_composition::NestedCompositionProject =
        serde_json::from_value(snapshot.clone()).unwrap();
    let physical = native
        .library_source_id(
            &video_kadr_backend::domain::composition::SourceId::parse(source).unwrap(),
        )
        .as_str()
        .to_owned();
    let entry = state.library.get(physical.as_str()).await.unwrap();
    let original = state.library.resolve_media_path(&entry).await.unwrap();
    let copy_id = format!("unregistered-copy-{}", uuid::Uuid::new_v4());
    let filename = format!("{copy_id}.mkv");
    tokio::fs::copy(original, state.sources_dir().join(&filename))
        .await
        .unwrap();
    assert!(state.library.add(video_kadr_backend::library::MediaEntry::from_result("source",&json!({"id":copy_id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"video","width":entry.width,"height":entry.height}))).await);
    let mut copy = snapshot.clone();
    let descriptor = copy["compositions"][node]["content"]["sources"][source].clone();
    copy["projectMedia"][source] = json!({"source":descriptor,"librarySourceId":copy_id,"name":"Different physical copy","placeholder":false});
    assert!(
        !app.clone()
            .oneshot(preview(copy, current.revision))
            .await
            .unwrap()
            .status()
            .is_success(),
        "identical bytes must not borrow another physical file's reservation"
    );
    let delete = Request::builder()
        .method("DELETE")
        .uri(format!("/api/library/{copy_id}"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(delete).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE composition_project_members SET role='viewer' WHERE project_id=? AND actor='archive-owner'").bind(&current.id).execute(&pool).await.unwrap();
    assert!(!app
        .clone()
        .oneshot(preview(snapshot.clone(), current.revision))
        .await
        .unwrap()
        .status()
        .is_success());
    sqlx::query("UPDATE composition_project_members SET role='owner' WHERE project_id=? AND actor='archive-owner'").bind(&current.id).execute(&pool).await.unwrap();
    // Saving the old import snapshot must select this project's immutable original,
    // despite an irrelevant external origin retained in the historical wire data.
    let mut recovered = snapshot.clone();
    let source = recovered["compositions"][node]["content"]["tracks"][0]["clips"][0]["sourceId"]
        .as_str()
        .unwrap()
        .to_owned();
    recovered["compositions"][node]["content"]["tracks"][0]["clips"][0]["concatCutout"]["origin"] =
        json!({"projectId":"unavailable-old-origin","expectedRevision":1,"sourceId":"unavailable"});
    let update=Request::builder().method("PUT").uri(format!("/api/composition-projects/{}",current.id)).header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(Body::from(json!({"schemaVersion":2,"mode":"composition","name":current.name,"document":recovered,"baseRevision":current.revision}).to_string())).unwrap();
    let response = app.clone().oneshot(update).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    assert!(
        status.is_success(),
        "source {source}: {}",
        String::from_utf8_lossy(&body)
    );
}
