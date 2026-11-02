mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::{
    config::resource_classes::ResourceClass, db::SpaceRole, library::MediaEntry, model::JobStatus,
    services::local_sam::Config, state::AppState,
};
fn request(method: &str, path: &str, token: &str, data: Value) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if !token.is_empty() {
        b = b.header("authorization", format!("Bearer {token}"));
    }
    b.body(Body::from(data.to_string())).unwrap()
}
async fn value(r: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(r.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}
async fn terminal(state: &AppState, id: &str) -> video_kadr_backend::model::Job {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if matches!(
                job.status,
                JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
#[test]
fn config_is_opt_in_complete_and_absolute() {
    assert!(Config::from_lookup(&mut |_| None).unwrap().is_none());
    assert!(Config::from_lookup(&mut |key| if key == "SAM_BIN" {
        Some("/bin".into())
    } else {
        None
    })
    .is_err());
    assert!(Config::from_lookup(&mut |_| Some("relative".into())).is_err());
    assert!(Config::from_lookup(&mut |_| Some("/absolute".into()))
        .unwrap()
        .is_some());
}
#[tokio::test]
#[ignore = "requires native vision executable, pinned SAM models and ORT"]
async fn actual_sam_jobs_recover_and_keep_owner_source_engine_invariants() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        encoder: std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap().into(),
        decoder: std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap().into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    for mode in [
        "recover",
        "running_recover",
        "managed_recover",
        "managed_running_recover",
        "source_changed",
        "revoked",
        "engine_changed",
    ] {
        eprintln!("SAM mode {mode}");
        let (state, _dir) = support::make_state(true, false).await;
        let state = state.with_sam_config(Some(config.clone()));
        let owner = state
            .db
            .register_auth_user("sam-owner", "public synthetic password")
            .await
            .unwrap()
            .unwrap();
        let viewer = state
            .db
            .register_auth_user("sam-viewer", "public synthetic password")
            .await
            .unwrap()
            .unwrap();
        let space = state
            .db
            .create_space(&owner.user.username, "SAM source")
            .await
            .unwrap();
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
        let source = state.sources_dir().join("source.png");
        image::RgbaImage::from_fn(37, 23, |x, y| {
            let n = y * 37 + x;
            image::Rgba([
                (n % 151) as u8,
                ((n * 17) % 180) as u8,
                ((n * 31) % 199) as u8,
                (n % 256) as u8,
            ])
        })
        .save(&source)
        .unwrap();
        state.library.add(MediaEntry::from_result("source",&json!({"id":"sam-source","filename":"source.png","url":"/files/sources/source.png","mediaType":"image","width":37,"height":23}))).await;
        state
            .db
            .create_space_composition_project(
                "Source binding",
                &json!({"schemaVersion":1,"sources":{"sam-source":{"id":"sam-source"}}}),
                &["sam-source".into()],
                &owner.user.username,
                &space.id,
            )
            .await
            .unwrap();
        let actor = if mode == "revoked" { &viewer } else { &owner };
        let outsider = if mode == "revoked" { &owner } else { &viewer };
        let app = support::router(state.clone());
        if mode.starts_with("managed_") {
            let root = state.storage.join("models/sam/slimsam-77-uniform");
            std::fs::create_dir_all(&root).unwrap();
            std::fs::copy(&config.encoder, root.join("vision_encoder.onnx")).unwrap();
            std::fs::copy(
                &config.decoder,
                root.join("prompt_encoder_mask_decoder.onnx"),
            )
            .unwrap();
            let response = app
                .clone()
                .oneshot(request(
                    "POST",
                    "/api/sam/models/downloads",
                    &owner.token,
                    json!({"catalogId":"slimsam-77-uniform"}),
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
                        "/api/sam/models/select",
                        &owner.token,
                        json!({"modelId":"slimsam-77-uniform"})
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
        }

        assert_eq!(
            app.clone()
                .oneshot(request("GET", "/api/sam/status", "", Value::Null))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let status = app
            .clone()
            .oneshot(request("GET", "/api/sam/status", &actor.token, Value::Null))
            .await
            .unwrap();
        assert_eq!(status.status(), StatusCode::OK);
        assert_eq!(value(status).await["available"], true);
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    "/api/sam/jobs",
                    &actor.token,
                    json!({"sourceId":"sam-source","points":[[0.5,0.5]],"encoder":"/untrusted"})
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
                    "/api/sam/jobs",
                    &actor.token,
                    json!({"sourceId":"sam-source","points":[]})
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        let mut held = Vec::new();
        for _ in 0..state.resource_limit(ResourceClass::Analysis) {
            held.push(
                state
                    .acquire_job_slot(ResourceClass::Analysis)
                    .await
                    .unwrap(),
            );
        }
        let recipe = json!({"sourceId":"sam-source","points":(0..40).map(|i|[i as f64/39.,0.3]).collect::<Vec<_>>()});
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/sam/jobs",
                &actor.token,
                recipe.clone(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let id = value(response).await["jobId"].as_str().unwrap().to_owned();
        for route in [
            format!("/api/jobs/{id}"),
            format!("/api/sam/jobs/{id}/points"),
            format!("/api/sam/jobs/{id}/mask"),
        ] {
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &route, &outsider.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &route, "", Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
        }
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
        let saved = value(
            app.clone()
                .oneshot(request(
                    "GET",
                    &format!("/api/sam/jobs/{id}/points"),
                    &actor.token,
                    Value::Null,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(saved, recipe);
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/sam/jobs",
                &actor.token,
                recipe.clone(),
            ))
            .await
            .unwrap();
        let cancelled = value(response).await["jobId"].as_str().unwrap().to_owned();
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
        if mode.ends_with("running_recover") {
            held.clear();
            tokio::time::timeout(std::time::Duration::from_secs(30), async {
                loop {
                    let job = state.get_job(&id).await.unwrap().unwrap();
                    if job.status == JobStatus::Running && job.progress.unwrap_or(0.) > 0. {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                }
            })
            .await
            .unwrap();
        }
        if mode == "source_changed" {
            assert_eq!(state.resource_available(ResourceClass::Analysis), 0);
            assert_eq!(
                state.get_job(&id).await.unwrap().unwrap().status,
                JobStatus::Pending
            );
            let actual = state
                .library
                .resolve_media_path(&state.library.get("sam-source").await.unwrap())
                .await
                .unwrap();
            std::fs::write(&actual, b"changed").unwrap();
            assert_eq!(std::fs::read(&actual).unwrap(), b"changed");
        }
        if mode == "revoked" {
            state
                .db
                .remove_space_member(&space.id, &owner.user.username, &viewer.user.username)
                .await
                .unwrap()
                .unwrap();
        }
        if mode.starts_with("managed_") {
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "POST",
                        "/api/sam/models/select",
                        &owner.token,
                        json!({"modelId":"configured"})
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
        }
        if mode.starts_with("managed_") {
            let response = app
                .clone()
                .oneshot(request(
                    "POST",
                    "/api/sam/models/remove",
                    &owner.token,
                    json!({"modelId":"slimsam-77-uniform"}),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let removed = value(response).await;
            assert_eq!(removed["removed"], true);
            assert_eq!(
                removed["fileReleased"], false,
                "accepted region must pin its selected pair"
            );
            assert!(state.storage.join("models/sam/slimsam-77-uniform").exists());
        }
        state.begin_shutdown();
        drop(held);
        assert!(
            state
                .wait_for_tasks(std::time::Duration::from_secs(10))
                .await
        );
        let mut changed = config.clone();
        if mode.starts_with("managed_") {
            changed.encoder = state.storage.join("absent-default-encoder");
            changed.decoder = state.storage.join("absent-default-decoder");
        }
        if mode == "engine_changed" {
            changed.decoder = source.clone();
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
        .with_sam_config(Some(changed));
        recovered.recover_jobs().await;
        video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
        let job = terminal(&recovered, &id).await;
        assert_eq!(
            terminal(&recovered, &cancelled).await.status,
            JobStatus::Cancelled
        );
        let target = recovered.storage.join("sam-jobs").join(format!("{id}.png"));
        if mode.ends_with("recover") {
            if mode.starts_with("managed_") {
                let app = support::router(recovered.clone());
                let response = app
                    .oneshot(request("GET", "/api/sam/models", &owner.token, Value::Null))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                assert!(value(response).await["models"]
                    .as_array()
                    .unwrap()
                    .is_empty());
                assert!(
                    !recovered
                        .storage
                        .join("models/sam/slimsam-77-uniform")
                        .exists(),
                    "terminal request must release removed model pin"
                );
            }
            assert_eq!(job.status, JobStatus::Done, "{:?}", job.error);
            let app = support::router(recovered.clone().with_sam_config(None));
            let mask = app
                .clone()
                .oneshot(request(
                    "GET",
                    &format!("/api/sam/jobs/{id}/mask"),
                    &actor.token,
                    Value::Null,
                ))
                .await
                .unwrap();
            assert_eq!(mask.status(), StatusCode::OK);
            assert_eq!(mask.headers()["cache-control"], "no-store");
            let bytes = to_bytes(mask.into_body(), 1024 * 1024).await.unwrap();
            assert_eq!(
                image::load_from_memory(&bytes)
                    .unwrap()
                    .to_luma8()
                    .dimensions(),
                (256, 256)
            );
            assert_eq!(std::fs::read(&target).unwrap(), bytes);
            let settings = value(
                app.clone()
                    .oneshot(request(
                        "GET",
                        &format!("/api/sam/jobs/{id}/points"),
                        &actor.token,
                        Value::Null,
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(settings, recipe);
            let history = value(
                app.clone()
                    .oneshot(request(
                        "GET",
                        "/api/sam/jobs?sourceId=sam-source",
                        &actor.token,
                        Value::Null,
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(history["jobs"].as_array().unwrap().len(), 2);
            let project = recovered
                .db
                .list_composition_projects_for(&actor.user.username)
                .await
                .unwrap()
                .into_iter()
                .next()
                .unwrap();
            let asset_url = format!("/api/sam/jobs/{id}/asset");
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "POST",
                        &asset_url,
                        &outsider.token,
                        json!({"projectId":project.id})
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
                        &asset_url,
                        "",
                        json!({"projectId":project.id})
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
            let accepted = app
                .clone()
                .oneshot(request(
                    "POST",
                    &asset_url,
                    &actor.token,
                    json!({"projectId":project.id}),
                ))
                .await
                .unwrap();
            assert_eq!(accepted.status(), StatusCode::OK);
            let accepted = value(accepted).await;
            assert_eq!(accepted["mediaType"], "image");
            assert_eq!(accepted["width"], 256);
            assert_eq!(accepted["height"], 256);
            assert_eq!(accepted["source"]["id"], "sam-source");
            assert_eq!(accepted["region"]["points"], recipe["points"]);
            let region: video_kadr_backend::domain::cutout::RegionAttachment =
                serde_json::from_value(accepted["region"].clone()).unwrap();
            region.validate().unwrap();
            assert_eq!(region.mask_source_id, accepted["id"]);
            assert!(!serde_json::to_string(&region).unwrap().contains(&id));
            let mut forged = accepted["region"].clone();
            forged["points"][0][0] = json!(0.99);
            assert!(
                serde_json::from_value::<video_kadr_backend::domain::cutout::RegionAttachment>(
                    forged
                )
                .is_err()
            );
            let project_mask = recovered
                .library
                .resolve_media_path(
                    &recovered
                        .library
                        .get(accepted["id"].as_str().unwrap())
                        .await
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(std::fs::read(&project_mask).unwrap(), bytes);
            // Shared project media follows Space ACL, unlike private job outputs.
            let shared = app
                .clone()
                .oneshot(request(
                    "GET",
                    accepted["url"].as_str().unwrap(),
                    &viewer.token,
                    Value::Null,
                ))
                .await
                .unwrap();
            assert_eq!(shared.status(), StatusCode::OK);
            assert_eq!(
                to_bytes(shared.into_body(), 1024 * 1024).await.unwrap(),
                bytes
            );
            let stranger = recovered
                .db
                .register_auth_user("sam-stranger", "public synthetic password")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        accepted["url"].as_str().unwrap(),
                        &stranger.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            let repeated = value(
                app.clone()
                    .oneshot(request(
                        "POST",
                        &asset_url,
                        &actor.token,
                        json!({"projectId":project.id}),
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(repeated["id"], accepted["id"]);
            assert_eq!(repeated["region"], accepted["region"]);
            std::fs::write(&target, b"corrupt").unwrap();
            let repeated = app
                .clone()
                .oneshot(request(
                    "POST",
                    &asset_url,
                    &actor.token,
                    json!({"projectId":project.id}),
                ))
                .await
                .unwrap();
            assert_eq!(repeated.status(), StatusCode::OK);
            assert_eq!(value(repeated).await["id"], accepted["id"]);
            assert_eq!(std::fs::read(&project_mask).unwrap(), bytes);
            std::fs::write(&project_mask, b"damaged accepted region").unwrap();
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "POST",
                        &asset_url,
                        &actor.token,
                        json!({"projectId":project.id})
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CONFLICT
            );

            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        &format!("/api/sam/jobs/{id}/mask"),
                        &actor.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CONFLICT
            );
        } else {
            assert_eq!(job.status, JobStatus::Error, "{mode}: {:?}", job.error);
            assert!(!target.exists());
        }
        recovered.begin_shutdown();
        assert!(
            recovered
                .wait_for_tasks(std::time::Duration::from_secs(10))
                .await
        );
    }
}

#[tokio::test]
#[ignore = "downloads pinned public SAM pair and requires native runtime"]
async fn managed_pair_install_selection_access_and_collection() {
    let (state, _dir) = support::make_state(true, false).await;
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        encoder: state.storage.join("not-installed-encoder"),
        decoder: state.storage.join("not-installed-decoder"),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let state = state.with_sam_config(Some(config));
    let owner = state
        .db
        .register_auth_user("sam-model-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("sam-model-other", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    assert_eq!(
        app.clone()
            .oneshot(request("GET", "/api/sam/models", "", Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let library = value(
        app.clone()
            .oneshot(request("GET", "/api/sam/models", &owner.token, Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(library["catalog"][0]["id"], "slimsam-77-uniform");
    assert_eq!(library["catalog"][0]["bytes"], 39_833_906);
    assert_eq!(
        value(
            app.clone()
                .oneshot(request("GET", "/api/sam/status", &owner.token, Value::Null))
                .await
                .unwrap()
        )
        .await["available"],
        false
    );
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/sam/models/downloads",
            &owner.token,
            json!({"catalogId":"slimsam-77-uniform"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let id = value(response).await["jobId"].as_str().unwrap().to_owned();
    let done = terminal(&state, &id).await;
    assert_eq!(done.status, JobStatus::Done, "{:?}", done.error);
    let root = state.storage.join("models/sam/slimsam-77-uniform");
    video_kadr_backend::services::sam_model::verify_directory(&root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for file in video_kadr_backend::services::sam_model::files() {
            assert_eq!(
                std::fs::metadata(root.join(file.path))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    for path in [format!("/api/jobs/{id}"), format!("/api/jobs/{id}/cancel")] {
        let method = if path.ends_with("cancel") {
            "POST"
        } else {
            "GET"
        };
        assert_eq!(
            app.clone()
                .oneshot(request(method, &path, &outsider.token, Value::Null))
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
                "/api/sam/models/select",
                &outsider.token,
                json!({"modelId":"slimsam-77-uniform"})
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
                "/api/sam/models/select",
                &owner.token,
                json!({"modelId":"slimsam-77-uniform"})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        value(
            app.clone()
                .oneshot(request("GET", "/api/sam/status", &owner.token, Value::Null))
                .await
                .unwrap()
        )
        .await["available"],
        true
    );
    assert_eq!(
        value(
            app.clone()
                .oneshot(request(
                    "GET",
                    "/api/sam/status",
                    &outsider.token,
                    Value::Null
                ))
                .await
                .unwrap()
        )
        .await["available"],
        false
    );
    let history = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/sam/models/downloads",
                &owner.token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(history["jobs"][0]["jobId"], id);
    let source = state.sources_dir().join("managed-source.png");
    image::RgbaImage::from_fn(37, 23, |x, y| {
        image::Rgba([(x * 7) as u8, (y * 9) as u8, 123, 255])
    })
    .save(&source)
    .unwrap();
    state.library.add(MediaEntry::from_result("source",&json!({"id":"managed-source","filename":"managed-source.png","url":"/files/sources/managed-source.png","mediaType":"image","width":37,"height":23}))).await;
    let space = state
        .db
        .create_space(&owner.user.username, "Managed SAM source")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Managed SAM",
            &json!({"schemaVersion":1,"sources":{"managed-source":{"id":"managed-source"}}}),
            &["managed-source".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/sam/jobs",
            &owner.token,
            json!({"sourceId":"managed-source","points":[[0.5,0.5]]}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let region = value(response).await["jobId"].as_str().unwrap().to_owned();
    let done = terminal(&state, &region).await;
    assert_eq!(done.status, JobStatus::Done, "{:?}", done.error);
    assert_eq!(
        app.clone()
            .oneshot(request(
                "GET",
                &format!("/api/sam/jobs/{region}/mask"),
                &owner.token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let removed = value(
        app.clone()
            .oneshot(request(
                "POST",
                "/api/sam/models/remove",
                &owner.token,
                json!({"modelId":"slimsam-77-uniform"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(removed["removed"], true);
    assert_eq!(removed["fileReleased"], true);
    assert!(!root.exists());
    assert_eq!(
        value(
            app.clone()
                .oneshot(request("GET", "/api/sam/models", &owner.token, Value::Null))
                .await
                .unwrap()
        )
        .await["selectedId"],
        "configured"
    );
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(10))
            .await
    );
}

#[tokio::test]
#[ignore = "downloads pinned public SAM pair twice"]
async fn managed_download_queued_and_running_restart() {
    for running in [false, true] {
        let (state, _dir) = support::make_state(true, false).await;
        let owner = state
            .db
            .register_auth_user("sam-download-recover", "public synthetic password")
            .await
            .unwrap()
            .unwrap();
        let app = support::router(state.clone());
        let mut held = Vec::new();
        for _ in 0..state.resource_limit(ResourceClass::Ingest) {
            held.push(state.acquire_job_slot(ResourceClass::Ingest).await.unwrap());
        }
        let response = app
            .oneshot(request(
                "POST",
                "/api/sam/models/downloads",
                &owner.token,
                json!({"catalogId":"slimsam-77-uniform"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let id = value(response).await["jobId"].as_str().unwrap().to_owned();
        assert_eq!(
            state.get_job(&id).await.unwrap().unwrap().status,
            JobStatus::Pending
        );
        if running {
            held.clear();
            tokio::time::timeout(std::time::Duration::from_secs(30), async {
                loop {
                    let job = state.get_job(&id).await.unwrap().unwrap();
                    assert!(
                        !matches!(job.status, JobStatus::Done | JobStatus::Error),
                        "transfer finished before in-progress interruption: {:?}",
                        job.status
                    );
                    if job.status == JobStatus::Running && job.progress.unwrap_or(0.) > 0. {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                }
            })
            .await
            .unwrap();
            assert!(state.storage.join("sam-model-downloads").join(&id).exists());
        }
        state.begin_shutdown();
        drop(held);
        assert!(
            state
                .wait_for_tasks(std::time::Duration::from_secs(10))
                .await
        );
        let recovered = AppState::new(
            state.storage.clone(),
            2,
            (*state.tools).clone(),
            video_kadr_backend::library::Library::load(state.storage.clone()).await,
            video_kadr_backend::db::Db::open(&state.storage)
                .await
                .unwrap(),
        );
        recovered.recover_jobs().await;
        video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
        let job = terminal(&recovered, &id).await;
        assert_eq!(
            job.status,
            JobStatus::Done,
            "running={running}: {:?}",
            job.error
        );
        video_kadr_backend::services::sam_model::verify_directory(
            &recovered.storage.join("models/sam/slimsam-77-uniform"),
        )
        .unwrap();
        let app = support::router(recovered.clone());
        let library = value(
            app.clone()
                .oneshot(request("GET", "/api/sam/models", &owner.token, Value::Null))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(library["models"].as_array().unwrap().len(), 1);
        let history = value(
            app.oneshot(request(
                "GET",
                "/api/sam/models/downloads",
                &owner.token,
                Value::Null,
            ))
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(history["jobs"][0]["jobId"], id);
        assert_eq!(history["jobs"][0]["status"], "done");
        recovered.begin_shutdown();
        assert!(
            recovered
                .wait_for_tasks(std::time::Duration::from_secs(10))
                .await
        );
    }
}

#[tokio::test]
#[ignore = "requires retained pinned SAM pair"]
async fn shared_model_removal_does_not_resurrect_from_stale_download() {
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("sam-shared-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let other = state
        .db
        .register_auth_user("sam-shared-other", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let root = state.storage.join("models/sam/slimsam-77-uniform");
    std::fs::create_dir_all(&root).unwrap();
    for (key, name) in [
        ("VIDEO_KADR_TEST_SAM_ENCODER", "vision_encoder.onnx"),
        (
            "VIDEO_KADR_TEST_SAM_DECODER",
            "prompt_encoder_mask_decoder.onnx",
        ),
    ] {
        std::fs::copy(std::env::var(key).unwrap(), root.join(name)).unwrap();
    }
    let app = support::router(state.clone());
    for token in [&owner.token, &other.token] {
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/sam/models/downloads",
                token,
                json!({"catalogId":"slimsam-77-uniform"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let id = value(response).await["jobId"].as_str().unwrap().to_owned();
        assert_eq!(terminal(&state, &id).await.status, JobStatus::Done);
    }
    let mut held = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Ingest) {
        held.push(state.acquire_job_slot(ResourceClass::Ingest).await.unwrap());
    }
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/sam/models/downloads",
            &owner.token,
            json!({"catalogId":"slimsam-77-uniform"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let stale = value(response).await["jobId"].as_str().unwrap().to_owned();
    assert_eq!(
        state.get_job(&stale).await.unwrap().unwrap().status,
        JobStatus::Pending
    );
    let removed = value(
        app.clone()
            .oneshot(request(
                "POST",
                "/api/sam/models/remove",
                &owner.token,
                json!({"modelId":"slimsam-77-uniform"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(removed["removed"], true);
    assert_eq!(removed["fileReleased"], false);
    assert!(root.exists());
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                "/api/sam/models/select",
                &owner.token,
                json!({"modelId":"slimsam-77-uniform"})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    drop(held);
    let failed = terminal(&state, &stale).await;
    assert_eq!(failed.status, JobStatus::Error);
    assert!(failed.error.unwrap().contains("удалена"));
    let own = value(
        app.clone()
            .oneshot(request("GET", "/api/sam/models", &owner.token, Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert!(own["models"].as_array().unwrap().is_empty());
    let theirs = value(
        app.clone()
            .oneshot(request("GET", "/api/sam/models", &other.token, Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(theirs["models"].as_array().unwrap().len(), 1);
    video_kadr_backend::services::sam_model::verify_directory(&root).unwrap();
    assert_eq!(
        app.clone()
            .oneshot(request(
                "GET",
                &format!("/api/sam/models/downloads?cursor={stale}"),
                &other.token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/sam/models/downloads",
            &owner.token,
            json!({"catalogId":"slimsam-77-uniform"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let fresh = value(response).await["jobId"].as_str().unwrap().to_owned();
    assert_ne!(fresh, stale);
    assert_eq!(terminal(&state, &fresh).await.status, JobStatus::Done);
    for (token, released) in [(&other.token, false), (&owner.token, true)] {
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/sam/models/remove",
                token,
                json!({"modelId":"slimsam-77-uniform"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let removed = value(response).await;
        assert_eq!(removed["fileReleased"], released);
    }
    assert!(!root.exists());
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(10))
            .await
    );
}
