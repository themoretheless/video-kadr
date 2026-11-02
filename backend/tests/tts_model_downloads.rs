mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
fn request(method: &str, path: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut r = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        r = r.header("authorization", format!("Bearer {token}"));
    }
    r.body(Body::from(body.to_string())).unwrap()
}
async fn value(r: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(r.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}
#[tokio::test]
async fn tts_catalog_queue_access_and_cancel() {
    let (state, _dir) = support::make_state(true, false).await;
    let a = state
        .db
        .register_auth_user("download-a", "download test password")
        .await
        .unwrap()
        .unwrap();
    let b = state
        .db
        .register_auth_user("download-b", "download test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let model = video_kadr_backend::services::tts_model_catalog::ModelCatalog::bundled().unwrap();
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                "/api/speech/tts-model-downloads",
                None,
                json!({"catalogId":model.id})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    for body in [
        json!({"catalogId":"https://example.org/model"}),
        json!({"catalogId":model.id,"url":"https://example.org"}),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    "/api/speech/tts-model-downloads",
                    Some(&a.token),
                    body
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let mut permits = vec![];
    for _ in
        0..state.resource_limit(video_kadr_backend::config::resource_classes::ResourceClass::Ingest)
    {
        permits.push(
            state
                .acquire_job_slot(
                    video_kadr_backend::config::resource_classes::ResourceClass::Ingest,
                )
                .await
                .unwrap(),
        );
    }
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/speech/tts-model-downloads",
            Some(&a.token),
            json!({"catalogId":model.id}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let job = value(response).await;
    let id = job["jobId"].as_str().unwrap();
    for (method, suffix) in [
        ("GET", ""),
        ("POST", "/cancel"),
        ("POST", "/retry"),
        ("POST", "/discard"),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request(
                    method,
                    &format!("/api/jobs/{id}{suffix}"),
                    Some(&b.token),
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    let own = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/speech/tts-model-downloads",
                Some(&a.token),
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(own["jobs"][0]["jobId"], id);
    let other = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/speech/tts-model-downloads",
                Some(&b.token),
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(other["jobs"].as_array().unwrap().is_empty());
    assert_eq!(
        app.clone()
            .oneshot(request(
                "GET",
                &format!("/api/speech/tts-model-downloads?cursor={id}"),
                Some(&b.token),
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                &format!("/api/jobs/{id}/cancel"),
                Some(&a.token),
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    drop(permits);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if state.get_job(id).await.unwrap().unwrap().status
                == video_kadr_backend::model::JobStatus::Cancelled
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let library = value(
        app.oneshot(request(
            "GET",
            "/api/speech/tts-models",
            Some(&a.token),
            Value::Null,
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(library["models"].as_array().unwrap().is_empty());
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[tokio::test]
#[ignore = "downloads the explicitly selected complete pinned public TTS bundle"]
async fn fresh_public_bundle_download_and_explicit_selection() {
    use video_kadr_backend::{
        db::Db,
        library::Library,
        model::JobStatus,
        state::{AppState, ToolInfo},
    };
    let root = std::path::PathBuf::from(
        std::env::var("VIDEO_KADR_TTS_DOWNLOAD_TEST_STORAGE").expect("fresh retained test storage"),
    );
    assert!(root.is_absolute());
    std::fs::create_dir(&root).expect("fresh storage must not already exist");
    let state = AppState::new(
        root.clone(),
        2,
        ToolInfo::default(),
        Library::load(root.clone()).await,
        Db::open(&root).await.unwrap(),
    );
    let state = if let (Ok(executable), Ok(library)) = (
        std::env::var("VIDEO_KADR_TEST_TTS_BIN"),
        std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY"),
    ) {
        state.with_chatterbox_runtime(Some(
            video_kadr_backend::services::local_chatterbox::Runtime {
                executable: executable.into(),
                library: library.into(),
            },
        ))
    } else {
        state
    };
    let owner = state
        .db
        .register_auth_user("tts-fresh-download", "download test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let catalog = match std::env::var("VIDEO_KADR_TTS_DOWNLOAD_TEST_CATALOG") {
        Ok(id) => video_kadr_backend::services::tts_model_catalog::ModelCatalog::find(&id)
            .unwrap()
            .unwrap(),
        Err(_) => video_kadr_backend::services::tts_model_catalog::ModelCatalog::bundled().unwrap(),
    };
    let admitted = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/speech/tts-model-downloads",
            Some(&owner.token),
            json!({"catalogId":catalog.id}),
        ))
        .await
        .unwrap();
    assert_eq!(admitted.status(), StatusCode::ACCEPTED);
    let admitted = value(admitted).await;
    let id = admitted["jobId"].as_str().unwrap();
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(900), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if matches!(
                job.status,
                JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    })
    .await;
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(10))
            .await
    );
    let job = outcome.unwrap();
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    catalog
        .verify_directory(&root.join("models/tts").join(&catalog.id))
        .unwrap();
    let library = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/speech/tts-models",
                Some(&owner.token),
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(library["selectedId"], "configured");
    assert_eq!(library["models"][0]["id"], catalog.id);
    assert_eq!(
        app.oneshot(request(
            "PUT",
            "/api/speech/tts-models/selection",
            Some(&owner.token),
            json!({"modelId":catalog.id})
        ))
        .await
        .unwrap()
        .status(),
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires pinned public Kokoro fixture and network for one missing file"]
async fn cold_queued_download_restores_verified_files_and_restarts_partial() {
    use sha2::{Digest, Sha256};
    use video_kadr_backend::{
        config::resource_classes::ResourceClass,
        db::Db,
        library::Library,
        model::JobStatus,
        state::{AppState, ToolInfo},
    };
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("tts-cold-download", "download test password")
        .await
        .unwrap()
        .unwrap();
    let catalog = video_kadr_backend::services::tts_model_catalog::ModelCatalog::bundled().unwrap();
    let source = std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_TTS_MODEL").unwrap());
    catalog.verify_directory(&source).unwrap();
    let mut permits = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Ingest) {
        permits.push(state.acquire_job_slot(ResourceClass::Ingest).await.unwrap());
    }
    let app = support::router(state.clone());
    let admitted = app
        .oneshot(request(
            "POST",
            "/api/speech/tts-model-downloads",
            Some(&owner.token),
            json!({"catalogId":catalog.id}),
        ))
        .await
        .unwrap();
    assert_eq!(admitted.status(), StatusCode::ACCEPTED);
    let admitted = value(admitted).await;
    let id = admitted["jobId"].as_str().unwrap();
    let staging = state.storage.join("tts-model-downloads").join(id);
    for file in &catalog.files {
        if file.path == "tokens.txt" {
            continue;
        }
        let target = staging.join(&file.path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(source.join(&file.path), target).unwrap();
    }
    let parts = staging.join(".download-parts");
    std::fs::create_dir(&parts).unwrap();
    let part = parts.join(format!("{:x}", Sha256::digest(b"tokens.txt")));
    std::fs::write(&part, b"incomplete").unwrap();
    state.begin_shutdown();
    drop(permits);
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let root = state.storage.clone();
    let recovered = AppState::new(
        root.clone(),
        2,
        ToolInfo::default(),
        Library::load(root.clone()).await,
        Db::open(&root).await.unwrap(),
    );
    recovered.recover_jobs().await;
    video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
    let app = support::router(recovered.clone());
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(90), async {
        loop {
            let job = recovered.get_job(id).await.unwrap().unwrap();
            if matches!(
                job.status,
                JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    recovered.begin_shutdown();
    assert!(
        recovered
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let job = outcome.unwrap();
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    let installed = root.join("models/tts").join(&catalog.id);
    catalog.verify_directory(&installed).unwrap();
    assert!(!staging.exists());
    assert!(!installed.join(".download-parts").exists());
    let library = value(
        app.oneshot(request(
            "GET",
            "/api/speech/tts-models",
            Some(&owner.token),
            Value::Null,
        ))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(library["models"][0]["id"], catalog.id);
    assert_eq!(library["selectedId"], "configured");
}

#[tokio::test]
#[ignore = "subprocess helper, run only by the SIGKILL qualification"]
async fn tts_download_worker_process() {
    use video_kadr_backend::{
        db::Db,
        library::Library,
        model::JobStatus,
        state::{AppState, ToolInfo},
    };
    let root = std::path::PathBuf::from(std::env::var_os("VIDEO_KADR_TTS_WORKER_STORAGE").unwrap());
    let id = std::env::var("VIDEO_KADR_TTS_WORKER_JOB").unwrap();
    let state = AppState::new(
        root.clone(),
        2,
        ToolInfo::default(),
        Library::load(root.clone()).await,
        Db::open(&root).await.unwrap(),
    );
    state.recover_jobs().await;
    video_kadr_backend::handlers::resume_pending_jobs(&state).await;
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(180), async {
        loop {
            let job = state.get_job(&id).await.unwrap().unwrap();
            if matches!(
                job.status,
                JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await;
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let job = outcome.unwrap();
    std::fs::write(
        root.join("tts-worker-result.json"),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
}
#[cfg(unix)]
#[tokio::test]
#[ignore = "requires pinned public Kokoro fixture and network for ONNX download twice"]
async fn killed_running_download_resumes_in_new_process() {
    use sha2::{Digest, Sha256};
    use std::os::unix::{fs::MetadataExt, process::ExitStatusExt};
    use video_kadr_backend::config::resource_classes::ResourceClass;
    struct Worker(std::process::Child);
    impl Drop for Worker {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }
    fn worker(root: &std::path::Path, id: &str, label: &str) -> Worker {
        let log = std::fs::File::create(root.join(format!("tts-worker-{label}.log"))).unwrap();
        Worker(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "tts_download_worker_process",
                    "--nocapture",
                ])
                .env("VIDEO_KADR_TTS_WORKER_STORAGE", root)
                .env("VIDEO_KADR_TTS_WORKER_JOB", id)
                .stdout(std::process::Stdio::from(log.try_clone().unwrap()))
                .stderr(std::process::Stdio::from(log))
                .spawn()
                .unwrap(),
        )
    }
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("tts-killed-download", "download test password")
        .await
        .unwrap()
        .unwrap();
    let catalog = video_kadr_backend::services::tts_model_catalog::ModelCatalog::bundled().unwrap();
    let source = std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_TTS_MODEL").unwrap());
    catalog.verify_directory(&source).unwrap();
    let mut permits = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Ingest) {
        permits.push(state.acquire_job_slot(ResourceClass::Ingest).await.unwrap());
    }
    let app = support::router(state.clone());
    let response = app
        .oneshot(request(
            "POST",
            "/api/speech/tts-model-downloads",
            Some(&owner.token),
            json!({"catalogId":catalog.id}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let admitted = value(response).await;
    let id = admitted["jobId"].as_str().unwrap();
    let staging = state.storage.join("tts-model-downloads").join(id);
    for file in &catalog.files {
        if file.path == "model.int8.onnx" {
            continue;
        }
        let target = staging.join(&file.path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(source.join(&file.path), target).unwrap();
    }
    let preserved_inode = std::fs::metadata(staging.join("voices.bin")).unwrap().ino();
    state.begin_shutdown();
    drop(permits);
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    let partial = staging
        .join(".download-parts")
        .join(format!("{:x}", Sha256::digest(b"model.int8.onnx")));
    let expected = catalog
        .files
        .iter()
        .find(|f| f.path == "model.int8.onnx")
        .unwrap()
        .bytes;
    let mut first = worker(&state.storage, id, "killed");
    let prefix = tokio::time::timeout(std::time::Duration::from_secs(90), async {
        loop {
            if let Some(status) = first.0.try_wait().unwrap() {
                panic!(
                    "worker ended before partial: {status}; {}",
                    std::fs::read_to_string(state.storage.join("tts-worker-killed.log")).unwrap()
                );
            }
            if let Ok(meta) = std::fs::metadata(&partial) {
                if meta.len() >= 1_000_000 && meta.len() < expected {
                    break meta.len();
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("download prefix");
    first.0.kill().unwrap();
    assert_eq!(first.0.wait().unwrap().signal(), Some(9));
    assert!(!state.storage.join("tts-worker-result.json").exists());
    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id=?")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "running");
    let registered: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tts_models")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(registered, 0);
    assert!(!staging.join("model.int8.onnx").exists());
    let mut second = worker(&state.storage, id, "resumed");
    let terminal = tokio::time::timeout(std::time::Duration::from_secs(180), async {
        loop {
            if let Some(status) = second.0.try_wait().unwrap() {
                break status;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("resumed worker terminal");
    assert!(
        terminal.success(),
        "{}",
        std::fs::read_to_string(state.storage.join("tts-worker-resumed.log")).unwrap()
    );
    let job: Value = serde_json::from_slice(
        &std::fs::read(state.storage.join("tts-worker-result.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(job["status"], "done");
    let installed = state.storage.join("models/tts").join(&catalog.id);
    catalog.verify_directory(&installed).unwrap();
    assert_eq!(
        std::fs::metadata(installed.join("voices.bin"))
            .unwrap()
            .ino(),
        preserved_inode
    );
    assert!(!installed.join(".download-parts").exists());
    let registered: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tts_models WHERE owner=? AND id=?")
            .bind(&owner.user.username)
            .bind(&catalog.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(registered, 1);
    eprintln!("SIGKILL after at least {prefix} ONNX bytes; same job completed in a new process with verified cached voices retained");
}

#[tokio::test]
async fn older_download_cannot_restore_removed_owner_model() {
    use video_kadr_backend::{config::resource_classes::ResourceClass, model::JobStatus};
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("tts-download-order", "download test password")
        .await
        .unwrap()
        .unwrap();
    let catalog = video_kadr_backend::services::tts_model_catalog::ModelCatalog::bundled().unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    // Metadata seed avoids network: removal of a missing installed bundle is valid.
    sqlx::query("INSERT INTO tts_models(owner,id,name,bytes) VALUES(?,?,?,?)")
        .bind(&owner.user.username)
        .bind(&catalog.id)
        .bind(&catalog.name)
        .bind(catalog.bytes as i64)
        .execute(&pool)
        .await
        .unwrap();
    let app = support::router(state.clone());
    let mut permits = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Ingest) {
        permits.push(state.acquire_job_slot(ResourceClass::Ingest).await.unwrap());
    }
    let admitted = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/speech/tts-model-downloads",
            Some(&owner.token),
            json!({"catalogId":catalog.id}),
        ))
        .await
        .unwrap();
    assert_eq!(admitted.status(), StatusCode::ACCEPTED);
    let admitted = value(admitted).await;
    let id = admitted["jobId"].as_str().unwrap();
    let removed = app
        .clone()
        .oneshot(request(
            "DELETE",
            "/api/speech/tts-models",
            Some(&owner.token),
            json!({"modelId":catalog.id}),
        ))
        .await
        .unwrap();
    assert_eq!(removed.status(), StatusCode::OK);
    drop(permits);
    let job = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if job.status == JobStatus::Error {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(job.error_kind.as_deref(), Some("validation"));
    assert!(job.error.unwrap().contains("запустите новое скачивание"));
    assert!(
        !state.storage.join("tts-model-downloads").join(id).exists(),
        "superseded request must stop before transfer"
    );
    let library = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/speech/tts-models",
                Some(&owner.token),
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(library["models"].as_array().unwrap().is_empty());
    let retry = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/jobs/{id}/retry"),
            Some(&owner.token),
            Value::Null,
        ))
        .await
        .unwrap();
    assert!(retry.status().is_client_error());
    let mut permits = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Ingest) {
        permits.push(state.acquire_job_slot(ResourceClass::Ingest).await.unwrap());
    }
    let fresh = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/speech/tts-model-downloads",
            Some(&owner.token),
            json!({"catalogId":catalog.id}),
        ))
        .await
        .unwrap();
    assert_eq!(fresh.status(), StatusCode::ACCEPTED);
    let fresh = value(fresh).await;
    let fresh_id = fresh["jobId"].as_str().unwrap();
    assert_ne!(fresh_id, id);
    let payload: String =
        sqlx::query_scalar("SELECT payload_json FROM job_requests WHERE job_id=?")
            .bind(fresh_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&payload).unwrap()["epoch"], 1);
    let cancelled = app
        .oneshot(request(
            "POST",
            &format!("/api/jobs/{fresh_id}/cancel"),
            Some(&owner.token),
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(cancelled.status(), StatusCode::OK);
    drop(permits);
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[tokio::test]
async fn legacy_removals_get_a_durable_epoch_without_resetting_newer_epochs() {
    let (state, _dir) = support::make_state(true, false).await;
    let catalog = video_kadr_backend::services::tts_model_catalog::ModelCatalog::bundled().unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO tts_model_removals(owner,id) VALUES('legacy',?)")
        .bind(&catalog.id)
        .execute(&pool)
        .await
        .unwrap();
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let reopened = video_kadr_backend::db::Db::open(&state.storage)
        .await
        .unwrap();
    let epoch: i64 =
        sqlx::query_scalar("SELECT epoch FROM tts_model_epochs WHERE owner='legacy' AND id=?")
            .bind(&catalog.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(epoch, 1);
    drop(reopened);
    sqlx::query("UPDATE tts_model_epochs SET epoch=7 WHERE owner='legacy'")
        .execute(&pool)
        .await
        .unwrap();
    let _reopened = video_kadr_backend::db::Db::open(&state.storage)
        .await
        .unwrap();
    let epoch: i64 =
        sqlx::query_scalar("SELECT epoch FROM tts_model_epochs WHERE owner='legacy' AND id=?")
            .bind(&catalog.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(epoch, 7);
}
