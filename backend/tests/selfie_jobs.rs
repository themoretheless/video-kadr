mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::{
    config::resource_classes::ResourceClass, db::SpaceRole, library::MediaEntry, model::JobStatus,
    services::local_selfie::Config, state::AppState,
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
#[tokio::test]
#[ignore = "requires native Selfie model/runtime"]
async fn real_selfie_job_recovery_source_access_engine_and_private_results() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        model: std::env::var("VIDEO_KADR_TEST_SELFIE_MODEL")
            .unwrap()
            .into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    for mode in ["queued", "running", "source", "access", "engine"] {
        eprintln!("Selfie mode {mode}");
        let (state, _dir) = support::make_state(true, false).await;
        let state = state.with_selfie_config(Some(config.clone()));
        let owner = state
            .db
            .register_auth_user("selfie-owner", "public synthetic password")
            .await
            .unwrap()
            .unwrap();
        let viewer = state
            .db
            .register_auth_user("selfie-viewer", "public synthetic password")
            .await
            .unwrap()
            .unwrap();
        let space = state
            .db
            .create_space(&owner.user.username, "Selfie source")
            .await
            .unwrap();
        state
            .db
            .set_space_member(
                &space.id,
                &owner.user.username,
                &viewer.user.username,
                SpaceRole::Editor,
            )
            .await
            .unwrap()
            .unwrap();
        let (w, h) = if mode == "running" {
            (2048, 1024)
        } else {
            (37, 23)
        };
        let source = state.sources_dir().join("selfie.png");
        image::RgbaImage::from_fn(w, h, |x, y| {
            image::Rgba([
                (x % 151) as u8,
                ((y * 17) % 180) as u8,
                ((x * 31) % 199) as u8,
                ((x + y) % 256) as u8,
            ])
        })
        .save(&source)
        .unwrap();
        state.library.add(MediaEntry::from_result("source",&json!({"id":"selfie-source","filename":"selfie.png","url":"/files/sources/selfie.png","mediaType":"image","width":w,"height":h}))).await;
        let project = state
            .db
            .create_space_composition_project(
                "Selfie",
                &json!({"schemaVersion":1,"sources":{"selfie-source":{"id":"selfie-source"}}}),
                &["selfie-source".into()],
                &owner.user.username,
                &space.id,
            )
            .await
            .unwrap();
        let app = support::router(state.clone());
        assert_eq!(
            app.clone()
                .oneshot(request("GET", "/api/selfie/status", "", Value::Null))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            value(
                app.clone()
                    .oneshot(request(
                        "GET",
                        "/api/selfie/status",
                        &viewer.token,
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
                    "/api/selfie/jobs",
                    &viewer.token,
                    json!({"sourceId":"selfie-source","model":"/foreign"})
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
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/selfie/jobs",
                &viewer.token,
                json!({"sourceId":"selfie-source"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let id = value(response).await["jobId"].as_str().unwrap().to_owned();
        for path in [
            format!("/api/jobs/{id}"),
            format!("/api/selfie/jobs/{id}/image"),
            format!("/api/selfie/jobs/{id}/mask"),
            format!("/api/selfie/jobs/{id}/original"),
        ] {
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &path, &owner.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/selfie/jobs",
                &viewer.token,
                json!({"sourceId":"selfie-source"}),
            ))
            .await
            .unwrap();
        let cancelled = value(response).await["jobId"].as_str().unwrap().to_owned();
        assert!(app
            .clone()
            .oneshot(request(
                "POST",
                &format!("/api/jobs/{cancelled}/cancel"),
                &viewer.token,
                Value::Null
            ))
            .await
            .unwrap()
            .status()
            .is_success());
        if mode == "running" {
            held.clear();
            tokio::time::timeout(std::time::Duration::from_secs(30), async {
                loop {
                    let job = state.get_job(&id).await.unwrap().unwrap();
                    assert!(
                        !matches!(job.status, JobStatus::Done | JobStatus::Error),
                        "missed running inference"
                    );
                    if job.status == JobStatus::Running && job.progress.unwrap_or(0.) > 0. {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                }
            })
            .await
            .unwrap();
        }
        if mode == "source" {
            let entry = state.library.get("selfie-source").await.unwrap();
            let actual = state.library.resolve_media_path(&entry).await.unwrap();
            std::fs::write(actual, b"changed").unwrap();
        }
        if mode == "access" {
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
        let mut configured = config.clone();
        if mode == "engine" {
            configured.model = source.clone();
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
        .with_selfie_config(Some(configured));
        recovered.recover_jobs().await;
        video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
        let job = terminal(&recovered, &id).await;
        if ["queued", "running"].contains(&mode) {
            assert_eq!(job.status, JobStatus::Done, "{:?}", job.error);
            let app = support::router(recovered.clone().with_selfie_config(None));
            for (kind, width, height) in [
                ("image", w, h),
                ("mask", 256, 256),
                (
                    "original",
                    w.min(1024),
                    if w > 1024 { h * 1024 / w } else { h },
                ),
            ] {
                let response = app
                    .clone()
                    .oneshot(request(
                        "GET",
                        &format!("/api/selfie/jobs/{id}/{kind}"),
                        &viewer.token,
                        Value::Null,
                    ))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(response.headers()["cache-control"], "no-store");
                let bytes = to_bytes(response.into_body(), 32 * 1024 * 1024)
                    .await
                    .unwrap();
                let image = image::load_from_memory(&bytes).unwrap();
                assert_eq!((image.width(), image.height()), (width, height));
            }
            let response = app
                .clone()
                .oneshot(request(
                    "POST",
                    &format!("/api/selfie/jobs/{id}/asset"),
                    &viewer.token,
                    json!({"projectId":project.id}),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let asset = value(response).await;
            assert_eq!(asset["cutout"]["mask"]["width"], 256);
            assert_eq!(asset["cutout"]["mask"]["height"], 256);
            assert_eq!(
                asset["cutout"]["snapshot"]["modelHash"],
                video_kadr_backend::domain::cutout::SELFIE_MODEL_HASH
            );
            let repeated = value(
                app.clone()
                    .oneshot(request(
                        "POST",
                        &format!("/api/selfie/jobs/{id}/asset"),
                        &viewer.token,
                        json!({"projectId":project.id}),
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(repeated, asset);
            let missing = recovered.clone().with_selfie_config(Some(Config {
                model: recovered.storage.join("removed-selfie-model"),
                ..config.clone()
            }));
            let editing = support::router(missing.clone());
            let correction = json!({"custom":true,"strokes":[{"tool":"brush","size":0.5,"points":[[0.5,0.5]]}],"feather":0.01});
            let response = editing
                .clone()
                .oneshot(request(
                    "POST",
                    &format!("/api/selfie/jobs/{id}/corrections"),
                    &viewer.token,
                    correction.clone(),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::ACCEPTED);
            let edited = value(response).await["jobId"].as_str().unwrap().to_owned();
            let result = terminal(&missing, &edited).await;
            assert_eq!(result.status, JobStatus::Done, "{:?}", result.error);
            let response = editing
                .clone()
                .oneshot(request(
                    "POST",
                    &format!("/api/cutout/jobs/{edited}/asset"),
                    &viewer.token,
                    json!({"projectId":project.id}),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let edited_asset = value(response).await;
            assert_eq!(edited_asset["cutout"]["mask"]["width"], 256);
            assert_eq!(
                edited_asset["cutout"]["snapshot"]["correction"]["strokes"][0]["tool"],
                "brush"
            );
            assert_eq!(
                edited_asset["cutout"]["snapshot"]["maskFileHash"],
                asset["cutout"]["snapshot"]["maskFileHash"]
            );
            if mode == "queued" {
                let fixtures: Value =
                    serde_json::from_str(include_str!("fixtures/concat-merge-preview.json"))
                        .unwrap();
                let mut document = fixtures["createdTimeline"].clone();
                document["compositions"]
                    .as_object_mut()
                    .unwrap()
                    .retain(|key, _| key == "root");
                document["timelineOrder"] = json!(["root"]);
                let transform = document["compositions"]["root"]["content"]["tracks"][0]["clips"]
                    [0]["transform"]
                    .clone();
                let content = &mut document["compositions"]["root"]["content"];
                content["sources"] = json!({});
                for entry in [
                    &edited_asset,
                    &edited_asset["cutout"]["source"],
                    &edited_asset["cutout"]["mask"],
                ] {
                    let key = entry["id"].as_str().unwrap();
                    content["sources"][key] = json!({"id":key,"kind":"image","durationTicks":0,"width":entry["width"],"height":entry["height"],"hasAudio":false});
                }
                content["tracks"] = json!([{"id":"images","name":"Images","kind":"image","clips":[{"id":"picture","sourceId":edited_asset["id"],"timelineStartTick":0,"durationTicks":1000000,"transform":transform,"opacity":{"mode":"constant","value":1.0},"blendMode":"normal"}]}]);
                content["metadata"] = json!({"mediaVersions":{"picture":{"originalSourceId":"selfie-source","copySourceId":edited_asset["id"],"cutout":edited_asset["cutout"]["snapshot"]}}});
                document["compositions"]["root"]["durationUs"] = json!(1000000);
                let response=editing.clone().oneshot(request("PUT",&format!("/api/composition-projects/{}",project.id),&viewer.token,json!({"schemaVersion":2,"mode":"composition","name":"Selfie portable mask","baseRevision":project.revision,"document":document}))).await.unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                let response=editing.clone().oneshot(request("POST",&format!("/api/composition-projects/{}/cutout/corrections",project.id),&viewer.token,json!({"compositionId":"root","clipId":"picture","correction":correction,"expectedAttachment":edited_asset["cutout"]["snapshot"]}))).await.unwrap();
                assert_eq!(response.status(), StatusCode::ACCEPTED);
                let project_edit = value(response).await["jobId"].as_str().unwrap().to_owned();
                let project_result = terminal(&missing, &project_edit).await;
                assert_eq!(
                    project_result.status,
                    JobStatus::Done,
                    "{:?}",
                    project_result.error
                );
                assert_eq!(
                    project_result.result.as_ref().unwrap()["outputHash"],
                    result.result.as_ref().unwrap()["outputHash"]
                );
            }
            let history = value(
                app.clone()
                    .oneshot(request(
                        "GET",
                        "/api/selfie/jobs?sourceId=selfie-source",
                        &viewer.token,
                        Value::Null,
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(history["jobs"].as_array().unwrap().len(), 2);
            std::fs::write(
                recovered
                    .storage
                    .join("selfie-jobs")
                    .join(format!("{id}.mask.png")),
                b"corrupt",
            )
            .unwrap();
            assert_eq!(
                app.oneshot(request(
                    "GET",
                    &format!("/api/selfie/jobs/{id}/mask"),
                    &viewer.token,
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
                StatusCode::CONFLICT
            );
        } else {
            assert_eq!(job.status, JobStatus::Error);
            assert!(!recovered
                .storage
                .join("selfie-jobs")
                .join(format!("{id}.png"))
                .exists());
            assert!(!recovered
                .storage
                .join("selfie-jobs")
                .join(format!("{id}.mask.png"))
                .exists());
        }
        assert_eq!(
            terminal(&recovered, &cancelled).await.status,
            JobStatus::Cancelled
        );
        recovered.begin_shutdown();
        assert!(
            recovered
                .wait_for_tasks(std::time::Duration::from_secs(10))
                .await
        );
    }
}
