mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use video_kadr_backend::{
    config::resource_classes::ResourceClass, library::MediaEntry, model::JobStatus,
    services::local_sam::Config, state::AppState,
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
#[ignore = "requires native SAM, pinned models/ORT, original tracking oracles and FFmpeg/FFprobe on PATH"]
async fn actual_video_jobs_private_transport_recovery_and_invariants() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        encoder: std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap().into(),
        decoder: std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap().into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let oracle =
        std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_SAM_TRACK_ORACLE_DIR").unwrap())
            .join("37x23");
    let expected = std::fs::read(oracle.join("reference.masks")).unwrap();
    for mode in [
        "whole",
        "recover",
        "running_recover",
        "source_changed",
        "running_source_changed",
        "revoked",
        "running_revoked",
        "engine_changed",
        "cancel",
        "running_cancel",
    ] {
        eprintln!("SAM video mode {mode}");
        let (state, _root) = support::make_state(true, false).await;
        let state = state.with_sam_config(Some(config.clone()));
        let owner = state
            .db
            .register_auth_user("sam-video-owner", "public synthetic password")
            .await
            .unwrap()
            .unwrap();
        let viewer = state
            .db
            .register_auth_user("sam-video-viewer", "public synthetic password")
            .await
            .unwrap()
            .unwrap();
        let space = state
            .db
            .create_space(&owner.user.username, "Video tracking")
            .await
            .unwrap();
        state
            .db
            .set_space_member(
                &space.id,
                &owner.user.username,
                &viewer.user.username,
                video_kadr_backend::db::SpaceRole::Viewer,
            )
            .await
            .unwrap()
            .unwrap();
        let movie = state.sources_dir().join("source.mov");
        let result = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                "37x23",
                "-framerate",
                "4",
                "-i",
                oracle.join("source.rgba").to_str().unwrap(),
                "-c:v",
                "qtrle",
                "-pix_fmt",
                "argb",
                movie.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        state.library.add(MediaEntry::from_result("source",&json!({"id":"sam-video-source","filename":"source.mov","url":"/files/sources/source.mov","mediaType":"video","width":37,"height":23}))).await;
        state.db.create_space_composition_project("Binding",&json!({"schemaVersion":1,"sources":{"sam-video-source":{"id":"sam-video-source"}}}),&["sam-video-source".into()],&owner.user.username,&space.id).await.unwrap();
        let actor = if mode.ends_with("revoked") {
            &viewer
        } else {
            &owner
        };
        let outsider = if mode.ends_with("revoked") {
            &owner
        } else {
            &viewer
        };
        let app = support::router(state.clone());
        let recipe = json!({"sourceId":"sam-video-source","points":[[0.5,0.5]],"seedFrame":1});
        for invalid in [
            json!({"sourceId":"sam-video-source","points":[[0.5,0.5]],"seedFrame":360000}),
            json!({"sourceId":"sam-video-source","points":[],"seedFrame":1}),
            json!({"sourceId":"sam-video-source","points":[[0.5,0.5]],"seedFrame":1,"output":"/foreign"}),
        ] {
            assert_eq!(
                app.clone()
                    .oneshot(request("POST", "/api/sam/jobs", &actor.token, invalid))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
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
        let actual_source = state
            .library
            .resolve_media_path(&state.library.get("sam-video-source").await.unwrap())
            .await
            .unwrap();
        let target = state.storage.join("sam-jobs").join(format!("{id}.mkv"));
        for route in [
            format!("/api/jobs/{id}"),
            format!("/api/sam/jobs/{id}/points"),
            format!("/api/sam/jobs/{id}/video"),
        ] {
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &route, &outsider.token, Value::Null))
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
                        &format!("/api/sam/jobs/{id}/points"),
                        &actor.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
            )
            .await,
            recipe
        );
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "GET",
                    &format!("/api/sam/jobs/{id}/video"),
                    &actor.token,
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::CONFLICT
        );
        if mode.starts_with("running_") {
            held.clear();
            tokio::time::timeout(std::time::Duration::from_secs(30), async {
                loop {
                    let job = state.get_job(&id).await.unwrap().unwrap();
                    if job.status == JobStatus::Running && job.progress.unwrap_or(0.) >= 20. {
                        break;
                    }
                    assert!(
                        !matches!(job.status, JobStatus::Error | JobStatus::Done),
                        "{job:?}"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                }
            })
            .await
            .unwrap();
        }
        if mode.ends_with("source_changed") {
            std::fs::write(&actual_source, b"changed").unwrap();
        }
        if mode.ends_with("revoked") {
            state
                .db
                .remove_space_member(&space.id, &owner.user.username, &viewer.user.username)
                .await
                .unwrap()
                .unwrap();
        }
        if mode.ends_with("cancel") {
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "POST",
                        &format!("/api/jobs/{id}/cancel"),
                        &actor.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
        }
        let active = if ["recover", "running_recover", "engine_changed"].contains(&mode) {
            state.begin_shutdown();
            drop(held);
            assert!(
                state
                    .wait_for_tasks(std::time::Duration::from_secs(15))
                    .await
            );
            let mut selected = config.clone();
            if mode == "engine_changed" {
                selected.decoder = movie.clone();
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
            .with_sam_config(Some(selected));
            recovered.recover_jobs().await;
            video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
            recovered
        } else {
            drop(held);
            state.clone()
        };
        let job = terminal(&active, &id).await;
        if ["whole", "recover", "running_recover"].contains(&mode) {
            assert_eq!(job.status, JobStatus::Done, "{job:?}");
            let meta = job.result.unwrap();
            assert_eq!(
                meta["tracking"]["outputHash"],
                format!("{:x}", Sha256::digest(&expected))
            );
            assert_eq!(
                std::fs::read(target.with_extension("gray")).unwrap(),
                expected
            );
            let app = support::router(active.clone());
            assert_eq!(
                value(
                    app.clone()
                        .oneshot(request(
                            "GET",
                            &format!("/api/sam/jobs/{id}/points"),
                            &actor.token,
                            Value::Null
                        ))
                        .await
                        .unwrap()
                )
                .await,
                recipe,
                "recipe survives terminal payload erasure"
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
            let response = app
                .clone()
                .oneshot(request(
                    "GET",
                    &format!("/api/sam/jobs/{id}/video"),
                    &actor.token,
                    Value::Null,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert_eq!(response.headers()["content-type"], "video/x-matroska");
            let encoded = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            assert_eq!(encoded.as_ref(), std::fs::read(&target).unwrap());
            let mut range = request(
                "GET",
                &format!("/api/sam/jobs/{id}/video"),
                &actor.token,
                Value::Null,
            );
            range
                .headers_mut()
                .insert("range", "bytes=0-31".parse().unwrap());
            let response = app.clone().oneshot(range).await.unwrap();
            assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
            assert_eq!(
                &to_bytes(response.into_body(), 1024).await.unwrap()[..],
                &encoded[..32]
            );
            let listing = value(
                app.clone()
                    .oneshot(request(
                        "GET",
                        "/api/sam/jobs?video=true&sourceId=sam-video-source",
                        &actor.token,
                        Value::Null,
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(listing["jobs"][0]["jobId"], id);
            let page = value(
                app.clone()
                    .oneshot(request(
                        "GET",
                        &format!("/api/sam/jobs?video=true&limit=1&cursor={id}"),
                        &actor.token,
                        Value::Null,
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(page["jobs"], json!([]));
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        &format!("/api/sam/jobs?cursor={id}"),
                        &actor.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST,
                "video cursor cannot cross into still history"
            );

            let stills = value(
                app.clone()
                    .oneshot(request(
                        "GET",
                        "/api/sam/jobs?sourceId=sam-video-source",
                        &actor.token,
                        Value::Null,
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(stills["jobs"], json!([]));

            if mode == "whole" {
                let project = active
                    .db
                    .list_composition_projects_for(&actor.user.username)
                    .await
                    .unwrap()
                    .into_iter()
                    .next()
                    .unwrap();
                let asset_url = format!("/api/sam/jobs/{id}/video-asset");
                let app = support::router(active.clone().with_sam_config(None));
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
                let saved = app
                    .clone()
                    .oneshot(request(
                        "POST",
                        &asset_url,
                        &actor.token,
                        json!({"projectId":project.id}),
                    ))
                    .await
                    .unwrap();
                assert_eq!(saved.status(), StatusCode::OK);
                let first = value(saved).await;
                let saved = value(
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
                assert_eq!(first, saved);
                archive_region(
                    &active,
                    &actor.token,
                    &actor.user.username,
                    &saved,
                    &expected,
                )
                .await;
                let snapshot: video_kadr_backend::domain::video_region::Attachment =
                    serde_json::from_value(saved["snapshot"].clone()).unwrap();
                assert_eq!(snapshot.mask_hash, meta["tracking"]["outputHash"]);
                assert_eq!(snapshot.seed_frame, 1);
                assert_eq!(snapshot.frames, 4);
                assert_ne!(snapshot.source_id, "sam-video-source");
                assert_ne!(snapshot.source_id, snapshot.mask_source_id);
                for (field, expected) in [
                    ("source", std::fs::read(&actual_source).unwrap()),
                    ("mask", encoded.to_vec()),
                ] {
                    let entry = active
                        .library
                        .get(saved[field]["id"].as_str().unwrap())
                        .await
                        .unwrap();
                    let path = active.library.resolve_media_path(&entry).await.unwrap();
                    assert_eq!(std::fs::read(&path).unwrap(), expected);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        assert_eq!(
                            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                            0o600
                        );
                    }
                    assert_eq!(
                        active.db.source_space_id(&entry.id).await.unwrap(),
                        project.space_id
                    );
                }
                assert_eq!(
                    saved,
                    value(
                        app.clone()
                            .oneshot(request(
                                "POST",
                                &asset_url,
                                &actor.token,
                                json!({"projectId":project.id})
                            ))
                            .await
                            .unwrap()
                    )
                    .await,
                    "acceptance is idempotent without models"
                );
                let copied = active.library.get(&snapshot.mask_source_id).await.unwrap();
                let copied = active.library.resolve_media_path(&copied).await.unwrap();
                let bytes = std::fs::read(&copied).unwrap();
                std::fs::write(&copied, b"corrupt").unwrap();
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
                assert_eq!(std::fs::read(&copied).unwrap(), b"corrupt");
                std::fs::write(&copied, bytes).unwrap();
            }
            let original = std::fs::read(&target).unwrap();
            std::fs::write(&target, b"corrupt").unwrap();
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        &format!("/api/sam/jobs/{id}/video"),
                        &actor.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CONFLICT
            );
            std::fs::write(&target, original).unwrap();
            std::fs::write(&actual_source, b"changed").unwrap();
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        &format!("/api/sam/jobs/{id}/video"),
                        &actor.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CONFLICT
            );
        } else {
            assert_eq!(
                job.status,
                if mode.ends_with("cancel") {
                    JobStatus::Cancelled
                } else {
                    JobStatus::Error
                },
                "{job:?}"
            );
            assert!(!target.exists());
            assert!(!target.with_extension("gray").exists());
        }
        active.begin_shutdown();
        assert!(
            active
                .wait_for_tasks(std::time::Duration::from_secs(15))
                .await
        );
        if let Ok(entries) = std::fs::read_dir(active.storage.join("sam-jobs")) {
            assert!(entries
                .map(|e| e.unwrap())
                .all(|e| !e.file_name().to_string_lossy().starts_with(".sam-")));
        }
    }
}

#[test]
fn portable_video_region_admits_clock_losses_and_rejects_corrupt_provenance() {
    use video_kadr_backend::domain::video_region::Attachment;
    let points = vec![[0.5f64, 0.5]];
    let data = json!({"sourceId":"original","maskSourceId":"region","sourceHash":"a".repeat(64),"maskHash":"b".repeat(64),"maskFileHash":"c".repeat(64),"encoderHash":"d".repeat(64),"decoderHash":"e".repeat(64),"runtimeHash":"f".repeat(64),"promptHash":format!("{:x}",Sha256::digest(serde_json::to_vec(&points).unwrap())),"points":points,"width":37,"height":23,"frames":4,"frameRate":{"numerator":30000,"denominator":1001},"seedFrame":1,"trackedFrames":4,"forwardLostAt":null,"backwardLostAt":null});
    let parsed: Attachment = serde_json::from_value(data.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), data);
    for (field, bad) in [
        ("sourceId", json!("../escape")),
        ("maskSourceId", json!("original")),
        ("maskHash", json!("invalid")),
        ("points", json!([[0.1, 0.1]])),
        ("width", json!(0)),
        ("frames", json!(360001)),
        ("frameRate", json!({"numerator":0,"denominator":1001})),
        ("seedFrame", json!(4)),
        ("trackedFrames", json!(3)),
        ("forwardLostAt", json!(1)),
        ("backwardLostAt", json!(1)),
        ("unexpected", json!(true)),
    ] {
        let mut bad_data = data.clone();
        bad_data[field] = bad;
        assert!(
            serde_json::from_value::<Attachment>(bad_data).is_err(),
            "{field}"
        );
    }
    let mut loss = data.clone();
    loss["forwardLostAt"] = json!(3);
    loss["backwardLostAt"] = json!(0);
    loss["trackedFrames"] = json!(2);
    serde_json::from_value::<Attachment>(loss).unwrap();
}

async fn archive_region(
    state: &AppState,
    token: &str,
    actor: &str,
    saved: &Value,
    expected: &[u8],
) {
    use video_kadr_backend::domain::{
        composition::*,
        metadata_burnin::{CompositionMetadata, MediaVersions},
    };
    let region: video_kadr_backend::domain::video_region::Attachment =
        serde_json::from_value(saved["snapshot"].clone()).unwrap();
    let original = state.library.get(&region.source_id).await.unwrap();
    let original_path = state.library.resolve_media_path(&original).await.unwrap();
    let copy = state.sources_dir().join("sam-holder-copy.mov");
    std::fs::copy(&original_path, &copy).unwrap();
    let auto = state.sources_dir().join("sam-holder-auto.mkv");
    let raw = state.storage.join("sam-holder-auto.gray");
    let planes = vec![255u8; 512 * 320 * 4];
    std::fs::write(&raw, &planes).unwrap();
    let result = std::process::Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            "gray",
            "-video_size",
            "512x320",
            "-framerate",
            "4",
            "-i",
            raw.to_str().unwrap(),
            "-c:v",
            "ffv1",
            "-pix_fmt",
            "gray",
            auto.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let auto_hash = format!("{:x}", Sha256::digest(std::fs::read(&auto).unwrap()));
    for (id, filename, w, h) in [
        ("sam-holder-copy", "sam-holder-copy.mov", 37, 23),
        ("sam-holder-auto", "sam-holder-auto.mkv", 512, 320),
    ] {
        assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"video","width":w,"height":h,"duration":1.,"fps":4.}))).await);
    }
    let mut content = Composition::new(CanvasSpec::default());
    for (id, w, h) in [
        (region.source_id.as_str(), 37, 23),
        (region.mask_source_id.as_str(), 256, 256),
        ("sam-holder-copy", 37, 23),
        ("sam-holder-auto", 512, 320),
    ] {
        let id = SourceId::parse(id).unwrap();
        content.sources.insert(
            id.clone(),
            CompositionSource {
                id,
                kind: SourceKind::Video,
                duration_ticks: 1_000_000,
                width: w,
                height: h,
                has_audio: false,
                audio_channels: None,
            },
        );
    }
    let clip = CompositionClipId::parse("clip").unwrap();
    content.tracks.push(CompositionTrack::Video {
        id: TrackId::parse("video").unwrap(),
        name: "Video".into(),
        hidden: false,
        muted: false,
        locked: false,
        transitions: vec![],
        clips: vec![VideoClip {concat_cutout:None,audio_stream:None,concat_gain_keys:Vec::new(),dsp_effects:Vec::new(),independent_audio:false,audio_fade_in_ticks:0,audio_fade_out_ticks:0,
            id: clip.clone(),
            source_id: SourceId::parse("sam-holder-copy").unwrap(),
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
            source_audio_enabled: false,
            audio_gain: AnimatableValue::constant(1.),
            audio_pan: AnimatableValue::constant(0.),
            enabled: true,
        }],
    });
    let mut metadata = CompositionMetadata::default();
    metadata.media_versions.insert(clip.clone(),MediaVersions{original_source_id:SourceId::parse(region.source_id.clone()).unwrap(),copy_source_id:SourceId::parse("sam-holder-copy").unwrap(),cutout:None,video_cutout:Some(serde_json::from_value(json!({"sourceId":region.source_id,"maskSourceId":"sam-holder-auto","sourceHash":region.source_hash,"maskHash":format!("{:x}",Sha256::digest(&planes)),"maskFileHash":auto_hash,"width":37,"height":23,"maskWidth":512,"maskHeight":320,"frames":4,"frameRate":{"numerator":4,"denominator":1},"corrections":[],"trackedRegions":[region]})).unwrap())});
    content.metadata = Some(metadata);
    content.validate().unwrap();
    let app = support::router(state.clone().with_sam_config(None));
    let document = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":content,"instances":[]}}});
    let source_space = state
        .db
        .source_space_id(&original.id)
        .await
        .unwrap()
        .unwrap();
    let create = request(
        "POST",
        "/api/composition-projects",
        token,
        json!({"schemaVersion":2,"mode":"composition","spaceId":source_space,"name":"Actual tracked regions","document":document}),
    );
    let response = app.clone().oneshot(create).await.unwrap();
    let status = response.status();
    let project = value(response).await;
    assert_eq!(status, StatusCode::CREATED, "{project}");
    let response = app
        .clone()
        .oneshot(request(
            "GET",
            &format!(
                "/api/composition-projects/{}/archive",
                project["id"].as_str().unwrap()
            ),
            token,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let archive = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    let space = state
        .db
        .create_space(actor, "Imported SAM tracking")
        .await
        .unwrap();
    let mut multipart=b"--SAM_TRACK\r\nContent-Disposition: form-data; name=\"file\"; filename=\"regions.veproj\"\r\nContent-Type: application/vnd.video-editor.project\r\n\r\n".to_vec();
    multipart.extend_from_slice(&archive);
    multipart.extend_from_slice(b"\r\n--SAM_TRACK--\r\n");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/composition-projects/import")
                .header("authorization", format!("Bearer {token}"))
                .header("x-space-id", &space.id)
                .header("content-type", "multipart/form-data; boundary=SAM_TRACK")
                .body(Body::from(multipart))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let restored = value(response).await;
    assert_eq!(status, StatusCode::CREATED, "{restored}");
    let content: Composition = serde_json::from_value(
        restored["project"]["document"]["compositions"]["root"]["content"].clone(),
    )
    .unwrap();
    content.validate().unwrap();
    let imported = &content.metadata.as_ref().unwrap().media_versions[&clip]
        .video_cutout
        .as_ref()
        .unwrap()
        .tracked_regions[0];
    assert_ne!(imported.source_id, region.source_id);
    assert_ne!(imported.mask_source_id, region.mask_source_id);
    let mut remapped = region.clone();
    remapped.source_id = imported.source_id.clone();
    remapped.mask_source_id = imported.mask_source_id.clone();
    assert_eq!(*imported, remapped);
    let entry = state.library.get(&imported.mask_source_id).await.unwrap();
    assert_eq!(
        state.db.source_space_id(&entry.id).await.unwrap(),
        Some(space.id)
    );
    let path = state.library.resolve_media_path(&entry).await.unwrap();
    let tools = video_kadr_backend::services::local_enhance_video::resolve_tools()
        .await
        .unwrap();
    let destination = state.storage.join("imported-sam-region.gray");
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    video_kadr_backend::services::local_person_mask_video::decode(
        &state.process_runtime,
        video_kadr_backend::services::local_person_mask_video::Tools {
            ffmpeg: &tools.ffmpeg,
            ffprobe: &tools.ffprobe,
        },
        &path,
        &video_kadr_backend::services::local_person_mask_video::Metadata {
            width: 256,
            height: 256,
            frames: imported.frames,
            frame_rate: imported.frame_rate,
            mask_hash: imported.mask_hash.clone(),
            file_hash: imported.mask_file_hash.clone(),
            size_bytes: std::fs::metadata(&path).unwrap().len(),
        },
        &destination,
        64 * 1024 * 1024 * 1024,
        &tokio_util::sync::CancellationToken::new(),
        std::time::Duration::from_secs(30),
        &tx,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(destination).unwrap(), expected);
}
