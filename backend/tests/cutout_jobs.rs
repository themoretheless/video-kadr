mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::{
    config::resource_classes::ResourceClass, db::SpaceRole, library::MediaEntry, model::JobStatus,
    services::local_cutout::Config, state::AppState,
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
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if matches!(
                job.status,
                JobStatus::Done | JobStatus::Cancelled | JobStatus::Error
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
#[ignore = "requires pinned IS-Net and native runtime"]
async fn actual_private_cutout_jobs_recover_and_refuse_changed_invariants() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        model: std::env::var("VIDEO_KADR_TEST_OBJECT_MODEL")
            .unwrap()
            .into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    for mode in [
        "recover",
        "running_recover",
        "changed",
        "revoked",
        "engine_changed",
        "managed_removed",
        "managed_running_removed",
    ] {
        eprintln!("cutout mode {mode}");
        let (state, _dir) = support::make_state(true, false).await;
        let mut host_config = config.clone();
        if mode.starts_with("managed_") {
            host_config.model = state.storage.join("missing-default.onnx");
        }
        let state = state.with_cutout_config(Some(host_config));
        let owner = state
            .db
            .register_auth_user("cutout-owner", "public synthetic password")
            .await
            .unwrap()
            .unwrap();
        let viewer = state
            .db
            .register_auth_user("cutout-viewer", "public synthetic password")
            .await
            .unwrap()
            .unwrap();
        let path = state.sources_dir().join("cutout.png");
        image::RgbaImage::from_fn(37, 23, |x, y| {
            let n = y * 37 + x;
            image::Rgba([
                (n % 151) as u8,
                ((n * 17) % 180) as u8,
                ((n * 31) % 199) as u8,
                (n % 256) as u8,
            ])
        })
        .save(&path)
        .unwrap();
        assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"cutout-source","filename":"cutout.png","url":"/files/sources/cutout.png","mediaType":"image"}))).await);
        let space = state
            .db
            .create_space(&owner.user.username, "Cutout")
            .await
            .unwrap();
        let project = state
            .db
            .create_space_composition_project(
                "Cutout",
                &json!({"schemaVersion":1,"sources":{"cutout-source":{"id":"cutout-source"}}}),
                &["cutout-source".into()],
                &owner.user.username,
                &space.id,
            )
            .await
            .unwrap();
        if mode == "revoked" {
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
        }
        let actor = if mode == "revoked" { &viewer } else { &owner };
        let outsider = if mode == "revoked" { &owner } else { &viewer };
        let app = support::router(state.clone());
        if mode.starts_with("managed_") {
            let response = app
                .clone()
                .oneshot(request(
                    "POST",
                    "/api/cutout/models/downloads",
                    &owner.token,
                    json!({"catalogId":"isnet-general-use"}),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::ACCEPTED);
            let download = value(response).await["jobId"].as_str().unwrap().to_owned();
            assert_eq!(terminal(&state, &download).await.status, JobStatus::Done);
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "POST",
                        "/api/cutout/models/select",
                        &owner.token,
                        json!({"modelId":"isnet-general-use"})
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "POST",
                        "/api/cutout/models/select",
                        &viewer.token,
                        json!({"modelId":"isnet-general-use"})
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }

        assert_eq!(
            value(
                app.clone()
                    .oneshot(request(
                        "GET",
                        "/api/cutout/status",
                        &actor.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
            )
            .await["available"],
            true
        );
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    "/api/cutout/jobs",
                    &actor.token,
                    json!({"sourceId":"cutout-source","model":"/untrusted"})
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        let mut held = vec![];
        for _ in 0..state.resource_limit(ResourceClass::Analysis) {
            held.push(
                state
                    .acquire_job_slot(ResourceClass::Analysis)
                    .await
                    .unwrap(),
            );
        }
        let data = json!({"sourceId":"cutout-source"});
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/cutout/jobs",
                &actor.token,
                data.clone(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let id = value(response).await["jobId"].as_str().unwrap().to_owned();
        let response = app
            .clone()
            .oneshot(request("POST", "/api/cutout/jobs", &actor.token, data))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let cancelled = value(response).await["jobId"].as_str().unwrap().to_owned();
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    &format!("/api/jobs/{id}/cancel"),
                    &outsider.token,
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
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
        let page = value(
            app.clone()
                .oneshot(request(
                    "GET",
                    "/api/cutout/jobs?sourceId=cutout-source&limit=1",
                    &actor.token,
                    Value::Null,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(page["jobs"].as_array().unwrap().len(), 1);
        let cursor = page["nextCursor"].as_str().unwrap();
        let next = value(
            app.clone()
                .oneshot(request(
                    "GET",
                    &format!("/api/cutout/jobs?limit=1&cursor={cursor}"),
                    &actor.token,
                    Value::Null,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_ne!(page["jobs"][0]["jobId"], next["jobs"][0]["jobId"]);
        assert!(value(
            app.clone()
                .oneshot(request(
                    "GET",
                    "/api/cutout/jobs",
                    &outsider.token,
                    Value::Null
                ))
                .await
                .unwrap()
        )
        .await["jobs"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "GET",
                    &format!("/api/jobs/{id}"),
                    &outsider.token,
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let path = state
            .library
            .resolve_media_path(&state.library.get("cutout-source").await.unwrap())
            .await
            .unwrap();
        if mode == "changed" {
            tokio::fs::write(&path, b"changed source").await.unwrap();
        }
        if mode == "revoked" {
            state
                .db
                .remove_space_member(&space.id, &owner.user.username, &viewer.user.username)
                .await
                .unwrap()
                .unwrap();
        }
        if matches!(mode, "running_recover" | "managed_running_removed") {
            held.clear();
            tokio::time::timeout(std::time::Duration::from_secs(60), async {
                loop {
                    let job = state.get_job(&id).await.unwrap().unwrap();
                    assert!(
                        !matches!(
                            job.status,
                            JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
                        ),
                        "must observe native progress {job:?}"
                    );
                    if job.status == JobStatus::Running && job.progress.unwrap_or(0.) > 0. {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
        }

        if mode.starts_with("managed_") {
            let removed = app
                .clone()
                .oneshot(request(
                    "POST",
                    "/api/cutout/models/remove",
                    &owner.token,
                    json!({"modelId":"isnet-general-use"}),
                ))
                .await
                .unwrap();
            assert_eq!(removed.status(), StatusCode::OK);
            assert_eq!(value(removed).await["fileReleased"], false);
            let models = value(
                app.clone()
                    .oneshot(request(
                        "GET",
                        "/api/cutout/models",
                        &owner.token,
                        Value::Null,
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert!(models["models"].as_array().unwrap().is_empty());
            assert_eq!(models["selectedId"], "configured");
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "POST",
                        "/api/cutout/models/select",
                        &owner.token,
                        json!({"modelId":"isnet-general-use"})
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::NOT_FOUND
            );
        }
        state.begin_shutdown();
        drop(held);
        assert!(
            state
                .wait_for_tasks(std::time::Duration::from_secs(10))
                .await
        );
        let mut next_config = config.clone();
        if mode.starts_with("managed_") {
            next_config.model = state.storage.join("missing-default.onnx");
        }
        if mode == "engine_changed" {
            next_config.runtime_library = path.clone();
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
        .with_cutout_config(Some(next_config));
        recovered.recover_jobs().await;
        video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
        let job = terminal(&recovered, &id).await;
        assert_eq!(
            terminal(&recovered, &cancelled).await.status,
            JobStatus::Cancelled
        );
        let app = support::router(recovered.clone());
        let image_url = format!("/api/cutout/jobs/{id}/image");
        if matches!(
            mode,
            "recover" | "running_recover" | "managed_removed" | "managed_running_removed"
        ) {
            assert_eq!(job.status, JobStatus::Done, "{job:?}");
            assert_eq!(job.result.as_ref().unwrap()["width"], 37);
            assert_eq!(job.result.as_ref().unwrap()["height"], 23);
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &image_url, &outsider.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            let response = app
                .clone()
                .oneshot(request("GET", &image_url, &actor.token, Value::Null))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["cache-control"], "no-store");
            let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            let result = image::load_from_memory(&bytes).unwrap().to_rgba8();
            let original = image::open(&path).unwrap().to_rgba8();
            assert_eq!(result.dimensions(), original.dimensions());
            let mut reduced = false;
            for (a, b) in original.pixels().zip(result.pixels()) {
                assert_eq!(a.0[..3], b.0[..3]);
                assert!(b[3] <= a[3]);
                reduced |= b[3] < a[3];
            }
            assert!(reduced);

            let mask_url = format!("/api/cutout/jobs/{id}/mask");
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &mask_url, &outsider.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            let response = app
                .clone()
                .oneshot(request("GET", &mask_url, &actor.token, Value::Null))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["cache-control"], "no-store");
            let mask_bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            let mask = image::load_from_memory(&mask_bytes).unwrap().to_luma8();
            assert_eq!(mask.dimensions(), (512, 512));
            use sha2::{Digest, Sha256};
            assert_eq!(
                format!("{:x}", Sha256::digest(mask.as_raw())),
                job.result.as_ref().unwrap()["maskHash"].as_str().unwrap()
            );
            assert_eq!(
                format!("{:x}", Sha256::digest(&mask_bytes)),
                job.result.as_ref().unwrap()["maskFileHash"]
                    .as_str()
                    .unwrap()
            );
            let mask_path = recovered
                .storage
                .join("cutout-jobs")
                .join(format!("{id}.mask.png"));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(&mask_path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            let cold = support::router(recovered.clone().with_cutout_config(None));
            assert_eq!(
                cold.oneshot(request("GET", &mask_url, &actor.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
            tokio::fs::write(&mask_path, b"corrupt mask").await.unwrap();
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &mask_url, &actor.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CONFLICT
            );
            tokio::fs::write(&mask_path, &mask_bytes).await.unwrap();
            let asset_url = format!("/api/cutout/jobs/{id}/asset");
            let target = json!({"projectId":project.id});
            assert_eq!(
                app.clone()
                    .oneshot(request("POST", &asset_url, &outsider.token, target.clone()))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            let response = app
                .clone()
                .oneshot(request("POST", &asset_url, &actor.token, target.clone()))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let saved = value(response).await;
            let again = value(
                app.clone()
                    .oneshot(request("POST", &asset_url, &actor.token, target.clone()))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(saved["id"], again["id"]);
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        saved["url"].as_str().unwrap(),
                        &outsider.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        saved["url"].as_str().unwrap(),
                        &actor.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
            let accepted = recovered
                .library
                .resolve_media_path(
                    &recovered
                        .library
                        .get(saved["id"].as_str().unwrap())
                        .await
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(tokio::fs::read(&accepted).await.unwrap(), bytes);
            tokio::fs::write(&accepted, b"corrupt accepted copy")
                .await
                .unwrap();
            assert_eq!(
                app.clone()
                    .oneshot(request("POST", &asset_url, &actor.token, target))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CONFLICT
            );
            tokio::fs::write(&accepted, &bytes).await.unwrap();
            tokio::fs::write(
                recovered
                    .storage
                    .join("cutout-jobs")
                    .join(format!("{id}.png")),
                b"corrupt",
            )
            .await
            .unwrap();
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &image_url, &actor.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CONFLICT
            );
        } else {
            assert_eq!(job.status, JobStatus::Error, "{mode}: {job:?}");
            assert!(!recovered
                .storage
                .join("cutout-jobs")
                .join(format!("{id}.mask.png"))
                .exists());
            assert!(!recovered
                .storage
                .join("cutout-jobs")
                .join(format!("{id}.png"))
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
