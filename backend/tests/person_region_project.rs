mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tower::ServiceExt;
use video_kadr_backend::{
    config::resource_classes::ResourceClass,
    domain::{
        composition::*,
        metadata_burnin::{CompositionMetadata, MediaVersions},
        video_cutout::Attachment,
    },
    library::MediaEntry,
    model::JobStatus,
    state::AppState,
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn request(method: &str, path: &str, token: &str, data: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(data.to_string()))
        .unwrap()
}
async fn value(response: axum::response::Response) -> Value {
    serde_json::from_slice(
        &to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}
async fn terminal(state: &AppState, id: &str) -> video_kadr_backend::model::Job {
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if matches!(
                job.status,
                JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
            ) {
                break job;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
fn run(tool: &Path, args: &[&str]) -> Vec<u8> {
    let r = std::process::Command::new(tool)
        .args(args)
        .output()
        .unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    r.stdout
}
// Opt-in extension of the real imported-project fixture: no synthetic SAM replies.
async fn session_region_queue_recovery(
    previous: &AppState,
    config: video_kadr_backend::services::local_person::Config,
    token: &str,
    project: &str,
    parent: &str,
    baseline: &Value,
    automatic: &[u8],
) {
    let reopen = || async {
        AppState::new(
            previous.storage.clone(),
            2,
            (*previous.tools).clone(),
            video_kadr_backend::library::Library::load(previous.storage.clone()).await,
            video_kadr_backend::db::Db::open(&previous.storage)
                .await
                .unwrap(),
        )
        .with_person_config(Some(config.clone()))
    };
    let state =
        reopen()
            .await
            .with_sam_config(Some(video_kadr_backend::services::local_sam::Config {
                executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
                encoder: std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap().into(),
                decoder: std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap().into(),
                runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
                    .unwrap()
                    .into(),
            }));
    let app = support::router(state.clone());
    let snapshot = &baseline["compositions"]["root"]["content"]["metadata"]["mediaVersions"]
        ["clip"]["videoCutout"];
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/sam/jobs",
            token,
            json!({"sourceId":snapshot["sourceId"],"points":[[0.5,0.5]],"seedFrame":1}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let sam = value(response).await["jobId"].as_str().unwrap().to_owned();
    let result = terminal(&state, &sam).await;
    assert_eq!(result.status, JobStatus::Done, "{result:?}");
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
            &format!("/api/person/video-jobs/{parent}/regions"),
            token,
            json!({"samJobId":sam,"corrections":[]}),
        ))
        .await
        .unwrap();
    let status = response.status();
    let admitted = value(response).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{admitted}");
    let id = admitted["jobId"].as_str().unwrap();
    state.begin_shutdown();
    drop(held);
    assert!(state.wait_for_tasks(Duration::from_secs(15)).await);
    // Reconstruct DB/library/dispatcher with NO SAM configuration. Only replay is needed.
    let recovered = reopen().await;
    recovered.recover_jobs().await;
    video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
    let result = terminal(&recovered, id).await;
    assert_eq!(result.status, JobStatus::Done, "{result:?}");
    assert_eq!(
        std::fs::read(
            recovered
                .storage
                .join("person-video-jobs")
                .join(format!("{id}.masks"))
        )
        .unwrap(),
        automatic
    );
    let app = support::router(recovered.clone());
    let response = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/person/video-jobs/{id}/corrections"),
            token,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let recipe = value(response).await;
    assert_eq!(&recipe["projectOrigin"]["savedSnapshot"], snapshot);
    let regions = recipe["projectOrigin"]["snapshot"]["trackedRegions"]
        .as_array()
        .unwrap();
    assert_eq!(
        regions.len(),
        snapshot["trackedRegions"].as_array().unwrap().len() + 1
    );
    let region = regions.last().unwrap()["maskSourceId"].as_str().unwrap();
    let response = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/person/video-jobs/{id}/regions/{region}/1"),
            token,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/composition-projects/{project}"),
            token,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(&value(response).await["document"], baseline);
    recovered.begin_shutdown();
    assert!(recovered.wait_for_tasks(Duration::from_secs(15)).await);
    let source_ids = baseline["compositions"]["root"]["content"]["sources"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let oracle = PathBuf::from(std::env::var("VIDEO_KADR_TEST_PERSON_REGIONS_ORACLE_DIR").unwrap())
        .join("37x23");
    let fixture: Value =
        serde_json::from_slice(&std::fs::read(oracle.join("valid.json")).unwrap()).unwrap();
    let learned = regions.last().unwrap();
    let original_planes = std::fs::read(fixture["regions"][0]["input"].as_str().unwrap()).unwrap();
    assert_eq!(learned["maskHash"], hash(&original_planes));
    let mut smart_edits = fixture["corrections"].clone();
    for edit in smart_edits.as_array_mut().unwrap() {
        for stroke in edit["correction"]["strokes"].as_array_mut().unwrap() {
            for key in [
                "sourceId",
                "maskSourceId",
                "sourceHash",
                "maskFileHash",
                "encoderHash",
                "decoderHash",
                "runtimeHash",
                "promptHash",
                "points",
            ] {
                stroke["region"][key] = learned[key].clone();
            }
        }
    }
    // Compare the public canonical recipe, including defaulted `at: null` fields.
    let normalized: Vec<video_kadr_backend::domain::video_cutout::FrameCorrection> =
        serde_json::from_value(smart_edits).unwrap();
    let smart_edits = serde_json::to_value(normalized).unwrap();
    for mode in [
        "running_recover",
        "running_corrected",
        "snapshot_changed",
        "cancelled",
    ] {
        let running = mode.starts_with("running_");
        let queued = reopen().await;
        let app = support::router(queued.clone());
        let mut held = Vec::new();
        for _ in 0..queued.resource_limit(ResourceClass::Analysis) {
            held.push(
                queued
                    .acquire_job_slot(ResourceClass::Analysis)
                    .await
                    .unwrap(),
            );
        }
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                &if mode == "running_corrected" {
                    format!("/api/person/video-jobs/{id}/corrections")
                } else {
                    format!("/api/person/video-jobs/{parent}/regions")
                },
                token,
                if mode == "running_corrected" {
                    json!({"corrections":smart_edits})
                } else {
                    json!({"samJobId":sam,"corrections":[]})
                },
            ))
            .await
            .unwrap();
        let status = response.status();
        let admitted = value(response).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{mode}: {admitted}");
        let id = admitted["jobId"].as_str().unwrap();
        if running {
            held.clear();
            tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let job = queued.get_job(id).await.unwrap().unwrap();
                    if job.status == JobStatus::Running && job.progress.unwrap_or(0.) > 0. {
                        break;
                    }
                    assert!(
                        !matches!(
                            job.status,
                            JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
                        ),
                        "expected in-flight work: {job:?}"
                    );
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .unwrap();
        }
        if mode == "snapshot_changed" {
            let mut changed = baseline.clone();
            changed["compositions"]["root"]["content"]["metadata"]["mediaVersions"]["clip"]
                ["videoCutout"]["trackedRegions"][0]["runtimeHash"] = json!("d".repeat(64));
            fixture_snapshot(&queued, project, "Changed baseline", &changed, &source_ids).await;
        } else if mode == "cancelled" {
            let response = app
                .oneshot(request(
                    "POST",
                    &format!("/api/jobs/{id}/cancel"),
                    token,
                    json!({}),
                ))
                .await
                .unwrap();
            assert!(response.status().is_success(), "{}", response.status());
        }
        queued.begin_shutdown();
        drop(held);
        assert!(queued.wait_for_tasks(Duration::from_secs(15)).await);
        if running {
            assert_eq!(
                queued.get_job(id).await.unwrap().unwrap().status,
                JobStatus::Running
            );
        }
        let recovered = reopen().await;
        recovered.recover_jobs().await;
        if running {
            assert_eq!(
                recovered.get_job(id).await.unwrap().unwrap().status,
                JobStatus::Pending
            );
        }
        video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
        let result = terminal(&recovered, id).await;
        assert_eq!(
            result.status,
            if running {
                JobStatus::Done
            } else if mode == "cancelled" {
                JobStatus::Cancelled
            } else {
                JobStatus::Error
            },
            "{mode}: {result:?}"
        );
        let masks = recovered
            .storage
            .join("person-video-jobs")
            .join(format!("{id}.masks"));
        if running {
            assert_eq!(
                std::fs::read(masks).unwrap(),
                if mode == "running_corrected" {
                    std::fs::read(oracle.join("expected.gray")).unwrap()
                } else {
                    automatic.to_vec()
                }
            );
            let app = support::router(recovered.clone());
            let response = app
                .clone()
                .oneshot(request(
                    "GET",
                    &format!("/api/person/video-jobs/{id}/corrections"),
                    token,
                    Value::Null,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let restored_recipe = value(response).await;
            assert_eq!(restored_recipe["projectOrigin"], recipe["projectOrigin"]);
            if mode == "running_corrected" {
                assert_eq!(restored_recipe["corrections"], smart_edits);
                let movie = recovered
                    .storage
                    .join("person-video-jobs")
                    .join(format!("{id}.mov"));
                let ffmpeg = PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFMPEG").unwrap());
                assert_eq!(
                    run(
                        &ffmpeg,
                        &[
                            "-nostdin",
                            "-v",
                            "error",
                            "-i",
                            movie.to_str().unwrap(),
                            "-map",
                            "0:v:0",
                            "-an",
                            "-pix_fmt",
                            "rgba",
                            "-fps_mode",
                            "passthrough",
                            "-f",
                            "rawvideo",
                            "pipe:1"
                        ]
                    ),
                    std::fs::read(oracle.join("expected.rgba")).unwrap()
                );
            }
            let response = app
                .oneshot(request(
                    "GET",
                    &format!("/api/composition-projects/{project}"),
                    token,
                    Value::Null,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(&value(response).await["document"], baseline);
        } else {
            assert!(!masks.exists());
        }
        recovered.begin_shutdown();
        assert!(recovered.wait_for_tasks(Duration::from_secs(15)).await);
        if mode == "snapshot_changed" {
            fixture_snapshot(
                &recovered,
                project,
                "Restored baseline",
                baseline,
                &source_ids,
            )
            .await;
        }
    }
}
async fn archive_import(
    state: &AppState,
    app: &axum::Router,
    id: &str,
    token: &str,
    actor: &str,
) -> Value {
    let response = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/composition-projects/{id}/archive"),
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
        .create_space(actor, "Imported tracked correction")
        .await
        .unwrap();
    let mut bytes=b"--REGION_PROJECT\r\nContent-Disposition: form-data; name=\"file\"; filename=\"regions.veproj\"\r\nContent-Type: application/vnd.video-editor.project\r\n\r\n".to_vec();
    bytes.extend_from_slice(&archive);
    bytes.extend_from_slice(b"\r\n--REGION_PROJECT--\r\n");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/composition-projects/import")
                .header("authorization", format!("Bearer {token}"))
                .header("x-space-id", space.id)
                .header(
                    "content-type",
                    "multipart/form-data; boundary=REGION_PROJECT",
                )
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let data = value(response).await;
    assert_eq!(status, StatusCode::CREATED, "{data}");
    data["project"].clone()
}
fn content(snapshot: &Attachment) -> Composition {
    let mut value = Composition::new(CanvasSpec::default());
    for (id, width, height) in [
        ("original", 37, 23),
        ("copy", 37, 23),
        ("automatic-masks", 512, 320),
    ] {
        let id = SourceId::parse(id).unwrap();
        value.sources.insert(
            id.clone(),
            CompositionSource {
                id: id.clone(),
                kind: SourceKind::Video,
                duration_ticks: 1_000_000,
                width,
                height,
                has_audio: id.as_str() == "original" || id.as_str() == "copy",
                audio_channels: None,
            },
        );
    }
    let tracked = SourceId::parse("tracked").unwrap();
    value.sources.insert(
        tracked.clone(),
        CompositionSource {
            id: tracked,
            kind: SourceKind::Video,
            duration_ticks: 1_000_000,
            width: 256,
            height: 256,
            has_audio: false,
            audio_channels: None,
        },
    );
    let clip = CompositionClipId::parse("clip").unwrap();
    value.tracks.push(CompositionTrack::Video {
        id: TrackId::parse("video").unwrap(),
        name: "Video".into(),
        hidden: false,
        muted: false,
        locked: false,
        transitions: vec![],
        clips: vec![VideoClip {concat_cutout:None,audio_stream:None,concat_gain_keys:Vec::new(),dsp_effects:Vec::new(),independent_audio:false,audio_fade_in_ticks:0,audio_fade_out_ticks:0,
            id: clip.clone(),
            source_id: SourceId::parse("copy").unwrap(),
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
    metadata.media_versions.insert(
        clip,
        MediaVersions {
            original_source_id: SourceId::parse("original").unwrap(),
            copy_source_id: SourceId::parse("copy").unwrap(),
            cutout: None,
            video_cutout: Some(snapshot.clone()),
        },
    );
    value.metadata = Some(metadata);
    value
}
// Fixture corruption deliberately races live job bookkeeping. Retry the complete
// test-only transaction on SQLite busy snapshots; keep all refusal assertions.
async fn fixture_snapshot(
    state: &AppState,
    project: &str,
    name: &str,
    document: &Value,
    sources: &[String],
) {
    for _ in 0..50 {
        match state
            .db
            .update_composition_project(project, name, document, sources)
            .await
        {
            Ok(Some(_)) => return,
            Ok(None) => panic!("fixture project vanished"),
            Err(error) => {
                let busy = error
                    .downcast_ref::<sqlx::Error>()
                    .and_then(|e| e.as_database_error())
                    .and_then(|e| e.code())
                    .is_some_and(|code| matches!(code.as_ref(), "5" | "517"));
                if !busy {
                    panic!("fixture update failed: {error}");
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
    panic!("fixture snapshot stayed busy after bounded retries");
}
#[tokio::test]
#[ignore = "requires native model-free binary, host FFmpeg/FFprobe and retained region replay fixtures; no models/ORT"]
async fn actual_imported_project_bound_regions_queue_recovery_security_and_acceptance() {
    let binary = PathBuf::from(std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap());
    let oracle = PathBuf::from(std::env::var("VIDEO_KADR_TEST_PERSON_REGIONS_ORACLE_DIR").unwrap())
        .join("37x23");
    let tool = PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFMPEG").unwrap());
    let fixture: Value =
        serde_json::from_slice(&std::fs::read(oracle.join("valid.json")).unwrap()).unwrap();
    let (state, _root) = support::make_state(true, false).await;
    let config = video_kadr_backend::services::local_person::Config {
        executable: binary.clone(),
        model: state.storage.join("absent-model.onnx"),
        runtime_library: state.storage.join("absent-ort.dylib"),
    };
    let state = state.with_person_config(Some(config.clone()));
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    let owner = state
        .db
        .register_auth_user("region-project-owner", "public fixture password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("region-project-outsider", "public fixture password")
        .await
        .unwrap()
        .unwrap();
    for (id, input, size, format, codec, extension, audio) in [
        (
            "original",
            fixture["input"].as_str().unwrap(),
            "37x23",
            "rgba",
            "qtrle",
            "mov",
            true,
        ),
        (
            "copy",
            fixture["input"].as_str().unwrap(),
            "37x23",
            "rgba",
            "qtrle",
            "mov",
            true,
        ),
        (
            "automatic-masks",
            fixture["mask"].as_str().unwrap(),
            "512x320",
            "gray",
            "ffv1",
            "mkv",
            false,
        ),
        (
            "tracked",
            fixture["regions"][0]["input"].as_str().unwrap(),
            "256x256",
            "gray",
            "ffv1",
            "mkv",
            false,
        ),
    ] {
        let filename = format!("{id}.{extension}");
        let path = state.sources_dir().join(&filename);
        let mut args = vec![
            "-nostdin",
            "-v",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            format,
            "-video_size",
            size,
            "-framerate",
            "4",
            "-i",
            input,
        ];
        if audio {
            args.extend([
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=733:sample_rate=48000:duration=1",
                "-c:a",
                "pcm_s16le",
                "-shortest",
            ]);
        }
        args.extend([
            "-c:v",
            codec,
            "-pix_fmt",
            if audio { "argb" } else { "gray" },
            path.to_str().unwrap(),
        ]);
        run(&tool, &args);
        let (w, h) = size.split_once('x').unwrap();
        let entry = MediaEntry::from_result(
            "source",
            &json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"video","width":w.parse::<u32>().unwrap(),"height":h.parse::<u32>().unwrap(),"duration":1.,"fps":4.,"vcodec":codec,"acodec":if audio{Some("pcm_s16le")}else{None},"sizeBytes":std::fs::metadata(&path).unwrap().len()}),
        );
        assert!(state.library.add(entry).await);
    }
    let source_hash = hash(&std::fs::read(state.sources_dir().join("original.mov")).unwrap());
    let tracked_hash = hash(&std::fs::read(state.sources_dir().join("tracked.mkv")).unwrap());
    let automatic_hash =
        hash(&std::fs::read(state.sources_dir().join("automatic-masks.mkv")).unwrap());
    let auto = std::fs::read(fixture["mask"].as_str().unwrap()).unwrap();
    let mut region = fixture["regions"][0]["attachment"].clone();
    region["sourceHash"] = json!(source_hash);
    region["maskFileHash"] = json!(tracked_hash);
    for (key, val) in [
        ("width", json!(37)),
        ("height", json!(23)),
        ("frames", json!(4)),
        ("frameRate", json!({"numerator":4,"denominator":1})),
        ("seedFrame", json!(1)),
        ("trackedFrames", json!(4)),
        ("forwardLostAt", Value::Null),
        ("backwardLostAt", Value::Null),
    ] {
        region[key] = val;
    }
    let snapshot:Attachment=serde_json::from_value(json!({"sourceId":"original","maskSourceId":"automatic-masks","sourceHash":source_hash,"maskHash":hash(&auto),"maskFileHash":automatic_hash,"width":37,"height":23,"maskWidth":512,"maskHeight":320,"frames":4,"frameRate":{"numerator":4,"denominator":1},"corrections":[],"trackedRegions":[region]})).unwrap();
    let document = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":content(&snapshot),"instances":[]}}});
    let app = support::router(state.clone());
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/composition-projects",
            &owner.token,
            json!({"schemaVersion":2,"mode":"composition","name":"Regions","document":document}),
        ))
        .await
        .unwrap();
    let status = response.status();
    let created = value(response).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let imported = archive_import(
        &state,
        &app,
        created["id"].as_str().unwrap(),
        &owner.token,
        &owner.user.username,
    )
    .await;
    let project = imported["id"].as_str().unwrap();
    let saved = imported["document"].clone();
    let snapshot: Attachment = serde_json::from_value(
        saved["compositions"]["root"]["content"]["metadata"]["mediaVersions"]["clip"]
            ["videoCutout"]
            .clone(),
    )
    .unwrap();
    let tracked = &snapshot.tracked_regions[0];
    let mut edits = fixture["corrections"].clone();
    for edit in edits.as_array_mut().unwrap() {
        for stroke in edit["correction"]["strokes"].as_array_mut().unwrap() {
            let r = &mut stroke["region"];
            r["sourceId"] = json!(tracked.source_id);
            r["maskSourceId"] = json!(tracked.mask_source_id);
            r["sourceHash"] = json!(tracked.source_hash);
            r["maskFileHash"] = json!(tracked.mask_file_hash);
        }
    }
    let endpoint = format!("/api/composition-projects/{project}/video-cutout/corrections");
    let body = |edits: Value| json!({"compositionId":"root","clipId":"clip","expectedAttachment":snapshot,"corrections":edits});
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                &endpoint,
                &outsider.token,
                body(edits.clone())
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    for field in ["maskSourceId", "runtimeHash"] {
        let mut bad = edits.clone();
        bad[0]["correction"]["strokes"][0]["region"][field] = json!(if field == "maskSourceId" {
            "unregistered".into()
        } else {
            "0".repeat(64)
        });
        assert_eq!(
            app.clone()
                .oneshot(request("POST", &endpoint, &owner.token, body(bad)))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            &endpoint,
            &owner.token,
            body(edits.clone()),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let id = value(response).await["jobId"].as_str().unwrap().to_owned();
    let job = terminal(&state, &id).await;
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    let raw = state
        .storage
        .join("person-video-jobs")
        .join(format!("{id}.masks"));
    assert_eq!(
        std::fs::read(&raw).unwrap(),
        std::fs::read(oracle.join("expected.gray")).unwrap()
    );
    let movie = state
        .storage
        .join("person-video-jobs")
        .join(format!("{id}.mov"));
    assert_eq!(
        run(
            &tool,
            &[
                "-nostdin",
                "-v",
                "error",
                "-i",
                movie.to_str().unwrap(),
                "-map",
                "0:v:0",
                "-an",
                "-fps_mode",
                "passthrough",
                "-pix_fmt",
                "rgba",
                "-f",
                "rawvideo",
                "pipe:1"
            ]
        ),
        std::fs::read(oracle.join("expected.rgba")).unwrap()
    );
    let recipe = value(
        app.clone()
            .oneshot(request(
                "GET",
                &format!("/api/person/video-jobs/{id}/corrections"),
                &owner.token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    let normalized: Vec<video_kadr_backend::domain::video_cutout::FrameCorrection> =
        serde_json::from_value(edits.clone()).unwrap();
    assert_eq!(
        recipe["corrections"],
        serde_json::to_value(&normalized).unwrap()
    );
    assert_eq!(
        recipe["projectOrigin"]["snapshot"],
        serde_json::to_value(&snapshot).unwrap()
    );
    let region_bytes = std::fs::read(fixture["regions"][0]["input"].as_str().unwrap()).unwrap();
    let region_url = |frame: u64| {
        format!(
            "/api/person/video-jobs/{id}/regions/{}/{frame}",
            tracked.mask_source_id
        )
    };
    for frame in 0..4u64 {
        let response = app
            .clone()
            .oneshot(request(
                "GET",
                &region_url(frame),
                &owner.token,
                Value::Null,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["content-type"], "image/png");
        assert_eq!(response.headers()["x-source-frame"], frame.to_string());
        let plane = &region_bytes[frame as usize * 65536..(frame as usize + 1) * 65536];
        assert_eq!(response.headers()["x-mask-sha256"], hash(plane));
        let png = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(
            image::load_from_memory(&png).unwrap().to_luma8().as_raw(),
            plane
        );
    }
    assert_eq!(
        app.clone()
            .oneshot(request("GET", &region_url(0), &outsider.token, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.clone()
            .oneshot(request("GET", &region_url(0), "", Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(request("GET", &region_url(4), &owner.token, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                "GET",
                &format!("/api/person/video-jobs/{id}/regions/missing/0"),
                &owner.token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let asset_url = format!("/api/person/video-jobs/{id}/asset");
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            &asset_url,
            &owner.token,
            json!({"projectId":project}),
        ))
        .await
        .unwrap();
    let status = response.status();
    let accepted = value(response).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let accepted_snapshot: Attachment =
        serde_json::from_value(accepted["videoCutout"]["snapshot"].clone()).unwrap();
    assert_eq!(accepted_snapshot.corrections, normalized);
    assert_eq!(accepted_snapshot.tracked_regions, snapshot.tracked_regions);
    assert_eq!(
        value(
            app.clone()
                .oneshot(request(
                    "POST",
                    &asset_url,
                    &owner.token,
                    json!({"projectId":project})
                ))
                .await
                .unwrap()
        )
        .await["videoCutout"],
        accepted["videoCutout"]
    );
    // Persist the authored annotations, archive them again, then replay after a
    // second Space import. This project is independent of the queued controls below.
    let mut bound_document = saved.clone();
    let content = &mut bound_document["compositions"]["root"]["content"];
    for entry in [
        &accepted,
        &accepted["videoCutout"]["source"],
        &accepted["videoCutout"]["mask"],
    ] {
        let id = entry["id"].as_str().unwrap();
        content["sources"][id] = json!({"id":id,"kind":"video","durationTicks":(entry["duration"].as_f64().unwrap_or(1.)*1_000_000.).round() as u64,"width":entry["width"],"height":entry["height"],"hasAudio":entry["acodec"].is_string(),"audioChannels":null});
    }
    content["tracks"][0]["clips"][0]["sourceId"] = accepted["id"].clone();
    content["metadata"]["mediaVersions"]["clip"] = json!({"originalSourceId":accepted_snapshot.source_id,"copySourceId":accepted["id"],"videoCutout":accepted_snapshot});
    let typed: Composition = serde_json::from_value(content.clone()).unwrap();
    typed.validate().unwrap();
    let response=app.clone().oneshot(request("POST","/api/composition-projects",&owner.token,json!({"schemaVersion":2,"mode":"composition","name":"Authored regions","spaceId":imported["spaceId"],"document":bound_document}))).await.unwrap();
    let status = response.status();
    let bound = value(response).await;
    assert_eq!(status, StatusCode::CREATED, "{bound}");
    let ported = archive_import(
        &state,
        &app,
        bound["id"].as_str().unwrap(),
        &owner.token,
        &owner.user.username,
    )
    .await;
    let ported_snapshot: Attachment = serde_json::from_value(
        ported["document"]["compositions"]["root"]["content"]["metadata"]["mediaVersions"]["clip"]
            ["videoCutout"]
            .clone(),
    )
    .unwrap();
    assert_ne!(
        ported_snapshot.tracked_regions[0].mask_source_id,
        snapshot.tracked_regions[0].mask_source_id
    );
    assert_eq!(
        ported_snapshot.corrections[0].correction.strokes[0]
            .region
            .as_ref()
            .unwrap()
            .mask_source_id,
        ported_snapshot.tracked_regions[0].mask_source_id
    );
    let response=app.clone().oneshot(request("POST",&format!("/api/composition-projects/{}/video-cutout/corrections",ported["id"].as_str().unwrap()),&owner.token,json!({"compositionId":"root","clipId":"clip","expectedAttachment":ported_snapshot,"corrections":ported_snapshot.corrections}))).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let ported_job = value(response).await["jobId"].as_str().unwrap().to_owned();
    let completed = terminal(&state, &ported_job).await;
    assert_eq!(completed.status, JobStatus::Done, "{completed:?}");
    assert_eq!(
        std::fs::read(
            state
                .storage
                .join("person-video-jobs")
                .join(format!("{ported_job}.masks"))
        )
        .unwrap(),
        std::fs::read(oracle.join("expected.gray")).unwrap()
    );
    let tracked_entry = state.library.get(&tracked.mask_source_id).await.unwrap();
    let tracked_path = state
        .library
        .resolve_media_path(&tracked_entry)
        .await
        .unwrap();
    let tracked_bytes = std::fs::read(&tracked_path).unwrap();
    std::fs::write(&tracked_path, b"changed").unwrap();
    assert_eq!(
        app.clone()
            .oneshot(request("GET", &region_url(0), &owner.token, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                "GET",
                &format!("/api/person/video-jobs/{id}/video"),
                &owner.token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    std::fs::write(&tracked_path, &tracked_bytes).unwrap();
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
            &endpoint,
            &owner.token,
            body(edits.clone()),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let recovered_id = value(response).await["jobId"].as_str().unwrap().to_owned();
    let payload: String =
        sqlx::query_scalar("SELECT payload_json FROM job_requests WHERE job_id=?")
            .bind(&recovered_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!payload.contains(state.storage.to_str().unwrap()));
    state.begin_shutdown();
    drop(held);
    assert!(state.wait_for_tasks(Duration::from_secs(15)).await);
    let recovered = AppState::new(
        state.storage.clone(),
        2,
        (*state.tools).clone(),
        video_kadr_backend::library::Library::load(state.storage.clone()).await,
        video_kadr_backend::db::Db::open(&state.storage)
            .await
            .unwrap(),
    )
    .with_person_config(Some(config));
    recovered.recover_jobs().await;
    video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
    let job = terminal(&recovered, &recovered_id).await;
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    assert_eq!(
        std::fs::read(
            recovered
                .storage
                .join("person-video-jobs")
                .join(format!("{recovered_id}.masks"))
        )
        .unwrap(),
        std::fs::read(oracle.join("expected.gray")).unwrap()
    );
    let app = support::router(recovered.clone());
    let source_ids: Vec<String> = saved["compositions"]["root"]["content"]["sources"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    for mode in ["region", "snapshot", "revoked", "cancel"] {
        let mut held = vec![];
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
                &endpoint,
                &owner.token,
                body(edits.clone()),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let failed = value(response).await["jobId"].as_str().unwrap().to_owned();
        if mode == "region" {
            std::fs::write(&tracked_path, b"changed queued region").unwrap();
        }
        if mode == "snapshot" {
            let mut changed = saved.clone();
            changed["compositions"]["root"]["content"]["metadata"]["mediaVersions"]["clip"]
                ["videoCutout"]["trackedRegions"][0]["runtimeHash"] = json!("d".repeat(64));
            fixture_snapshot(&recovered, project, "Changed", &changed, &source_ids).await;
        }
        if ["revoked", "cancel"].contains(&mode) {
            sqlx::query("UPDATE composition_project_members SET role='viewer' WHERE project_id=? AND actor=?").bind(project).bind(&owner.user.username).execute(&pool).await.unwrap();
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        &format!("/api/jobs/{failed}"),
                        &owner.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &region_url(0), &owner.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
        if mode == "cancel" {
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "POST",
                        &format!("/api/jobs/{failed}/cancel"),
                        &owner.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
        }
        drop(held);
        let job = terminal(&recovered, &failed).await;
        assert_eq!(
            job.status,
            if mode == "cancel" {
                JobStatus::Cancelled
            } else {
                JobStatus::Error
            },
            "{mode}/{job:?}"
        );
        for ext in ["mov", "masks", "automatic.mkv", "original.masks"] {
            assert!(
                !recovered
                    .storage
                    .join("person-video-jobs")
                    .join(format!("{failed}.{ext}"))
                    .exists(),
                "{mode}/{ext}"
            );
        }
        if mode == "region" {
            std::fs::write(&tracked_path, &tracked_bytes).unwrap();
        }
        if mode == "snapshot" {
            fixture_snapshot(&recovered, project, "Restored", &saved, &source_ids).await;
        }
        if ["revoked", "cancel"].contains(&mode) {
            sqlx::query("UPDATE composition_project_members SET role='owner' WHERE project_id=? AND actor=?").bind(project).bind(&owner.user.username).execute(&pool).await.unwrap();
        }
    }
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            &format!("/api/person/video-jobs/{recovered_id}/corrections"),
            &owner.token,
            json!({"corrections":[]}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let reset = value(response).await["jobId"].as_str().unwrap().to_owned();
    let reset_job = terminal(&recovered, &reset).await;
    assert_eq!(reset_job.status, JobStatus::Done, "{reset_job:?}");
    assert_eq!(
        std::fs::read(
            recovered
                .storage
                .join("person-video-jobs")
                .join(format!("{reset}.masks"))
        )
        .unwrap(),
        auto
    );
    recovered.begin_shutdown();
    assert!(recovered.wait_for_tasks(Duration::from_secs(15)).await);
    if std::env::var_os("VIDEO_KADR_TEST_SESSION_REGION_RECOVERY").is_some() {
        session_region_queue_recovery(
            &recovered,
            video_kadr_backend::services::local_person::Config {
                executable: binary.clone(),
                model: recovered.storage.join("absent-model.onnx"),
                runtime_library: recovered.storage.join("absent-ort.dylib"),
            },
            &owner.token,
            project,
            &reset,
            &saved,
            &auto,
        )
        .await;
    }
    if let Ok(path) = std::env::var("VIDEO_KADR_TEST_REGION_BROWSER_FIXTURE") {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path).unwrap();
        file.write_all(&serde_json::to_vec(&json!({"storage":recovered.storage,"token":owner.token,"jobId":reset,"projectId":project,"regionId":tracked.mask_source_id,"executable":binary,"model":recovered.storage.join("absent-model.onnx"),"runtimeLibrary":recovered.storage.join("absent-ort.dylib"),"expectedMasks":oracle.join("expected.gray"),"expectedRgba":oracle.join("expected.rgba"),"sourceRgba":fixture["input"]})).unwrap()).unwrap();
        file.sync_all().unwrap();
        let _kept = _root.keep();
    }
}
