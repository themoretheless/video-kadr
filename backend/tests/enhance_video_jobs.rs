mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::{
    config::resource_classes::ResourceClass, db::SpaceRole, library::MediaEntry, model::JobStatus,
    services::local_enhance::Config, state::AppState,
};
fn request(method: &str, path: &str, token: &str, data: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(data.to_string()))
        .unwrap()
}
async fn value(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}
async fn terminal(state: &AppState, id: &str) -> video_kadr_backend::model::Job {
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if matches!(
                job.status,
                JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
            ) {
                return job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
#[ignore = "requires native learned enhancer, public model, ONNX Runtime and host FFmpeg/FFprobe"]
async fn actual_private_video_jobs_recovery_running_cancel_source_acl_and_model_pin() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        model: std::env::var("VIDEO_KADR_TEST_ENHANCE_MODEL")
            .unwrap()
            .into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    for mode in [
        "recover",
        "running",
        "changed",
        "revoked",
        "engine_changed",
        "managed_removed",
    ] {
        eprintln!("video enhancement mode: {mode}");
        let (state, _dir) = support::make_state(true, false).await;
        let state = state.with_enhance_config(Some(config.clone()));
        let owner = state
            .db
            .register_auth_user("video-owner", "public video test password")
            .await
            .unwrap()
            .unwrap();
        let viewer = state
            .db
            .register_auth_user("video-viewer", "public video test password")
            .await
            .unwrap()
            .unwrap();
        let source = state.sources_dir().join("synthetic.mov");
        let generated = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=16x24:rate=2",
                "-f",
                "lavfi",
                "-i",
                if mode == "running" {
                    "anoisesrc=color=white:sample_rate=48000:seed=1"
                } else {
                    "sine=frequency=440:sample_rate=48000"
                },
                "-t",
                if mode == "running" { "3" } else { "1" },
                "-c:v",
                "qtrle",
                "-pix_fmt",
                "argb",
                "-c:a",
                "aac",
                "-b:a",
                if mode == "running" { "256k" } else { "64k" },
            ])
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            generated.status.success(),
            "{}",
            String::from_utf8_lossy(&generated.stderr)
        );
        assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"video-source","filename":"synthetic.mov","url":"/files/sources/synthetic.mov","mediaType":"video"}))).await);
        let space = state
            .db
            .create_space(&owner.user.username, "Video enhancement")
            .await
            .unwrap();
        let project = state
            .db
            .create_space_composition_project(
                "Video",
                &json!({"schemaVersion":1,"sources":{"video-source":{"id":"video-source"}}}),
                &["video-source".into()],
                &owner.user.username,
                &space.id,
            )
            .await
            .unwrap();
        let app = support::router(state.clone());
        let data = json!({"sourceId":"video-source","factor":2});
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    "/api/enhance/video-jobs",
                    &viewer.token,
                    data.clone()
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        for body in [
            json!({"sourceId":"video-source","factor":3}),
            json!({"sourceId":"video-source","factor":2,"path":"/etc/passwd"}),
        ] {
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "POST",
                        "/api/enhance/video-jobs",
                        &owner.token,
                        body
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
        }
        if mode == "managed_removed" {
            let admitted = app
                .clone()
                .oneshot(request(
                    "POST",
                    "/api/enhance/models/downloads",
                    &owner.token,
                    json!({"catalogId":"realesr-general-x4v3"}),
                ))
                .await
                .unwrap();
            assert_eq!(admitted.status(), StatusCode::ACCEPTED);
            let download = value(admitted).await["jobId"].as_str().unwrap().to_owned();
            assert_eq!(terminal(&state, &download).await.status, JobStatus::Done);
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "POST",
                        "/api/enhance/models/select",
                        &owner.token,
                        json!({"modelId":"realesr-general-x4v3"})
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
        }
        let actor = if mode == "revoked" {
            state
                .db
                .set_space_member(
                    &space.id,
                    &owner.user.username,
                    &viewer.user.username,
                    SpaceRole::Viewer,
                )
                .await
                .unwrap()
                .unwrap();
            &viewer
        } else {
            &owner
        };
        let mut held = vec![];
        for _ in 0..state.resource_limit(ResourceClass::Analysis) {
            held.push(
                state
                    .acquire_job_slot(ResourceClass::Analysis)
                    .await
                    .unwrap(),
            );
        }
        let admitted = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/enhance/video-jobs",
                &actor.token,
                data.clone(),
            ))
            .await
            .unwrap();
        assert_eq!(admitted.status(), StatusCode::ACCEPTED);
        let id = value(admitted).await["jobId"].as_str().unwrap().to_owned();
        let cancelled = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/enhance/video-jobs",
                &actor.token,
                data,
            ))
            .await
            .unwrap();
        assert_eq!(cancelled.status(), StatusCode::ACCEPTED);
        let cancelled = value(cancelled).await["jobId"].as_str().unwrap().to_owned();
        let outsider = if mode == "revoked" { &owner } else { &viewer };
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
                        &outsider.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    &format!("/api/jobs/{cancelled}/cancel"),
                    &actor.token,
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let first = value(
            app.clone()
                .oneshot(request(
                    "GET",
                    "/api/enhance/video-jobs?sourceId=video-source&limit=1",
                    &actor.token,
                    Value::Null,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(first["jobs"].as_array().unwrap().len(), 1);
        assert_eq!(first["jobs"][0]["factor"], 2);
        let cursor = first["nextCursor"].as_str().unwrap();
        let second = value(
            app.clone()
                .oneshot(request(
                    "GET",
                    &format!(
                        "/api/enhance/video-jobs?sourceId=video-source&limit=1&cursor={cursor}"
                    ),
                    &actor.token,
                    Value::Null,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(second["jobs"].as_array().unwrap().len(), 1);
        assert_ne!(first["jobs"][0]["jobId"], second["jobs"][0]["jobId"]);
        let outside = value(
            app.clone()
                .oneshot(request(
                    "GET",
                    "/api/enhance/video-jobs",
                    &outsider.token,
                    Value::Null,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert!(outside["jobs"].as_array().unwrap().is_empty());
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "GET",
                    &format!("/api/enhance/video-jobs?cursor={cursor}"),
                    &outsider.token,
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        let path = state
            .library
            .resolve_media_path(&state.library.get("video-source").await.unwrap())
            .await
            .unwrap();
        if mode == "changed" {
            std::fs::write(&path, b"changed after admission").unwrap();
        }
        if mode == "revoked" {
            state
                .db
                .remove_space_member(&space.id, &owner.user.username, &viewer.user.username)
                .await
                .unwrap()
                .unwrap();
        }
        if mode == "managed_removed" {
            let removed = app
                .clone()
                .oneshot(request(
                    "POST",
                    "/api/enhance/models/remove",
                    &owner.token,
                    json!({"modelId":"realesr-general-x4v3"}),
                ))
                .await
                .unwrap();
            assert_eq!(removed.status(), StatusCode::OK);
            assert_eq!(value(removed).await["fileReleased"], false);
        }
        if mode == "running" {
            held.clear();
            tokio::time::timeout(std::time::Duration::from_secs(90), async {
                loop {
                    let job = state.get_job(&id).await.unwrap().unwrap();
                    assert!(
                        !matches!(
                            job.status,
                            JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
                        ),
                        "must observe running native frames: {job:?}"
                    );
                    if job.status == JobStatus::Running && job.progress.unwrap_or(0.) > 10. {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
        }
        state.begin_shutdown();
        drop(held);
        assert!(
            state
                .wait_for_tasks(std::time::Duration::from_secs(10))
                .await
        );
        let mut restored_config = config.clone();
        if mode == "engine_changed" {
            restored_config.runtime_library = path.clone();
        }
        if mode == "managed_removed" {
            restored_config.model = state.storage.join("missing-default.onnx");
        }
        let recovered = AppState::new(
            state.storage.clone(),
            2,
            (*state.tools).clone(),
            video_kadr_backend::library::Library::load(state.storage.clone()).await,
            video_kadr_backend::db::Db::open(&state.storage)
                .await
                .unwrap(),
        )
        .with_enhance_config(Some(restored_config));
        recovered.recover_jobs().await;
        video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
        let job = terminal(&recovered, &id).await;
        assert_eq!(
            terminal(&recovered, &cancelled).await.status,
            JobStatus::Cancelled
        );
        let app = support::router(recovered.clone());
        if matches!(mode, "recover" | "running" | "managed_removed") {
            assert_eq!(job.status, JobStatus::Done, "{mode}: {job:?}");
            let meta = job.result.unwrap();
            assert_eq!(meta["width"], 32);
            assert_eq!(meta["height"], 48);
            assert_eq!(meta["frames"], if mode == "running" { 6 } else { 2 });
            assert_eq!(meta["audioCopied"], true);
            assert_eq!(meta["factor"], 2);
            let url = format!("/api/enhance/video-jobs/{id}/video");
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &url, &outsider.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            let mut range = request("GET", &url, &actor.token, Value::Null);
            range
                .headers_mut()
                .insert("range", "bytes=0-31".parse().unwrap());
            let response = app.clone().oneshot(range).await.unwrap();
            assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
            assert_eq!(response.headers()["content-type"], "video/mp4");
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert_eq!(
                to_bytes(response.into_body(), 1024).await.unwrap().len(),
                32
            );
            let response = app
                .clone()
                .oneshot(request("GET", &url, &actor.token, Value::Null))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            let result_path = recovered
                .storage
                .join("enhance-video-jobs")
                .join(format!("{id}.mp4"));
            assert_eq!(bytes.as_ref(), std::fs::read(&result_path).unwrap());
            assert!(recovered.library.get(&id).await.is_none());
            let audio = |input: &std::path::Path| {
                let out = std::process::Command::new("ffmpeg")
                    .args(["-nostdin", "-v", "error", "-i"])
                    .arg(input)
                    .args(["-map", "0:a:0", "-c", "copy", "-f", "data", "pipe:1"])
                    .output()
                    .unwrap();
                assert!(out.status.success());
                out.stdout
            };
            assert_eq!(audio(&path), audio(&result_path));
            let asset_url = format!("/api/enhance/video-jobs/{id}/asset");
            let target = json!({"projectId":project.id});
            assert_eq!(
                app.clone()
                    .oneshot(request("POST", &asset_url, &outsider.token, target.clone()))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            let accepted = app
                .clone()
                .oneshot(request("POST", &asset_url, &actor.token, target.clone()))
                .await
                .unwrap();
            assert_eq!(accepted.status(), StatusCode::OK);
            let accepted = value(accepted).await;
            assert_eq!(accepted["mediaType"], "video");
            assert_eq!(accepted["width"], 32);
            assert_eq!(accepted["height"], 48);
            assert_eq!(accepted["fps"], 2.);
            assert_eq!(accepted["vcodec"], "h264");
            assert_eq!(accepted["acodec"], "aac");
            assert_eq!(accepted["sizeBytes"], bytes.len());
            if mode == "running" {
                assert!(
                    bytes.len() > 65536,
                    "running fixture must cross streaming copy buffer boundary"
                );
            }
            let repeated = app
                .clone()
                .oneshot(request("POST", &asset_url, &actor.token, target.clone()))
                .await
                .unwrap();
            assert_eq!(repeated.status(), StatusCode::OK);
            assert_eq!(value(repeated).await["id"], accepted["id"]);
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        accepted["url"].as_str().unwrap(),
                        &actor.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        accepted["url"].as_str().unwrap(),
                        &outsider.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            let entry = recovered
                .library
                .get(accepted["id"].as_str().unwrap())
                .await
                .unwrap();
            let accepted_path = recovered.library.resolve_media_path(&entry).await.unwrap();
            assert_eq!(std::fs::read(&accepted_path).unwrap(), bytes.as_ref());
            assert_eq!(audio(&path), audio(&accepted_path));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(&accepted_path)
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o600
                );
            }
            std::fs::write(&accepted_path, b"changed accepted copy").unwrap();
            assert_eq!(
                app.clone()
                    .oneshot(request("POST", &asset_url, &actor.token, target.clone()))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CONFLICT
            );
            assert_eq!(
                std::fs::read(&accepted_path).unwrap(),
                b"changed accepted copy"
            );
            assert_eq!(std::fs::read(&result_path).unwrap(), bytes.as_ref());
            std::fs::write(&accepted_path, &bytes).unwrap();

            std::fs::write(&result_path, b"corrupt completed video").unwrap();
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &url, &actor.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CONFLICT
            );
            std::fs::write(&result_path, &bytes).unwrap();
            recovered.begin_shutdown();
            assert!(
                recovered
                    .wait_for_tasks(std::time::Duration::from_secs(10))
                    .await
            );
            let reopened = AppState::new(
                recovered.storage.clone(),
                2,
                (*recovered.tools).clone(),
                video_kadr_backend::library::Library::load(recovered.storage.clone()).await,
                video_kadr_backend::db::Db::open(&recovered.storage)
                    .await
                    .unwrap(),
            );
            reopened.recover_jobs().await;
            let fresh = support::router(reopened.clone());
            assert_eq!(
                fresh
                    .clone()
                    .oneshot(request("GET", &url, &actor.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
            let restored_asset = fresh
                .clone()
                .oneshot(request("POST", &asset_url, &actor.token, target))
                .await
                .unwrap();
            assert_eq!(restored_asset.status(), StatusCode::OK);
            assert_eq!(value(restored_asset).await["id"], accepted["id"]);
            let history = value(
                fresh
                    .oneshot(request(
                        "GET",
                        "/api/enhance/video-jobs",
                        &actor.token,
                        Value::Null,
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(history["jobs"].as_array().unwrap().len(), 2);
            reopened.begin_shutdown();
            assert!(
                reopened
                    .wait_for_tasks(std::time::Duration::from_secs(5))
                    .await
            );
        } else {
            assert_eq!(job.status, JobStatus::Error, "{mode}: {job:?}");
            assert_eq!(
                job.error_kind.as_deref(),
                Some(if mode == "revoked" {
                    "security"
                } else {
                    "validation"
                })
            );
            assert!(!recovered
                .storage
                .join("enhance-video-jobs")
                .join(format!("{id}.mp4"))
                .exists());
        }
        recovered.begin_shutdown();
        assert!(
            recovered
                .wait_for_tasks(std::time::Duration::from_secs(10))
                .await
        );
    }
}
