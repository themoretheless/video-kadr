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
#[ignore = "requires native executable and pinned model for original mask only"]
async fn actual_correction_jobs_recover_without_model_and_keep_authored_settings() {
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
        "source_changed",
        "mask_changed",
        "revoked",
        "engine_changed",
    ] {
        eprintln!("correction mode {mode}");
        let (state, _dir) = support::make_state(true, false).await;
        let state = state.with_cutout_config(Some(config.clone()));
        let owner = state
            .db
            .register_auth_user("edit-owner", "public synthetic password")
            .await
            .unwrap()
            .unwrap();
        let viewer = state
            .db
            .register_auth_user("edit-viewer", "public synthetic password")
            .await
            .unwrap()
            .unwrap();
        let source = state.sources_dir().join("original.png");
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
        let original = image::open(&source).unwrap().to_rgba8();
        assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"original-source","filename":"original.png","url":"/files/sources/original.png"}))).await);
        let space = state
            .db
            .create_space(&owner.user.username, "Mask correction")
            .await
            .unwrap();
        let project = state
            .db
            .create_space_composition_project(
                "Correction",
                &json!({"schemaVersion":1,"sources":{"original-source":{"id":"original-source"}}}),
                &["original-source".into()],
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
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/cutout/jobs",
                &actor.token,
                json!({"sourceId":"original-source"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let base = value(response).await["jobId"].as_str().unwrap().to_owned();
        let baseline = terminal(&state, &base).await;
        assert_eq!(baseline.status, JobStatus::Done);
        let path = state
            .library
            .resolve_media_path(&state.library.get("original-source").await.unwrap())
            .await
            .unwrap();
        let mut configured = config.clone();
        configured.model = state.storage.join("model-deliberately-missing");
        configured.runtime_library = state.storage.join("runtime-deliberately-missing");
        let state = state.with_cutout_config(Some(configured.clone()));
        let app = support::router(state.clone());
        let endpoint = format!("/api/cutout/jobs/{base}/corrections");
        let mut strokes:Vec<Value>=(0..5).map(|i|json!({"tool":"brush","size":0.5,"points":[[0.,i as f64/4.],[1.,i as f64/4.]],"at":0.})).collect();
        if mode == "running_recover" {
            strokes.push(json!({"tool":"brush","size":0.5,"points":vec![[0.5,0.5];500],"at":0.}));
        }
        let settings = json!({"custom":true,"strokes":strokes,"feather":0.});
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    &endpoint,
                    &outsider.token,
                    settings.clone()
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
                    &endpoint,
                    &actor.token,
                    json!({"custom":true,"strokes":[],"feather":0.,"model":"/untrusted"})
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
        let response = app
            .clone()
            .oneshot(request("POST", &endpoint, &actor.token, settings.clone()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let id = value(response).await["jobId"].as_str().unwrap().to_owned();
        let response = app
            .clone()
            .oneshot(request("POST", &endpoint, &actor.token, settings.clone()))
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
        let settings_url = format!("/api/cutout/jobs/{id}/correction");
        assert_eq!(
            app.clone()
                .oneshot(request("GET", &settings_url, &outsider.token, Value::Null))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let recorded = value(
            app.clone()
                .oneshot(request("GET", &settings_url, &actor.token, Value::Null))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(recorded["baseJobId"], base);
        let recorded_settings: video_kadr_backend::services::local_mask_edit::Correction =
            serde_json::from_value(recorded["correction"].clone()).unwrap();
        let sent: video_kadr_backend::services::local_mask_edit::Correction =
            serde_json::from_value(settings).unwrap();
        assert_eq!(recorded_settings.hash().unwrap(), sent.hash().unwrap());
        if mode == "running_recover" {
            held.clear();
            tokio::time::timeout(std::time::Duration::from_secs(30), async {
                loop {
                    let job = state.get_job(&id).await.unwrap().unwrap();
                    assert!(
                        !matches!(
                            job.status,
                            JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
                        ),
                        "must observe running correction {job:?}"
                    );
                    if job.status == JobStatus::Running && job.progress.unwrap_or(0.) > 0. {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                }
            })
            .await
            .unwrap();
        }
        if mode == "source_changed" {
            tokio::fs::write(&path, b"changed source").await.unwrap();
        }
        if mode == "mask_changed" {
            tokio::fs::write(
                state
                    .storage
                    .join("cutout-jobs")
                    .join(format!("{base}.mask.png")),
                b"changed mask",
            )
            .await
            .unwrap();
        }
        if mode == "revoked" {
            state
                .db
                .remove_space_member(&space.id, &owner.user.username, &viewer.user.username)
                .await
                .unwrap()
                .unwrap();
        }
        state.begin_shutdown();
        drop(held);
        assert!(
            state
                .wait_for_tasks(std::time::Duration::from_secs(10))
                .await
        );
        if mode == "engine_changed" {
            configured.executable = path.clone();
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
        .with_cutout_config(Some(configured));
        recovered.recover_jobs().await;
        video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
        let job = terminal(&recovered, &id).await;
        assert_eq!(
            terminal(&recovered, &cancelled).await.status,
            JobStatus::Cancelled
        );
        let app = support::router(recovered.clone());
        if matches!(mode, "recover" | "running_recover") {
            assert_eq!(job.status, JobStatus::Done, "{job:?}");
            let image_url = format!("/api/cutout/jobs/{id}/image");
            let response = app
                .clone()
                .oneshot(request("GET", &image_url, &actor.token, Value::Null))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            assert_eq!(
                image::load_from_memory(&bytes).unwrap().to_rgba8(),
                original
            );
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &image_url, &outsider.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );

            let original_url = format!("/api/cutout/jobs/{id}/original");
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &original_url, &outsider.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            let response = app
                .clone()
                .oneshot(request("GET", &original_url, &actor.token, Value::Null))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["cache-control"], "no-store");
            let bytes = to_bytes(response.into_body(), 16 * 1024 * 1024)
                .await
                .unwrap();
            assert_eq!(
                image::load_from_memory(&bytes).unwrap().to_rgba8(),
                original
            );
            let recorded = value(
                app.clone()
                    .oneshot(request("GET", &settings_url, &actor.token, Value::Null))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(recorded["baseJobId"], base);
            assert_eq!(serde_json::from_value::<video_kadr_backend::services::local_mask_edit::Correction>(recorded["correction"].clone()).unwrap().hash().unwrap(),sent.hash().unwrap());
            let asset_url = format!("/api/cutout/jobs/{id}/asset");
            let response = app
                .clone()
                .oneshot(request(
                    "POST",
                    &asset_url,
                    &actor.token,
                    json!({"projectId":project.id}),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let asset = value(response).await;
            assert_eq!(asset["cutout"]["source"]["mediaType"], "image");
            let saved_mask = asset["cutout"]["mask"]["id"].as_str().unwrap().to_owned();
            assert_eq!(asset["cutout"]["snapshot"]["sourceId"], "original-source");
            assert_eq!(asset["cutout"]["snapshot"]["maskSourceId"], saved_mask);
            let snapshot: video_kadr_backend::domain::cutout::Attachment =
                serde_json::from_value(asset["cutout"]["snapshot"].clone()).unwrap();
            assert_eq!(snapshot.correction.hash().unwrap(), sent.hash().unwrap());
            let project_mask = recovered
                .library
                .resolve_media_path(&recovered.library.get(&saved_mask).await.unwrap())
                .await
                .unwrap();
            let mask_bytes = std::fs::read(&project_mask).unwrap();
            // A saved project mask is executable without any originating job ID.
            let portable_document = json!({"schemaVersion":1,"sources":{
                "original-source":{"id":"original-source","kind":"image","width":37,"height":23},
                saved_mask.clone():{"id":saved_mask,"kind":"image","width":512,"height":512},
                asset["id"].as_str().unwrap():{"id":asset["id"],"kind":"image","width":37,"height":23}
            },"tracks":[{"id":"images","kind":"image","clips":[{"id":"picture","sourceId":asset["id"]}]}],"metadata":{"mediaVersions":{"picture":{"originalSourceId":"original-source","copySourceId":asset["id"],"cutout":asset["cutout"]["snapshot"]}}}});
            let portable = recovered
                .db
                .create_space_composition_project(
                    "Portable mask",
                    &portable_document,
                    &portable_document["sources"]
                        .as_object()
                        .unwrap()
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>(),
                    &actor.user.username,
                    &space.id,
                )
                .await
                .unwrap();
            let portable_url = format!(
                "/api/composition-projects/{}/cutout/corrections",
                portable.id
            );
            let settings_url = format!("/api/composition-projects/{}/cutout/picture", portable.id);
            let response = app
                .clone()
                .oneshot(request("GET", &settings_url, &actor.token, Value::Null))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                value(response).await["snapshot"],
                asset["cutout"]["snapshot"]
            );
            let response = app
                .clone()
                .oneshot(request(
                    "GET",
                    &format!("{settings_url}/original"),
                    &actor.token,
                    Value::Null,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["cache-control"], "no-store");
            let original_bytes = to_bytes(response.into_body(), 16 * 1024 * 1024)
                .await
                .unwrap();
            assert_eq!(
                image::load_from_memory(&original_bytes).unwrap().to_rgba8(),
                original
            );
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &settings_url, &outsider.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        &format!("{settings_url}/original"),
                        &outsider.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            let mut stale_attachment = asset["cutout"]["snapshot"].clone();
            stale_attachment["correction"]["feather"] = json!(0.099);
            assert_eq!(app.clone().oneshot(request("POST",&portable_url,&actor.token,json!({"clipId":"picture","correction":{"custom":false,"strokes":[],"feather":0},"expectedAttachment":stale_attachment}))).await.unwrap().status(),StatusCode::CONFLICT);
            let portable_recipe = json!({"clipId":"picture","correction":{"custom":true,"strokes":(0..5).map(|i|json!({"tool":"brush","size":0.5,"points":[[0.,i as f64/4.],[1.,i as f64/4.]],"at":null})).collect::<Vec<_>>(),"feather":0}});
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "POST",
                        &portable_url,
                        &outsider.token,
                        portable_recipe.clone()
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            let response = app
                .clone()
                .oneshot(request(
                    "POST",
                    &portable_url,
                    &actor.token,
                    portable_recipe.clone(),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::ACCEPTED);
            let portable_id = value(response).await["jobId"].as_str().unwrap().to_owned();
            let portable_result = terminal(&recovered, &portable_id).await;
            assert_eq!(
                portable_result.status,
                JobStatus::Done,
                "{:?}",
                portable_result.error
            );
            assert_eq!(
                portable_result.result.as_ref().unwrap()["sourceHash"],
                asset["cutout"]["snapshot"]["sourceHash"]
            );
            let portable_png = std::fs::read(
                recovered
                    .storage
                    .join("cutout-jobs")
                    .join(format!("{portable_id}.png")),
            )
            .unwrap();
            assert_eq!(
                image::load_from_memory(&portable_png).unwrap().to_rgba8(),
                original
            );
            let collaborator = recovered
                .db
                .register_auth_user("mask-collaborator", "public synthetic password")
                .await
                .unwrap()
                .unwrap();
            recovered
                .db
                .set_space_member(
                    &space.id,
                    &actor.user.username,
                    &collaborator.user.username,
                    SpaceRole::Editor,
                )
                .await
                .unwrap()
                .unwrap();
            let response = app
                .clone()
                .oneshot(request(
                    "GET",
                    &settings_url,
                    &collaborator.token,
                    Value::Null,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                value(response).await["snapshot"],
                asset["cutout"]["snapshot"]
            );
            // The collaborator cannot read the original owner's private job.
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        &format!("/api/cutout/jobs/{id}/mask"),
                        &collaborator.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            let response = app
                .clone()
                .oneshot(request(
                    "POST",
                    &portable_url,
                    &collaborator.token,
                    portable_recipe.clone(),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::ACCEPTED);
            let collaborator_id = value(response).await["jobId"].as_str().unwrap().to_owned();
            let collaborator_result = terminal(&recovered, &collaborator_id).await;
            assert_eq!(
                collaborator_result.status,
                JobStatus::Done,
                "{:?}",
                collaborator_result.error
            );
            let accepted = app
                .clone()
                .oneshot(request(
                    "POST",
                    &format!("/api/cutout/jobs/{collaborator_id}/asset"),
                    &collaborator.token,
                    json!({"projectId":portable.id}),
                ))
                .await
                .unwrap();
            assert_eq!(accepted.status(), StatusCode::OK);
            let accepted = value(accepted).await;
            assert_eq!(
                accepted["cutout"]["snapshot"]["sourceId"],
                "original-source"
            );
            assert_eq!(
                accepted["cutout"]["snapshot"]["maskHash"],
                asset["cutout"]["snapshot"]["maskHash"]
            );
            for mutation in ["attachment", "membership"] {
                let mut held = Vec::new();
                for _ in 0..recovered.resource_limit(ResourceClass::Analysis) {
                    held.push(
                        recovered
                            .acquire_job_slot(ResourceClass::Analysis)
                            .await
                            .unwrap(),
                    );
                }
                let response = app
                    .clone()
                    .oneshot(request(
                        "POST",
                        &portable_url,
                        &collaborator.token,
                        portable_recipe.clone(),
                    ))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::ACCEPTED);
                let queued = value(response).await["jobId"].as_str().unwrap().to_owned();
                assert!(matches!(
                    recovered.get_job(&queued).await.unwrap().unwrap().status,
                    JobStatus::Pending | JobStatus::Running
                ));
                if mutation == "attachment" {
                    let current = recovered
                        .db
                        .get_composition_project(&portable.id)
                        .await
                        .unwrap()
                        .unwrap();
                    let mut changed = current.document.clone();
                    changed["metadata"]["mediaVersions"]["picture"]["cutout"]["correction"]
                        ["feather"] = json!(0.099);
                    recovered
                        .db
                        .update_composition_project_for(
                            &portable.id,
                            &actor.user.username,
                            "Changed mask",
                            &changed,
                            &current.source_ids,
                            current.revision,
                        )
                        .await
                        .unwrap()
                        .unwrap();
                } else {
                    recovered
                        .db
                        .remove_space_member(
                            &space.id,
                            &actor.user.username,
                            &collaborator.user.username,
                        )
                        .await
                        .unwrap();
                }
                drop(held);
                let result = terminal(&recovered, &queued).await;
                assert_eq!(
                    result.status,
                    JobStatus::Error,
                    "{mutation}: {:?}",
                    result.error
                );
                assert!(!recovered
                    .storage
                    .join("cutout-jobs")
                    .join(format!("{queued}.png"))
                    .exists());
                assert!(!recovered
                    .storage
                    .join("cutout-jobs")
                    .join(format!("{queued}.mask.png"))
                    .exists());
                if mutation == "attachment" {
                    let current = recovered
                        .db
                        .get_composition_project(&portable.id)
                        .await
                        .unwrap()
                        .unwrap();
                    recovered
                        .db
                        .update_composition_project_for(
                            &portable.id,
                            &actor.user.username,
                            "Portable mask",
                            &portable_document,
                            &current.source_ids,
                            current.revision,
                        )
                        .await
                        .unwrap()
                        .unwrap();
                }
            }
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        &settings_url,
                        &collaborator.token,
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
                        asset["cutout"]["mask"]["url"].as_str().unwrap(),
                        &outsider.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );

            let accepted = recovered
                .library
                .resolve_media_path(
                    &recovered
                        .library
                        .get(asset["id"].as_str().unwrap())
                        .await
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(std::fs::read(accepted).unwrap(), bytes);
            let mask_url = format!("/api/cutout/jobs/{id}/mask");
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &mask_url, &actor.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
            // Completed edits own an independent automatic mask, even if the parent is damaged.
            tokio::fs::write(
                recovered
                    .storage
                    .join("cutout-jobs")
                    .join(format!("{base}.mask.png")),
                b"parent damaged after completion",
            )
            .await
            .unwrap();
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &mask_url, &actor.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );

            let response = app
                .clone()
                .oneshot(request(
                    "POST",
                    &format!("/api/cutout/jobs/{id}/corrections"),
                    &actor.token,
                    json!({"custom":false,"strokes":[],"feather":0.}),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::ACCEPTED);
            let next = value(response).await["jobId"].as_str().unwrap().to_owned();
            let automatic = terminal(&recovered, &next).await;
            assert_eq!(automatic.status, JobStatus::Done, "{automatic:?}");
            assert_eq!(automatic.result.as_ref().unwrap()["outputHash"],baseline.result.as_ref().unwrap()["outputHash"],"re-edit from derived mask must reconstruct original automatic cutout without accumulating prior alpha");

            let response = app
                .clone()
                .oneshot(request(
                    "POST",
                    &format!("/api/cutout/jobs/{next}/asset"),
                    &actor.token,
                    json!({"projectId":project.id}),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let next_asset = value(response).await;
            assert_eq!(
                next_asset["cutout"]["mask"]["id"], saved_mask,
                "same project mask bytes must be reused across processed copies"
            );
            tokio::fs::write(
                recovered
                    .storage
                    .join("cutout-jobs")
                    .join(format!("{id}.mask.png")),
                b"old job mask damaged",
            )
            .await
            .unwrap();
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
            assert_eq!(value(repeated).await["cutout"]["mask"]["id"], saved_mask);
            assert_eq!(std::fs::read(&project_mask).unwrap(), mask_bytes);
            std::fs::write(&project_mask, b"damaged project mask").unwrap();
            let refused=app.clone().oneshot(request("POST",&portable_url,&actor.token,json!({"clipId":"picture","correction":{"custom":false,"strokes":[],"feather":0}}))).await.unwrap();
            assert_eq!(refused.status(), StatusCode::CONFLICT);

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
            // Restore only our intentionally damaged fixture, then recover a project-origin job.
            std::fs::write(&project_mask, &mask_bytes).unwrap();
            recovered
                .db
                .set_space_member(
                    &space.id,
                    &actor.user.username,
                    &collaborator.user.username,
                    SpaceRole::Editor,
                )
                .await
                .unwrap()
                .unwrap();
            let mut held = Vec::new();
            for _ in 0..recovered.resource_limit(ResourceClass::Analysis) {
                held.push(
                    recovered
                        .acquire_job_slot(ResourceClass::Analysis)
                        .await
                        .unwrap(),
                );
            }
            let response = app
                .clone()
                .oneshot(request(
                    "POST",
                    &portable_url,
                    &collaborator.token,
                    portable_recipe.clone(),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::ACCEPTED);
            let resume_id = value(response).await["jobId"].as_str().unwrap().to_owned();
            assert!(matches!(
                recovered.get_job(&resume_id).await.unwrap().unwrap().status,
                JobStatus::Pending | JobStatus::Running
            ));
            recovered.begin_shutdown();
            drop(held);
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
            )
            .with_cutout_config((*recovered.cutout).clone());
            reopened.recover_jobs().await;
            video_kadr_backend::handlers::resume_pending_jobs(&reopened).await;
            let resumed = terminal(&reopened, &resume_id).await;
            assert_eq!(resumed.status, JobStatus::Done, "{:?}", resumed.error);
            let bytes = std::fs::read(
                reopened
                    .storage
                    .join("cutout-jobs")
                    .join(format!("{resume_id}.png")),
            )
            .unwrap();
            assert_eq!(
                image::load_from_memory(&bytes).unwrap().to_rgba8(),
                original
            );
            let reopened_app = support::router(reopened.clone());
            assert_eq!(
                reopened_app
                    .clone()
                    .oneshot(request(
                        "GET",
                        &format!("/api/cutout/jobs/{resume_id}/image"),
                        &collaborator.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
            assert_eq!(
                reopened_app
                    .clone()
                    .oneshot(request(
                        "GET",
                        &format!("/api/cutout/jobs/{resume_id}/image"),
                        &actor.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            reopened.begin_shutdown();
            assert!(
                reopened
                    .wait_for_tasks(std::time::Duration::from_secs(10))
                    .await
            );
        } else {
            assert_eq!(job.status, JobStatus::Error, "{mode}: {job:?}");
            assert!(!recovered
                .storage
                .join("cutout-jobs")
                .join(format!("{id}.png"))
                .exists());
            assert!(!recovered
                .storage
                .join("cutout-jobs")
                .join(format!("{id}.mask.png"))
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
