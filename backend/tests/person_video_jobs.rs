mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::{
    config::resource_classes::ResourceClass, db::SpaceRole, library::MediaEntry, model::JobStatus,
    services::local_person::Config, state::AppState,
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
async fn actual_private_person_video_jobs_recovery_cancel_source_acl_and_engine() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        model: std::env::var("VIDEO_KADR_TEST_PERSON_MODEL")
            .unwrap()
            .into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    for mode in ["recover", "running", "changed", "revoked", "engine_changed"] {
        eprintln!("person video mode: {mode}");
        let (state, _dir) = support::make_state(true, false).await;
        let state = state.with_person_config(Some(config.clone()));
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
        let capability = app
            .clone()
            .oneshot(request(
                "GET",
                "/api/person/status",
                &owner.token,
                Value::Null,
            ))
            .await
            .unwrap();
        assert_eq!(capability.status(), StatusCode::OK);
        assert_eq!(value(capability).await["available"], true);
        let data = json!({"sourceId":"video-source"});
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    "/api/person/video-jobs",
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
            json!({"sourceId":"video-source","unused":2,"path":"/etc/passwd"}),
        ] {
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "POST",
                        "/api/person/video-jobs",
                        &owner.token,
                        body
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
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
                "/api/person/video-jobs",
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
                "/api/person/video-jobs",
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
                    "/api/person/video-jobs?sourceId=video-source&limit=1",
                    &actor.token,
                    Value::Null,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(first["jobs"].as_array().unwrap().len(), 1);
        let cursor = first["nextCursor"].as_str().unwrap();
        let second = value(
            app.clone()
                .oneshot(request(
                    "GET",
                    &format!(
                        "/api/person/video-jobs?sourceId=video-source&limit=1&cursor={cursor}"
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
                    "/api/person/video-jobs",
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
                    &format!("/api/person/video-jobs?cursor={cursor}"),
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
        let recovered = AppState::new(
            state.storage.clone(),
            2,
            (*state.tools).clone(),
            video_kadr_backend::library::Library::load(state.storage.clone()).await,
            video_kadr_backend::db::Db::open(&state.storage)
                .await
                .unwrap(),
        )
        .with_person_config(Some(restored_config));
        recovered.recover_jobs().await;
        video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
        let job = terminal(&recovered, &id).await;
        assert_eq!(
            terminal(&recovered, &cancelled).await.status,
            JobStatus::Cancelled
        );
        let app = support::router(recovered.clone());
        if matches!(mode, "recover" | "running") {
            assert_eq!(job.status, JobStatus::Done, "{mode}: {job:?}");
            let meta = job.result.unwrap();
            assert_eq!(meta["width"], 16);
            assert_eq!(meta["height"], 24);
            assert_eq!(meta["frames"], if mode == "running" { 6 } else { 2 });
            assert_eq!(meta["audioCopied"], true);
            assert_eq!(meta["codec"], "qtrle");
            let url = format!("/api/person/video-jobs/{id}/video");
            let mask_url = format!("/api/person/video-jobs/{id}/masks");
            let mask_path = recovered
                .storage
                .join("person-video-jobs")
                .join(format!("{id}.masks"));
            let mask_bytes = std::fs::read(&mask_path).unwrap();
            let automatic_url = format!("/api/person/video-jobs/{id}/automatic-mask-video");
            let automatic_path = recovered
                .storage
                .join("person-video-jobs")
                .join(format!("{id}.automatic.mkv"));
            let automatic_bytes = std::fs::read(&automatic_path).unwrap();
            assert_eq!(meta["automaticMaskVideo"]["maskHash"], meta["maskHash"]);
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &automatic_url, &outsider.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            let response = app
                .clone()
                .oneshot(request("GET", &automatic_url, &actor.token, Value::Null))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert_eq!(response.headers()["content-type"], "video/x-matroska");
            assert_eq!(
                to_bytes(response.into_body(), 8 * 1024 * 1024)
                    .await
                    .unwrap()
                    .as_ref(),
                automatic_bytes.as_slice()
            );
            let mut range = request("GET", &automatic_url, &actor.token, Value::Null);
            range
                .headers_mut()
                .insert("range", "bytes=0-31".parse().unwrap());
            assert_eq!(
                app.clone().oneshot(range).await.unwrap().status(),
                StatusCode::PARTIAL_CONTENT
            );
            std::fs::write(&automatic_path, b"corrupt mask video").unwrap();
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &automatic_url, &actor.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CONFLICT
            );
            std::fs::write(&automatic_path, &automatic_bytes).unwrap();

            for frame in [0, meta["frames"].as_u64().unwrap() - 1] {
                let frame_url = format!("{mask_url}/{frame}");
                assert_eq!(
                    app.clone()
                        .oneshot(request("GET", &frame_url, &outsider.token, Value::Null))
                        .await
                        .unwrap()
                        .status(),
                    StatusCode::FORBIDDEN
                );
                let response = app
                    .clone()
                    .oneshot(request("GET", &frame_url, &actor.token, Value::Null))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(response.headers()["content-type"], "image/png");
                assert_eq!(response.headers()["cache-control"], "no-store");
                let png = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
                let image = image::load_from_memory(&png).unwrap().to_luma8();
                let size = (meta["maskWidth"].as_u64().unwrap()
                    * meta["maskHeight"].as_u64().unwrap()) as usize;
                assert_eq!(
                    image.as_raw(),
                    &mask_bytes[frame as usize * size..(frame as usize + 1) * size]
                );
            }
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        &format!("{mask_url}/{}", meta["frames"]),
                        &actor.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );

            assert_eq!(
                mask_bytes.len() as u64,
                meta["frames"].as_u64().unwrap()
                    * meta["maskWidth"].as_u64().unwrap()
                    * meta["maskHeight"].as_u64().unwrap()
            );
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
            assert_eq!(
                to_bytes(response.into_body(), 8 * 1024 * 1024)
                    .await
                    .unwrap()
                    .as_ref(),
                mask_bytes.as_slice()
            );
            let mut range = request("GET", &mask_url, &actor.token, Value::Null);
            range
                .headers_mut()
                .insert("range", "bytes=0-31".parse().unwrap());
            let response = app.clone().oneshot(range).await.unwrap();
            assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
            assert_eq!(
                to_bytes(response.into_body(), 1024).await.unwrap().as_ref(),
                &mask_bytes[..32]
            );
            std::fs::write(&mask_path, vec![0; mask_bytes.len()]).unwrap();
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &mask_url, &actor.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CONFLICT
            );
            std::fs::write(&mask_path, &mask_bytes).unwrap();

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
            assert_eq!(response.headers()["content-type"], "video/quicktime");
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
                .join("person-video-jobs")
                .join(format!("{id}.mov"));
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
            let asset_url = format!("/api/person/video-jobs/{id}/asset");
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
            assert_eq!(accepted["width"], 16);
            assert_eq!(accepted["height"], 24);
            assert_eq!(accepted["fps"], 2.);
            assert_eq!(accepted["vcodec"], "qtrle");
            assert_eq!(accepted["acodec"], "aac");
            assert_eq!(accepted["sizeBytes"], bytes.len());
            let resources = &accepted["videoCutout"];
            assert_eq!(resources["snapshot"]["sourceId"], resources["source"]["id"]);
            assert_eq!(
                resources["snapshot"]["maskSourceId"],
                resources["mask"]["id"]
            );
            assert_eq!(resources["snapshot"]["maskHash"], meta["maskHash"]);
            assert_eq!(
                resources["snapshot"]["maskFileHash"],
                meta["automaticMaskVideo"]["fileHash"]
            );
            for key in ["source", "mask"] {
                let item = &resources[key];
                let entry = recovered
                    .library
                    .get(item["id"].as_str().unwrap())
                    .await
                    .unwrap();
                let saved_path = recovered.library.resolve_media_path(&entry).await.unwrap();
                let actual = std::fs::read(saved_path).unwrap();
                assert_eq!(
                    actual,
                    if key == "source" {
                        std::fs::read(&path).unwrap()
                    } else {
                        automatic_bytes.clone()
                    }
                );
                assert_eq!(
                    app.clone()
                        .oneshot(request(
                            "GET",
                            item["url"].as_str().unwrap(),
                            &outsider.token,
                            Value::Null
                        ))
                        .await
                        .unwrap()
                        .status(),
                    StatusCode::FORBIDDEN
                );
            }

            for key in ["source", "mask"] {
                let item = &resources[key];
                let entry = recovered
                    .library
                    .get(item["id"].as_str().unwrap())
                    .await
                    .unwrap();
                let resource_path = recovered.library.resolve_media_path(&entry).await.unwrap();
                let good = std::fs::read(&resource_path).unwrap();
                std::fs::write(&resource_path, b"changed project video resource").unwrap();
                assert_eq!(
                    app.clone()
                        .oneshot(request("POST", &asset_url, &actor.token, target.clone()))
                        .await
                        .unwrap()
                        .status(),
                    StatusCode::CONFLICT
                );
                assert_eq!(
                    std::fs::read(&resource_path).unwrap(),
                    b"changed project video resource"
                );
                std::fs::write(&resource_path, &good).unwrap();
            }
            let repeated = app
                .clone()
                .oneshot(request("POST", &asset_url, &actor.token, target.clone()))
                .await
                .unwrap();
            assert_eq!(repeated.status(), StatusCode::OK);
            let repeated = value(repeated).await;
            assert_eq!(repeated["id"], accepted["id"]);
            assert_eq!(
                repeated["videoCutout"]["source"]["id"],
                resources["source"]["id"]
            );
            assert_eq!(
                repeated["videoCutout"]["mask"]["id"],
                resources["mask"]["id"]
            );

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
                    .oneshot(request("GET", &automatic_url, &actor.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );

            assert_eq!(
                fresh
                    .clone()
                    .oneshot(request(
                        "GET",
                        &format!("{mask_url}/0"),
                        &actor.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );

            assert_eq!(
                fresh
                    .clone()
                    .oneshot(request("GET", &mask_url, &actor.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
            let capability = fresh
                .clone()
                .oneshot(request(
                    "GET",
                    "/api/person/status",
                    &actor.token,
                    Value::Null,
                ))
                .await
                .unwrap();
            assert_eq!(capability.status(), StatusCode::OK);
            assert_eq!(value(capability).await["available"], false);

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
            let restored_asset = value(restored_asset).await;
            assert_eq!(restored_asset["id"], accepted["id"]);
            assert_eq!(
                restored_asset["videoCutout"]["snapshot"],
                resources["snapshot"]
            );

            let current = value(
                fresh
                    .clone()
                    .oneshot(request(
                        "GET",
                        &format!("/api/composition-projects/{}", project.id),
                        &actor.token,
                        Value::Null,
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            let saved=fresh.clone().oneshot(request("PUT",&format!("/api/composition-projects/{}",project.id),&actor.token,json!({"schemaVersion":2,"mode":"composition","name":"Portable video masks","baseRevision":current["revision"],"document":project_with_video_resources(&accepted)}))).await.unwrap();
            let code = saved.status();
            let data = value(saved).await;
            assert_eq!(code, StatusCode::OK, "{data}");
            let history = value(
                fresh
                    .oneshot(request(
                        "GET",
                        "/api/person/video-jobs",
                        &actor.token,
                        Value::Null,
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(history["jobs"].as_array().unwrap().len(), 2);
            correction_flow(
                &reopened,
                &config.executable,
                &actor.token,
                &outsider.token,
                &id,
                &project.id,
            )
            .await;
            reopened.begin_shutdown();
            assert!(
                reopened
                    .wait_for_tasks(std::time::Duration::from_secs(5))
                    .await
            );
        } else {
            assert_eq!(job.status, JobStatus::Error, "{mode}: {job:?}");
            assert!(!recovered
                .storage
                .join("person-video-jobs")
                .join(format!("{id}.automatic.mkv"))
                .exists());
            assert!(!recovered
                .storage
                .join("person-video-jobs")
                .join(format!("{id}.masks"))
                .exists());
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
                .join("person-video-jobs")
                .join(format!("{id}.mov"))
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

async fn correction_flow(
    base_state: &AppState,
    executable: &std::path::Path,
    owner: &str,
    outsider: &str,
    base: &str,
    project: &str,
) {
    let missing = video_kadr_backend::services::local_person::Config {
        executable: executable.to_owned(),
        model: base_state.storage.join("missing-rvm.onnx"),
        runtime_library: base_state.storage.join("missing-ort.dylib"),
    };
    let state = base_state.clone().with_person_config(Some(missing.clone()));
    let app = support::router(state.clone());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    let capability = value(
        app.clone()
            .oneshot(request("GET", "/api/person/status", owner, Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(capability["available"], false);
    assert_eq!(capability["correctionAvailable"], true);
    let masks_path = state
        .storage
        .join("person-video-jobs")
        .join(format!("{base}.masks"));
    let original_masks = std::fs::read(&masks_path).unwrap();
    let original_path = state
        .storage
        .join("person-video-jobs")
        .join(format!("{base}.mov"));
    let decode = |path: &std::path::Path| {
        let output = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(path)
            .args([
                "-map",
                "0:v:0",
                "-an",
                "-fps_mode",
                "passthrough",
                "-pix_fmt",
                "rgba",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        output.stdout
    };
    let original_rgba = decode(&original_path);
    let root_meta = state.get_job(base).await.unwrap().unwrap().result.unwrap();
    let plane = (root_meta["maskWidth"].as_u64().unwrap()
        * root_meta["maskHeight"].as_u64().unwrap()) as usize;
    let points: Vec<[f64; 2]> = (0..9)
        .flat_map(|y| (0..9).map(move |x| [x as f64 / 8., y as f64 / 8.]))
        .collect();
    let edits = json!({"corrections":[{"frame":1,"correction":{"custom":true,"strokes":[{"tool":"eraser","size":0.5,"points":points}],"feather":0}}]});
    let url = format!("/api/person/video-jobs/{base}/corrections");
    assert_eq!(
        app.clone()
            .oneshot(request("POST", &url, outsider, edits.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                &url,
                owner,
                json!({"corrections":[],"path":"/etc/passwd"})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let admitted = app
        .clone()
        .oneshot(request("POST", &url, owner, edits.clone()))
        .await
        .unwrap();
    assert_eq!(admitted.status(), StatusCode::ACCEPTED);
    let id = value(admitted).await["jobId"].as_str().unwrap().to_owned();
    let job = terminal(&state, &id).await;
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    let child = state
        .storage
        .join("person-video-jobs")
        .join(format!("{id}.mov"));
    assert_eq!(
        job.result.as_ref().unwrap()["automaticMaskVideo"]["maskHash"],
        root_meta["maskHash"]
    );
    let decoded = std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(child.with_extension("automatic.mkv"))
        .args([
            "-map",
            "0:v:0",
            "-an",
            "-fps_mode",
            "passthrough",
            "-pix_fmt",
            "gray",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(decoded.status.success());
    assert_eq!(decoded.stdout, original_masks);

    let mut expected = original_rgba.clone();
    let frame = 16 * 24 * 4;
    for i in (frame + 3..2 * frame).step_by(4) {
        expected[i] = 0;
    }
    assert_eq!(decode(&child), expected);
    let got = std::fs::read(child.with_extension("masks")).unwrap();
    assert_eq!(&got[..plane], &original_masks[..plane]);
    assert!(got[plane..2 * plane].iter().all(|v| *v == 0));
    assert_eq!(&got[2 * plane..], &original_masks[2 * plane..]);
    let child_url = format!("/api/person/video-jobs/{id}/corrections");
    let recipe = app
        .clone()
        .oneshot(request("GET", &child_url, owner, Value::Null))
        .await
        .unwrap();
    assert_eq!(recipe.status(), StatusCode::OK);
    let recipe = value(recipe).await;
    assert_eq!(recipe["baseJobId"], base);
    assert_eq!(recipe["corrections"][0]["frame"], 1);
    let stored = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/person/video-jobs/{id}/asset"),
            owner,
            json!({"projectId":project}),
        ))
        .await
        .unwrap();
    assert_eq!(stored.status(), StatusCode::OK);
    // Editing an edited job reuses original automatic masks, not already-erased alpha.
    let admitted = app
        .clone()
        .oneshot(request(
            "POST",
            &child_url,
            owner,
            json!({"corrections":[]}),
        ))
        .await
        .unwrap();
    assert_eq!(admitted.status(), StatusCode::ACCEPTED);
    let reset = value(admitted).await["jobId"].as_str().unwrap().to_owned();
    assert_eq!(terminal(&state, &reset).await.status, JobStatus::Done);
    assert_eq!(
        decode(
            &state
                .storage
                .join("person-video-jobs")
                .join(format!("{reset}.mov"))
        ),
        original_rgba
    );
    // A valid but modified stored recipe must not silently reopen as another edit.
    let saved: String = sqlx::query_scalar(
        "SELECT corrections_json FROM person_video_job_corrections WHERE job_id=?",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE person_video_job_corrections SET corrections_json='[]' WHERE job_id=?")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(request("GET", &child_url, owner, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    sqlx::query("UPDATE person_video_job_corrections SET corrections_json=? WHERE job_id=?")
        .bind(saved)
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
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
        .oneshot(request("POST", &url, owner, edits.clone()))
        .await
        .unwrap();
    assert_eq!(admitted.status(), StatusCode::ACCEPTED);
    let failed = value(admitted).await["jobId"].as_str().unwrap().to_owned();
    std::fs::write(&masks_path, b"changed queued masks").unwrap();
    drop(held);
    let job = terminal(&state, &failed).await;
    assert_eq!(job.status, JobStatus::Error);
    assert_eq!(job.error_kind.as_deref(), Some("validation"));
    assert!(!state
        .storage
        .join("person-video-jobs")
        .join(format!("{failed}.mov"))
        .exists());
    std::fs::write(&masks_path, &original_masks).unwrap();
    let retry = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/jobs/{failed}/retry"),
            owner,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(retry.status(), StatusCode::CONFLICT);
    let readmitted = app
        .clone()
        .oneshot(request("POST", &url, owner, edits.clone()))
        .await
        .unwrap();
    assert_eq!(readmitted.status(), StatusCode::ACCEPTED);
    let fresh = value(readmitted).await["jobId"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(fresh, failed);
    assert_eq!(terminal(&state, &fresh).await.status, JobStatus::Done);
    let mut workload = video_kadr_backend::config::WorkloadConfig::default();
    workload.queue_limits.max_new_jobs = 1;
    let limited = support::router(state.clone().with_workload_config(workload));
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM person_video_job_corrections")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        limited
            .oneshot(request("POST", &url, owner, edits.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM person_video_job_corrections")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let admitted = app
        .clone()
        .oneshot(request("POST", &url, owner, edits.clone()))
        .await
        .unwrap();
    assert_eq!(admitted.status(), StatusCode::ACCEPTED);
    let cancelling = value(admitted).await["jobId"].as_str().unwrap().to_owned();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let job = state.get_job(&cancelling).await.unwrap().unwrap();
            assert!(
                !matches!(
                    job.status,
                    JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
                ),
                "must observe running correction: {job:?}"
            );
            if job.status == JobStatus::Running && job.progress.unwrap_or(0.) > 10. {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                &format!("/api/jobs/{cancelling}/cancel"),
                outsider,
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
                &format!("/api/jobs/{cancelling}/cancel"),
                owner,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        terminal(&state, &cancelling).await.status,
        JobStatus::Cancelled
    );
    for extension in ["mov", "masks", "automatic.mkv"] {
        assert!(!state
            .storage
            .join("person-video-jobs")
            .join(format!("{cancelling}.{extension}"))
            .exists());
    }

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
        .oneshot(request("POST", &url, owner, json!({"corrections":[]})))
        .await
        .unwrap();
    assert_eq!(admitted.status(), StatusCode::ACCEPTED);
    let pending = value(admitted).await["jobId"].as_str().unwrap().to_owned();
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
    )
    .with_person_config(Some(missing));
    recovered.recover_jobs().await;
    video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
    assert_eq!(terminal(&recovered, &pending).await.status, JobStatus::Done);
    assert_eq!(
        decode(
            &recovered
                .storage
                .join("person-video-jobs")
                .join(format!("{pending}.mov"))
        ),
        original_rgba
    );
    recovered.begin_shutdown();
    assert!(
        recovered
            .wait_for_tasks(std::time::Duration::from_secs(10))
            .await
    );
}

fn project_with_video_resources(accepted: &Value) -> Value {
    use video_kadr_backend::domain::{
        composition::*,
        metadata_burnin::{CompositionMetadata, MediaVersions},
    };
    let snapshot: video_kadr_backend::domain::video_cutout::Attachment =
        serde_json::from_value(accepted["videoCutout"]["snapshot"].clone()).unwrap();
    let mut content = Composition::new(CanvasSpec::default());
    for entry in [
        &accepted["videoCutout"]["source"],
        &accepted["videoCutout"]["mask"],
        accepted,
    ] {
        let id = SourceId::parse(entry["id"].as_str().unwrap()).unwrap();
        content.sources.insert(
            id.clone(),
            CompositionSource {
                id,
                kind: SourceKind::Video,
                width: entry["width"].as_u64().unwrap() as u32,
                height: entry["height"].as_u64().unwrap() as u32,
                duration_ticks: (entry["duration"].as_f64().unwrap() * 1e6).round() as u64,
                has_audio: entry["acodec"].is_string(),
                audio_channels: None,
            },
        );
    }
    let clip = CompositionClipId::parse("portable-video").unwrap();
    content.tracks.push(CompositionTrack::Video {
        id: TrackId::parse("video").unwrap(),
        name: "Video".into(),
        hidden: false,
        muted: false,
        locked: false,
        transitions: vec![],
        clips: vec![VideoClip {concat_cutout:None,audio_stream:None,concat_gain_keys:Vec::new(),dsp_effects:Vec::new(),independent_audio:false,audio_fade_in_ticks:0,audio_fade_out_ticks:0,
            id: clip.clone(),
            source_id: SourceId::parse(accepted["id"].as_str().unwrap()).unwrap(),
            placement: ClipPlacement {
                timeline_start_tick: 0,
                source_in_tick: 0,
                source_out_tick: 1_000_000,
                speed: 1.,
                timeline_duration_tick: None,
                source_timeline_offset_tick: None,
                source_head_timeline_tick: None, concat_retime: None,
                speed_ramp: None,
            },
            effect_timing: None,
            filmcraft_motion: None,
            playback_mode: PlaybackMode::Forward,
            stabilization: StabilizationSpec::Disabled,
            frame_interpolation: FrameInterpolation::Duplicate,
            transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(1.),
            blend_mode: BlendMode::Normal,
            effects: vec![],
            source_audio_enabled: true,
            audio_gain: AnimatableValue::constant(1.),
            audio_pan: AnimatableValue::constant(0.),
            enabled: true,
        }],
    });
    let mut metadata = CompositionMetadata::default();
    metadata.media_versions.insert(
        clip,
        MediaVersions {
            original_source_id: SourceId::parse(&snapshot.source_id).unwrap(),
            copy_source_id: SourceId::parse(accepted["id"].as_str().unwrap()).unwrap(),
            cutout: None,
            video_cutout: Some(snapshot),
        },
    );
    content.metadata = Some(metadata);
    content.validate().unwrap();
    json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":content,"instances":[]}}})
}
