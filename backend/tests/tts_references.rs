mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
fn request(method: &str, path: &str, token: Option<&str>, body: Body) -> Request<Body> {
    let mut r = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "audio/wav");
    if let Some(token) = token {
        r = r.header("authorization", format!("Bearer {token}"));
    }
    r.body(body).unwrap()
}
fn job(token: &str, reference: Option<&str>, voice: u32) -> Request<Body> {
    job_text(token, reference, voice, "Private reference narration")
}
fn job_text(token: &str, reference: Option<&str>, voice: u32, text: &str) -> Request<Body> {
    let mut body = json!({"text":text,"voiceId":voice,"speed":1.});
    if let Some(id) = reference {
        body["referenceId"] = json!(id);
    }
    Request::builder()
        .method("POST")
        .uri("/api/speech/tts-jobs")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
async fn value(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}
#[tokio::test]
async fn private_upload_owner_admission_and_changed_reference_refusal() {
    use video_kadr_backend::{
        config::resource_classes::ResourceClass, model::JobStatus, services::local_tts::TtsConfig,
    };
    let (state, _dir) = support::make_state(true, false).await;
    let root = state.storage.join("pocket-fixture");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("lm_main.int8.onnx"), vec![0; 1_000_000]).unwrap();
    for name in [
        "lm_flow.int8.onnx",
        "encoder.onnx",
        "decoder.int8.onnx",
        "text_conditioner.onnx",
        "vocab.json",
        "token_scores.json",
    ] {
        std::fs::write(root.join(name), b"fixture").unwrap();
    }
    let executable = state.storage.join("unused-engine");
    std::fs::write(&executable, b"must not execute changed reference").unwrap();
    let state = state.with_tts_config(TtsConfig {
        executable,
        model_dir: Some(root),
        threads: 2,
    });
    let owner = state
        .db
        .register_auth_user("reference-owner", "reference test password")
        .await
        .unwrap()
        .unwrap();
    let other = state
        .db
        .register_auth_user("reference-other", "reference test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let wav =
        video_kadr_backend::domain::filmcraft_wav_audio::write_wav16(&vec![0.2; 24000], 1, 24000);
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                "/api/speech/tts-references",
                None,
                Body::from(wav.clone())
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    for invalid in [
        b"not a WAV".to_vec(),
        video_kadr_backend::domain::filmcraft_wav_audio::write_wav16(&vec![0.2; 6000], 1, 24000),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    "/api/speech/tts-references",
                    Some(&owner.token),
                    Body::from(invalid)
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let uploaded = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/speech/tts-references",
            Some(&owner.token),
            Body::from(wav),
        ))
        .await
        .unwrap();
    assert_eq!(uploaded.status(), StatusCode::CREATED);
    let uploaded = value(uploaded).await;
    let id = uploaded["id"].as_str().unwrap();
    assert_eq!(uploaded["durationUs"], 1_000_000);
    assert!(uploaded.get("path").is_none());
    let file = state
        .storage
        .join("tts-references")
        .join(format!("{id}.wav"));
    assert!(file.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o077,
            0
        );
    }
    assert_eq!(
        app.clone()
            .oneshot(request(
                "GET",
                &format!("/files/tts-references/{id}.wav"),
                None,
                Body::empty()
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    for (token, reference, voice, status) in [
        (&other.token, Some(id), 1099, StatusCode::NOT_FOUND),
        (&owner.token, None, 1099, StatusCode::BAD_REQUEST),
        (&owner.token, Some(id), 1000, StatusCode::BAD_REQUEST),
        (
            &owner.token,
            Some("../foreign"),
            1099,
            StatusCode::BAD_REQUEST,
        ),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(job(token, reference, voice))
                .await
                .unwrap()
                .status(),
            status
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
    let admitted = app
        .oneshot(job(&owner.token, Some(id), 1099))
        .await
        .unwrap();
    assert_eq!(admitted.status(), StatusCode::ACCEPTED);
    let admitted = value(admitted).await;
    let job_id = admitted["jobId"].as_str().unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    let payload: String =
        sqlx::query_scalar("SELECT payload_json FROM job_requests WHERE job_id=?")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!payload.contains(id));
    assert!(!payload.contains("Private reference narration"));
    assert_eq!(
        serde_json::from_str::<Value>(&payload).unwrap()["schema_version"],
        2
    );
    let mut changed = std::fs::read(&file).unwrap();
    changed[44] ^= 1;
    std::fs::write(&file, changed).unwrap();
    drop(held);
    let terminal = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let j = state.get_job(job_id).await.unwrap().unwrap();
            if j.status == JobStatus::Error {
                break j;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(terminal.error_kind.as_deref(), Some("validation"));
    assert!(terminal.error.unwrap().contains("Образец"));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tts_references")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
#[tokio::test]
#[ignore = "requires explicit learned TTS bundle, runtime and public Whisper model"]
async fn actual_private_reference_survives_cold_queued_job() {
    use video_kadr_backend::{
        config::resource_classes::ResourceClass,
        db::Db,
        library::Library,
        model::JobStatus,
        services::{local_tts::TtsConfig, tts_model_catalog::ModelCatalog},
        state::{AppState, ToolInfo},
    };
    let (state, _dir) = support::make_state(true, false).await;
    let chatterbox = std::env::var_os("VIDEO_KADR_TEST_TTS_CHATTERBOX").is_some();
    let runtime = if chatterbox {
        Some(video_kadr_backend::services::local_chatterbox::Runtime {
            executable: std::env::var("VIDEO_KADR_TEST_TTS_BIN").unwrap().into(),
            library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
                .unwrap()
                .into(),
        })
    } else {
        None
    };
    let voice = if chatterbox { 2099 } else { 1099 };

    let executable = std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_TTS_BIN").unwrap());
    let state = state.with_tts_config(TtsConfig {
        executable: executable.clone(),
        model_dir: None,
        threads: 2,
    });
    let state = state.with_chatterbox_runtime(runtime.clone());
    let catalog = ModelCatalog::find(if chatterbox {
        "3d500b0e333d1661a40e854cbdbc93d8f3e9f01942b6a4e1cea3f6b4aa8c9521"
    } else {
        "f85f04c7acf76154571a1e6b398534c257574cece490d2bb7991eae5e8426d54"
    })
    .unwrap()
    .unwrap();
    let source = std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_TTS_MODEL").unwrap());
    catalog.verify_directory(&source).unwrap();
    let installed = state.storage.join("models/tts").join(&catalog.id);
    for file in &catalog.files {
        let target = installed.join(&file.path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(source.join(&file.path), target).unwrap();
    }
    let owner = state
        .db
        .register_auth_user("clone-real-owner", "reference test password")
        .await
        .unwrap()
        .unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO tts_models(owner,id,name,bytes) VALUES(?,?,?,?)")
        .bind(&owner.user.username)
        .bind(&catalog.id)
        .bind(&catalog.name)
        .bind(catalog.bytes as i64)
        .execute(&pool)
        .await
        .unwrap();
    let app = support::router(state.clone());
    let selection = Request::builder()
        .method("PUT")
        .uri("/api/speech/tts-models/selection")
        .header("authorization", format!("Bearer {}", owner.token))
        .header("content-type", "application/json")
        .body(Body::from(json!({"modelId":catalog.id}).to_string()))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(selection).await.unwrap().status(),
        StatusCode::OK
    );
    let voice_sample = if chatterbox {
        std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_REFERENCE").unwrap())
    } else {
        source.join("test_wavs/bria.wav")
    };
    let source_mode = std::env::var_os("VIDEO_KADR_TEST_TTS_SOURCE_REFERENCE").is_some();
    let uploaded = if source_mode {
        use video_kadr_backend::library::MediaEntry;
        std::fs::copy(&voice_sample, state.sources_dir().join("public-bria.wav")).unwrap();
        assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"public-bria","filename":"public-bria.wav","url":"/files/sources/public-bria.wav","mediaType":"audio"}))).await);
        let request = Request::builder()
            .method("POST")
            .uri("/api/speech/tts-references/source")
            .header("authorization", format!("Bearer {}", owner.token))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"sourceId":"public-bria","startUs":0,"endUs":10000000}).to_string(),
            ))
            .unwrap();
        app.clone().oneshot(request).await.unwrap()
    } else {
        app.clone()
            .oneshot(request(
                "POST",
                "/api/speech/tts-references",
                Some(&owner.token),
                Body::from(std::fs::read(&voice_sample).unwrap()),
            ))
            .await
            .unwrap()
    };
    assert_eq!(uploaded.status(), StatusCode::CREATED);
    let uploaded = value(uploaded).await;
    assert_eq!(uploaded["durationUs"], 10_000_000);
    let reference = uploaded["id"].as_str().unwrap();
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
        .oneshot(job_text(
            &owner.token,
            Some(reference),
            voice,
            "The video editor creates a new audio track. Welcome to our project.",
        ))
        .await
        .unwrap();
    assert_eq!(admitted.status(), StatusCode::ACCEPTED);
    let admitted = value(admitted).await;
    let id = admitted["jobId"].as_str().unwrap();
    if chatterbox {
        let reset = Request::builder()
            .method("PUT")
            .uri("/api/speech/tts-models/selection")
            .header("authorization", format!("Bearer {}", owner.token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"modelId":"configured"}).to_string()))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(reset).await.unwrap().status(),
            StatusCode::OK
        );
    }
    let remove = Request::builder()
        .method("DELETE")
        .uri("/api/speech/tts-references")
        .header("authorization", format!("Bearer {}", owner.token))
        .header("content-type", "application/json")
        .body(Body::from(json!({"referenceId":reference}).to_string()))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(remove).await.unwrap().status(),
        StatusCode::OK
    );
    assert_eq!(
        app.oneshot(job(&owner.token, Some(reference), voice))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    state.begin_shutdown();
    drop(held);
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let root = state.storage.clone();
    let restored = AppState::new(
        root.clone(),
        2,
        ToolInfo::default(),
        Library::load(root.clone()).await,
        Db::open(&root).await.unwrap(),
    )
    .with_tts_config(TtsConfig {
        executable,
        model_dir: None,
        threads: 2,
    });
    let restored = restored.with_chatterbox_runtime(runtime);
    restored.recover_jobs().await;
    video_kadr_backend::handlers::resume_pending_jobs(&restored).await;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(if chatterbox { 300 } else { 120 }),
        async {
            loop {
                let j = restored.get_job(id).await.unwrap().unwrap();
                if matches!(j.status, JobStatus::Done | JobStatus::Error) {
                    break j;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        },
    )
    .await;
    restored.begin_shutdown();
    assert!(
        restored
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let result = result.unwrap();
    assert_eq!(result.status, JobStatus::Done, "{result:?}");
    let expected: String = sqlx::query_scalar("SELECT sha256 FROM tts_references WHERE id=?")
        .bind(reference)
        .fetch_one(&pool)
        .await
        .unwrap();
    let result = serde_json::to_value(result).unwrap();
    assert_eq!(result["result"]["referenceHash"], expected);
    assert_eq!(result["result"]["voiceId"], voice);
    assert_eq!(
        result["result"]["engine"],
        if chatterbox {
            "chatterbox-onnx"
        } else {
            "pocket-sherpa-onnx"
        }
    );
    let runtime = video_kadr_backend::process_control::ProcessRuntime::local_default();
    let whisper = video_kadr_backend::services::local_speech::SpeechConfig {
        executable: "/opt/homebrew/bin/whisper-cli".into(),
        model: Some(
            std::env::var("VIDEO_KADR_TEST_WHISPER_MODEL")
                .unwrap()
                .into(),
        ),
        threads: 2,
    };
    let duration = result["result"]["durationUs"].as_u64().unwrap();
    let transcript = video_kadr_backend::services::local_speech::transcribe(
        &runtime,
        &whisper,
        &root.join("tts-jobs").join(format!("{id}.wav")),
        0,
        duration,
        &root.join("clone-recognition-staging"),
        "en",
    )
    .await
    .unwrap();
    let recognized = transcript
        .segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    assert!(
        recognized.contains("video") && recognized.contains("project"),
        "custom narration must be intelligible: {recognized}"
    );
    let refreshed = support::router(restored.clone())
        .oneshot(request(
            "GET",
            "/api/speech/tts-references",
            Some(&owner.token),
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(refreshed.status(), StatusCode::OK);
    assert!(
        !root
            .join("tts-references")
            .join(format!("{reference}.wav"))
            .exists(),
        "finished job must release the removed private sample on refresh"
    );
    let audio = support::router(restored)
        .oneshot(request(
            "GET",
            &format!("/api/speech/tts-jobs/{id}/audio"),
            Some(&owner.token),
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(audio.status(), StatusCode::OK);
    assert!(
        to_bytes(audio.into_body(), 64 * 1024 * 1024)
            .await
            .unwrap()
            .len()
            > 44
    );
}

#[tokio::test]
async fn source_reference_clamps_to_media_end_and_preserves_space_authorization() {
    use video_kadr_backend::{db::SpaceRole, library::MediaEntry};
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("source-voice-owner", "reference test password")
        .await
        .unwrap()
        .unwrap();
    let viewer = state
        .db
        .register_auth_user("source-voice-viewer", "reference test password")
        .await
        .unwrap()
        .unwrap();
    let samples: Vec<f32> = (0..96000)
        .map(|i| if i < 48000 { 0.1 } else { 0.6 })
        .collect();
    let wav = video_kadr_backend::domain::filmcraft_wav_audio::write_wav16(&samples, 1, 24000);
    std::fs::write(state.sources_dir().join("voice.wav"), wav).unwrap();
    assert!(std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(state.sources_dir().join("voice.wav"))
        .args(["-c:a", "flac"])
        .arg(state.sources_dir().join("voice.flac"))
        .status()
        .unwrap()
        .success());
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"voice-source","filename":"voice.flac","url":"/files/sources/voice.flac","mediaType":"audio"}))).await);
    let space = state
        .db
        .create_space(&owner.user.username, "Voice source")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Voice",
            &json!({"schemaVersion":1,"sources":{"voice-source":{"id":"voice-source"}}}),
            &["voice-source".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let app = support::router(state.clone());
    let source_request = |token: Option<&str>, start: u64, end: u64| {
        let mut r = Request::builder()
            .method("POST")
            .uri("/api/speech/tts-references/source")
            .header("content-type", "application/json");
        if let Some(t) = token {
            r = r.header("authorization", format!("Bearer {t}"));
        }
        r.body(Body::from(
            json!({"sourceId":"voice-source","startUs":start,"endUs":end}).to_string(),
        ))
        .unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(source_request(None, 2000000, 12000000))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(source_request(Some(&viewer.token), 2000000, 12000000))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
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
    assert_eq!(
        app.clone()
            .oneshot(source_request(Some(&viewer.token), 3800000, 13800000))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let response = app
        .clone()
        .oneshot(source_request(Some(&viewer.token), 2000000, 12000000))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let result = value(response).await;
    assert_eq!(result["durationUs"], 2000000);
    let id = result["id"].as_str().unwrap();
    let bytes = std::fs::read(
        state
            .storage
            .join("tts-references")
            .join(format!("{id}.wav")),
    )
    .unwrap();
    assert_eq!(bytes.len(), 44 + 48000 * 2);
    let amplitude = i16::from_le_bytes([bytes[44], bytes[45]]) as f32 / 32768.;
    assert!((amplitude - 0.6).abs() < 0.001, "{amplitude}");
    state
        .db
        .remove_space_member(&space.id, &owner.user.username, &viewer.user.username)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        app.oneshot(job(&viewer.token, Some(id), 1099))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn reference_clip_plan_supports_freeze_and_keeps_transcription_freeze_refusal() {
    let (state, _dir) = support::make_state(true, false).await;
    let app = support::router(state);
    let seed: Value =
        serde_json::from_str(include_str!("fixtures/concat-merge-preview.json")).unwrap();
    let mut project = seed["createdTimeline"].clone();
    project["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["playbackMode"] =
        json!({"mode":"freeze","sourceTick":2500000});
    let body = json!({"project":project,"compositionId":"root","clipId":"picture"});
    let make = |path: &str| {
        Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let response = app
        .clone()
        .oneshot(make("/api/speech/reference-clip-plan"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result = value(response).await;
    assert_eq!(result["sourceId"], "source");
    assert_eq!(result["sourceStartUs"], 2500000);
    assert_eq!(
        app.oneshot(make("/api/speech/clip-plan"))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn saved_reference_library_is_owner_scoped_paginated_and_removal_blocks_new_work() {
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("sample-library-owner", "reference test password")
        .await
        .unwrap()
        .unwrap();
    let other = state
        .db
        .register_auth_user("sample-library-other", "reference test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let wav =
        video_kadr_backend::domain::filmcraft_wav_audio::write_wav16(&vec![0.2; 24000], 1, 24000);
    for _ in 0..3 {
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    "/api/speech/tts-references",
                    Some(&owner.token),
                    Body::from(wav.clone())
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::CREATED
        );
    }
    assert_eq!(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/speech/tts-references",
                None,
                Body::empty()
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = app
        .clone()
        .oneshot(request(
            "GET",
            "/api/speech/tts-references?limit=2",
            Some(&owner.token),
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let first = value(response).await;
    assert_eq!(first["references"].as_array().unwrap().len(), 2);
    let id = first["references"][0]["id"].as_str().unwrap();
    assert_eq!(first["references"][0]["durationUs"], 1000000);
    assert_eq!(first["references"][0]["available"], true);
    let cursor = first["nextCursor"].as_str().unwrap();
    let second = value(
        app.clone()
            .oneshot(request(
                "GET",
                &format!("/api/speech/tts-references?limit=2&cursor={cursor}"),
                Some(&owner.token),
                Body::empty(),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(second["references"].as_array().unwrap().len(), 1);
    assert!(second["nextCursor"].is_null());
    let foreign = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/speech/tts-references",
                Some(&other.token),
                Body::empty(),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(foreign["references"], json!([]));
    let remove = |token: &str| {
        Request::builder()
            .method("DELETE")
            .uri("/api/speech/tts-references")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(json!({"referenceId":id}).to_string()))
            .unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(remove(&other.token))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    std::fs::write(state.storage.join(".tts-job-key"), [42u8; 32]).unwrap();
    let qualification_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO job_requests(job_id,kind,payload_json,dedupe_key,created_at) VALUES('malformed-reference-pin','tts','broken','malformed-reference-pin',0)").execute(&qualification_pool).await.unwrap();
    let removed = app.clone().oneshot(remove(&owner.token)).await.unwrap();
    assert_eq!(removed.status(), StatusCode::OK);
    assert_eq!(value(removed).await["filesRetained"], true);
    assert_eq!(
        app.clone()
            .oneshot(remove(&owner.token))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(state
        .storage
        .join("tts-references")
        .join(format!("{id}.wav"))
        .exists());
    sqlx::query(
        "UPDATE job_requests SET payload_json='null' WHERE job_id='malformed-reference-pin'",
    )
    .execute(&qualification_pool)
    .await
    .unwrap();
    let listed = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/speech/tts-references",
                Some(&owner.token),
                Body::empty(),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(listed["references"].as_array().unwrap().len(), 2);
    assert!(!listed["references"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == id));
    assert!(!state
        .storage
        .join("tts-references")
        .join(format!("{id}.wav"))
        .exists());
    assert_eq!(
        app.oneshot(job(&owner.token, Some(id), 1099))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
}
