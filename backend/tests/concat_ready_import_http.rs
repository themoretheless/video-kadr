mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
fn request(token: Option<&str>, body: Value) -> Request<Body> {
    let mut r = Request::builder()
        .method("POST")
        .uri("/api/compositions/concat-project/import-ready")
        .header("content-type", "application/json");
    if let Some(token) = token {
        r = r.header("authorization", format!("Bearer {token}"));
    }
    r.body(Body::from(body.to_string())).unwrap()
}
#[tokio::test]
async fn saved_ready_import_requires_identity_project_access_and_server_owned_references() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("owner", "synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("outsider", "synthetic password")
        .await
        .unwrap()
        .unwrap();
    let space = state.db.create_space("owner", "Private").await.unwrap();
    let document = serde_json::to_value(video_kadr_backend::domain::composition::Composition::new(
        Default::default(),
    ))
    .unwrap();
    let project = state
        .db
        .create_space_composition_project("Source", &document, &[], "owner", &space.id)
        .await
        .unwrap();
    let body = json!({"request":{"schemaVersion":1,"document":{"version":1,"media":[],"timelines":[{"id":"main","clips":[{"id":"clip","kind":"image","cutout":{"mode":"auto","feather":0}}]}]},"mediaBindings":{},"preserveMediaIds":true},"preparedStills":[{"timelineId":"main","clipId":"clip","projectId":project.id,"compositionId":null,"sourceClipId":"missing","expectedRevision":project.revision}]});
    let app = support::router(state.clone());
    assert_eq!(
        app.clone()
            .oneshot(request(None, body.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(request(Some(&outsider.token), body.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.clone()
            .oneshot(request(Some(&owner.token), body.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let mut empty = body.clone();
    empty["preparedStills"] = json!([]);
    assert_eq!(
        app.clone()
            .oneshot(request(Some(&owner.token), empty))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let mut injected = body;
    injected["preparedStills"][0]["maskPath"] = json!("/tmp/untrusted.png");
    assert_eq!(
        app.oneshot(request(Some(&owner.token), injected))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert!(state.library.list().await.is_empty());
    assert_eq!(
        state
            .db
            .get_composition_project_for(&project.id, "owner")
            .await
            .unwrap()
            .unwrap()
            .revision,
        project.revision
    );
}

#[tokio::test]
async fn saved_ready_import_keeps_its_body_budget_before_file_verification() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let limit = 4 * 1024 * 1024
        + video_kadr_backend::domain::concat_project_fonts::MAX_FONT_JSON_BYTES
        + 1024 * 1024;
    let request = Request::builder()
        .method("POST")
        .uri("/api/compositions/concat-project/import-ready")
        .header("content-type", "application/json")
        .body(Body::from(vec![b' '; limit + 1]))
        .unwrap();
    assert_eq!(
        app.oneshot(request).await.unwrap().status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[tokio::test]
async fn rebuild_import_reserves_body_space_for_escaped_subtitle_documents() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("rebuild-owner", "synthetic password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state);
    let text = format!(
        "1\n00:00:00,000 --> 00:00:01,000\nDialogue{}",
        "\u{1}".repeat(
            video_kadr_backend::domain::metadata_burnin::MAX_SUBTITLE_DOCUMENT_TEXT_BYTES - 128,
        )
    );
    let payload = json!({
        "request":{"schemaVersion":1,"document":{"version":1,"media":[],"tracks":[],"clips":[],
            "name":"a".repeat(4 * 1024 * 1024 - 1024)},"mediaBindings":{}},
        "subtitleDocuments":[{"id":"dialogue","name":"dialogue.srt","text":text}],
    })
    .to_string();
    let previous_budget = 4 * 1024 * 1024
        + video_kadr_backend::domain::concat_project_fonts::MAX_FONT_JSON_BYTES
        + 1024 * 1024;
    assert!(payload.len() > previous_budget);
    let request = Request::post("/api/compositions/concat-project/import-rebuild")
        .header("authorization", format!("Bearer {}", owner.token))
        .header("content-type", "application/json")
        .body(Body::from(payload))
        .unwrap();
    let (status, response, _) = support::send(&app, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    assert_eq!(
        response["error"],
        "В проекте Concat нет масок для восстановления"
    );
}

#[tokio::test]
async fn rebuild_import_rejects_unsupported_timeline_before_analysing_an_image() {
    use video_kadr_backend::library::MediaEntry;
    let (state, _dir) = support::make_state(true, false).await;
    let state = state.with_person_config(None).with_selfie_config(None);
    let owner = state
        .db
        .register_auth_user("preflight-owner", "synthetic password")
        .await
        .unwrap()
        .unwrap();
    image::RgbaImage::new(16, 16)
        .save(state.sources_dir().join("image.png"))
        .unwrap();
    assert!(state.library.add(MediaEntry::from_result("source", &json!({
        "id":"image","filename":"image.png","url":"/files/sources/image.png","mediaType":"image",
    }))).await);
    let request = Request::post("/api/compositions/concat-project/import-rebuild")
        .header("authorization", format!("Bearer {}", owner.token))
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "rebuildStills":true,
            "request":{"schemaVersion":1,"preserveMediaIds":true,
                "mediaBindings":{"portrait":{"id":"image","kind":"image","durationTicks":0,
                    "width":16,"height":16,"hasAudio":false}},
                "document":{"version":1,"media":[{"id":"portrait","kind":"image"}],
                    "timelines":[
                        {"id":"main","tracks":[{"id":"images"}],"clips":[{"id":"cut","trackId":"images",
                            "mediaId":"portrait","kind":"image","duration":1,
                            "cutout":{"mode":"auto","subject":"person"}}]},
                        {"id":"later","tracks":[],"clips":[],"unsupportedSetting":true}
                    ]}
            }
        }).to_string())).unwrap();
    let app = support::router(state.clone());
    let (status, body, _) = support::send(&app, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("unsupportedSetting"));
    assert!(!state.storage.join("concat-analysis").exists());
    assert_eq!(state.library.list().await.len(), 1);
}
