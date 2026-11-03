mod support;
use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use serde_json::{json, Value};
use std::io::Cursor;
use tower::ServiceExt;
use video_kadr_backend::{
    domain::{
        composition::{CanvasSpec, Composition, SourceId},
        nested_composition::NestedCompositionProject,
    },
    library::MediaEntry,
    template_bundle::{self, Input},
};
fn multipart(bytes: &[u8]) -> Vec<u8> {
    let mut body=b"--TEMPLATE\r\nContent-Disposition: form-data; name=\"file\"; filename=\"edit.vktemplate\"\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec();
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n--TEMPLATE--\r\n");
    body
}
fn request(bytes: &[u8], token: &str, fills: Value) -> Request<Body> {
    let encoded = fills
        .to_string()
        .bytes()
        .map(|b| format!("%{b:02X}"))
        .collect::<String>();
    Request::post(format!(
        "/api/composition-projects/template-import?fills={encoded}"
    ))
    .header(header::AUTHORIZATION, format!("Bearer {token}"))
    .header(
        header::CONTENT_TYPE,
        "multipart/form-data; boundary=TEMPLATE",
    )
    .body(Body::from(multipart(bytes)))
    .unwrap()
}
#[tokio::test]
async fn imports_template_to_independent_project_and_rolls_back_design_publication_on_save_failure()
{
    let (state, _dir) = support::make_state(true, false).await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(state.storage.join("app.db"))
                .foreign_keys(true),
        )
        .await
        .unwrap();
    let owner = state
        .db
        .register_auth_user("template-importer", "template import password")
        .await
        .unwrap()
        .unwrap();
    let replacement = state.sources_dir().join("replacement.png");
    image::RgbaImage::from_pixel(16, 8, image::Rgba([200, 20, 50, 255]))
        .save(&replacement)
        .unwrap();
    assert!(
        state
            .library
            .add(MediaEntry {
                id: "replacement".into(),
                kind: "source".into(),
                filename: "replacement.png".into(),
                storage_key: None,
                url: "/files/sources/replacement.png".into(),
                title: Some("Chosen picture".into()),
                original_filename: None,
                media_type: Some("image".into()),
                duration: None,
                width: Some(16),
                height: Some(8),
                fps: None,
                vcodec: None,
                acodec: None,
                size_bytes: Some(std::fs::metadata(&replacement).unwrap().len()),
                created_at: 1
            })
            .await
    );
    let replacement_bytes = std::fs::read(&replacement).unwrap();
    let design_path = state.staging_dir().join("design.png");
    image::RgbaImage::from_pixel(16, 8, image::Rgba([20, 90, 10, 255]))
        .save(&design_path)
        .unwrap();
    let design_bytes = std::fs::read(&design_path).unwrap();
    let source = |id: &str| json!({"id":id,"kind":"image","durationTicks":0,"width":16,"height":8,"hasAudio":false});
    let mut content = serde_json::to_value(Composition::new(CanvasSpec::default())).unwrap();
    content["sources"] = json!({"slot":source("slot"),"design":source("design")});
    let project:NestedCompositionProject=serde_json::from_value(json!({"schemaVersion":1,"root":"root","projectMedia":{"slot":{"source":source("slot"),"name":"Replace","placeholder":true,"librarySourceId":"author-private"},"design":{"source":source("design"),"name":"Design","librarySourceId":"author-design"}},"compositions":{"root":{"content":content,"instances":[]}}})).unwrap();
    let bytes = template_bundle::write(
        Cursor::new(Vec::new()),
        "Fresh edit",
        &project,
        &[Input {
            source_id: SourceId::parse("author-design").unwrap(),
            path: design_path.clone(),
        }],
    )
    .unwrap()
    .into_inner();
    std::fs::remove_file(design_path).unwrap();
    let app = support::router(state.clone());
    let fills = json!([{"mediaId":"slot","sourceId":"replacement"}]);
    assert_eq!(
        app.clone()
            .oneshot(request(&bytes, &owner.token, json!([])))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                &bytes,
                &owner.token,
                json!([{"mediaId":"slot","sourceId":"missing"}])
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    assert_eq!(state.library.list().await.len(), 1);
    sqlx::query("CREATE TRIGGER reject_template_project BEFORE INSERT ON composition_projects BEGIN SELECT RAISE(ABORT,'controlled template save failure'); END").execute(&pool).await.unwrap();
    let failed = app
        .clone()
        .oneshot(request(&bytes, &owner.token, fills.clone()))
        .await
        .unwrap();
    assert_eq!(failed.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(state.library.list().await.len(), 1);
    assert_eq!(std::fs::read_dir(state.sources_dir()).unwrap().count(), 1);
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM composition_projects")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("DROP TRIGGER reject_template_project")
        .execute(&pool)
        .await
        .unwrap();
    let response = app
        .oneshot(request(&bytes, &owner.token, fills))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(body["project"]["name"], "Fresh edit");
    assert_eq!(
        body["project"]["document"]["projectMedia"]["slot"]["librarySourceId"],
        "replacement"
    );
    assert_ne!(
        body["project"]["document"]["projectMedia"]["slot"]["placeholder"],
        true
    );
    let fresh = body["project"]["document"]["projectMedia"]["design"]["librarySourceId"]
        .as_str()
        .unwrap();
    assert_ne!(fresh, "author-design");
    assert_ne!(fresh, "template-asset-0000");
    assert!(state.library.get(fresh).await.is_some());
    assert_eq!(state.library.list().await.len(), 2);
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    assert_eq!(std::fs::read(&replacement).unwrap(), replacement_bytes);
    let imported = state.library.get(fresh).await.unwrap();
    let imported_path = state.library.resolve_media_path(&imported).await.unwrap();
    assert_eq!(std::fs::read(imported_path).unwrap(), design_bytes);
}
