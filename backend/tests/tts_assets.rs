mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
fn req(method: &str, path: &str, token: Option<&str>, bytes: Vec<u8>) -> Request<Body> {
    let mut r = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "audio/wav");
    if let Some(token) = token {
        r = r.header("authorization", format!("Bearer {token}"));
    }
    r.body(Body::from(bytes)).unwrap()
}
fn wav() -> Vec<u8> {
    let mut b = vec![];
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&48036u32.to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24000u32.to_le_bytes());
    b.extend_from_slice(&48000u32.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&48000u32.to_le_bytes());
    for _ in 0..24000 {
        b.extend_from_slice(&1000i16.to_le_bytes());
    }
    b
}
async fn body(r: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(r.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}
#[tokio::test]
async fn saved_narration_is_private_durable_and_admitted_by_its_project() {
    let (mut state, _dir) = support::make_state(true, false).await;
    let tools = std::sync::Arc::make_mut(&mut state.tools);
    tools
        .ffmpeg_encoders
        .extend(["ffv1".into(), "pcm_f32le".into()]);
    tools.ffmpeg_muxers.push("matroska".into());
    let owner = state
        .db
        .register_auth_user("narration-owner", "narration test password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("narration-other", "narration test password")
        .await
        .unwrap()
        .unwrap();
    let doc = json!({"schemaVersion":1,"sources":{}});
    let project = state
        .db
        .create_owned_composition_project("Narration", &doc, &[], &owner.user.username)
        .await
        .unwrap();
    let app = support::router(state.clone());
    let path = format!("/api/speech/narration-assets?projectId={}", project.id);
    assert_eq!(
        app.clone()
            .oneshot(req("PUT", &path, None, wav()))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(req("PUT", &path, Some(&outsider.token), wav()))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.clone()
            .oneshot(req("PUT", &path, Some(&owner.token), b"bad wav".to_vec()))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let response = app
        .clone()
        .oneshot(req("PUT", &path, Some(&owner.token), wav()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let asset = body(response).await;
    let id = asset["id"].as_str().unwrap();
    let url = asset["url"].as_str().unwrap();
    assert_eq!(asset["duration"], 1.0);
    assert!(asset["storageKey"].as_str().unwrap().starts_with("spaces/"));
    assert!(!state
        .sources_dir()
        .join(asset["filename"].as_str().unwrap())
        .exists());
    assert_eq!(
        app.clone()
            .oneshot(req("GET", url, None, vec![]))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(req("GET", url, Some(&outsider.token), vec![]))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = app
        .clone()
        .oneshot(req("GET", url, Some(&owner.token), vec![]))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), 100000)
            .await
            .unwrap()
            .as_ref(),
        wav()
    );
    let public = body(
        app.clone()
            .oneshot(req("GET", "/api/library", None, vec![]))
            .await
            .unwrap(),
    )
    .await;
    assert!(!public.as_array().unwrap().iter().any(|e| e["id"] == id));
    let own = body(
        app.clone()
            .oneshot(req("GET", "/api/library", Some(&owner.token), vec![]))
            .await
            .unwrap(),
    )
    .await;
    assert!(own.as_array().unwrap().iter().any(|e| e["id"] == id));
    use video_kadr_backend::domain::project_collaboration::ProjectRole;
    state
        .db
        .set_review_member(
            &project.id,
            &owner.user.username,
            &outsider.user.username,
            ProjectRole::Commenter,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(req("GET", url, Some(&outsider.token), vec![]))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.clone()
            .oneshot(req("PUT", &path, Some(&outsider.token), wav()))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    state
        .db
        .set_review_member(
            &project.id,
            &owner.user.username,
            &outsider.user.username,
            ProjectRole::Editor,
        )
        .await
        .unwrap()
        .unwrap();
    let second = app
        .clone()
        .oneshot(req("PUT", &path, Some(&outsider.token), wav()))
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::OK);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    sqlx::query("DELETE FROM composition_project_members WHERE project_id=? AND actor=?")
        .bind(&project.id)
        .bind(&outsider.user.username)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(req("GET", url, Some(&outsider.token), vec![]))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let mut changed = doc.clone();
    changed["sources"][id] = json!({"id":id});
    let saved = state
        .db
        .update_composition_project_for(
            &project.id,
            &owner.user.username,
            "Narration",
            &changed,
            &[id.to_string()],
            project.revision,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.source_ids, [id]);
    let other = state
        .db
        .create_owned_composition_project("Other", &doc, &[], &outsider.user.username)
        .await
        .unwrap();
    assert!(state
        .db
        .update_composition_project_for(
            &other.id,
            &outsider.user.username,
            "Other",
            &changed,
            &[id.to_string()],
            other.revision
        )
        .await
        .is_err());
    let cold = video_kadr_backend::state::AppState::new_with_runtime(
        state.storage.clone(),
        2,
        (*state.tools).clone(),
        video_kadr_backend::library::Library::load(state.storage.clone()).await,
        video_kadr_backend::db::Db::open(&state.storage)
            .await
            .unwrap(),
        state.encode_budget.clone(),
        2,
    )
    .unwrap();
    assert_eq!(
        support::router(cold)
            .oneshot(req("GET", url, Some(&owner.token), vec![]))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        state
            .db
            .composition_source_reference_count(id)
            .await
            .unwrap(),
        1
    );
    let mut fixture: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/concat-merge-preview.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut node = fixture["createdTimeline"]["compositions"]["new-timeline"].take();
    node["content"]["sources"][id] = json!({"id":id,"kind":"audio","durationTicks":1000000,"width":0,"height":0,"hasAudio":true});
    node["content"]["tracks"][2]["clips"] = json!([{"id":"narration-clip","sourceId":id,"placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1},"gain":{"mode":"constant","value":1},"pan":{"mode":"constant","value":0},"reversed":false,"fadeInTicks":0,"fadeOutTicks":0,"voiceEffect":"none","pitchSemitones":0,"toneDb":0,"crossfadeCurve":"constant_gain","crossfadeInTicks":0,"enabled":true}]);
    let native = json!({"schemaVersion":1,"root":"timeline","compositions":{"timeline":node}});
    let current = state
        .db
        .get_composition_project_for(&project.id, &owner.user.username)
        .await
        .unwrap()
        .unwrap();
    state
        .db
        .update_composition_project_for(
            &project.id,
            &owner.user.username,
            "Narration",
            &native,
            &[id.to_string()],
            current.revision,
        )
        .await
        .unwrap()
        .unwrap();
    let export = json!({"schemaVersion":1,"mode":"video_audio","project":native,"compositionId":"timeline","startUs":0,"endUs":1000000});
    let post = |token: &str| {
        Request::builder()
            .method("POST")
            .uri("/api/compositions/nested-export")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(export.to_string()))
            .unwrap()
    };
    let app = support::router(state.clone());
    assert_eq!(
        app.clone()
            .oneshot(post(&outsider.token))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = app.oneshot(post(&owner.token)).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let admitted = body(response).await;
    let job_id = admitted["jobId"].as_str().unwrap();
    let job = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let job = state.get_job(job_id).await.unwrap().unwrap();
            if matches!(
                job.status,
                video_kadr_backend::model::JobStatus::Done
                    | video_kadr_backend::model::JobStatus::Error
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        job.status,
        video_kadr_backend::model::JobStatus::Done,
        "{job:?}"
    );
    let output = state
        .outputs_dir()
        .join(job.result.unwrap()["filename"].as_str().unwrap());
    let decoded = tokio::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(output)
        .args(["-map", "0:a:0", "-t", "0.1", "-f", "f32le", "-"])
        .output()
        .await
        .unwrap();
    assert!(decoded.status.success());
    assert!(decoded.stdout.len() > 8);
    let sample = f32::from_le_bytes(decoded.stdout[..4].try_into().unwrap());
    assert!(
        (sample - (1000. / 32768.) * std::f32::consts::FRAC_1_SQRT_2).abs() < 0.001,
        "narration must reach actual encoded audio: {sample}"
    );
    state
        .db
        .set_review_member(
            &project.id,
            &owner.user.username,
            &outsider.user.username,
            ProjectRole::Viewer,
        )
        .await
        .unwrap()
        .unwrap();
    assert!(state
        .db
        .delete_composition_project_for(&project.id, &owner.user.username)
        .await
        .unwrap());
    let app = support::router(state.clone());
    assert_eq!(
        app.clone()
            .oneshot(req("GET", url, Some(&outsider.token), vec![]))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.oneshot(req("GET", url, Some(&owner.token), vec![]))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
