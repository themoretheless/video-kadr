mod support;
use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;
use video_kadr_backend::{
    domain::composition::{CanvasSpec, Composition},
    library::MediaEntry,
};
#[tokio::test]
async fn saved_native_template_download_excludes_missing_slot_files_and_cleans_streams() {
    let (state, dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("template-owner", "template test password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("template-outsider", "template test password")
        .await
        .unwrap()
        .unwrap();
    let path = state.sources_dir().join("design.png");
    image::RgbaImage::from_pixel(16, 8, image::Rgba([25, 50, 75, 255]))
        .save(&path)
        .unwrap();
    assert!(
        state
            .library
            .add(MediaEntry {
                id: "design-file".into(),
                kind: "source".into(),
                filename: "design.png".into(),
                storage_key: None,
                url: "/files/sources/design.png".into(),
                title: None,
                original_filename: None,
                media_type: Some("image".into()),
                duration: None,
                width: Some(16),
                height: Some(8),
                fps: None,
                vcodec: None,
                acodec: None,
                size_bytes: Some(std::fs::metadata(&path).unwrap().len()),
                created_at: 1
            })
            .await
    );
    let source = |id: &str| json!({"id":id,"kind":"image","durationTicks":0,"width":16,"height":8,"hasAudio":false});
    let mut content = serde_json::to_value(Composition::new(CanvasSpec::default())).unwrap();
    content["canvas"]["width"] = json!(16);
    content["canvas"]["height"] = json!(8);
    content["tracks"] = json!([{"id":"pictures","kind":"image","name":"Pictures","clips":[{"id":"stand-in","sourceId":"slot","timelineStartTick":0,"durationTicks":2000000,"transform":video_kadr_backend::domain::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal"}]}]);
    content["sources"] = json!({"slot":source("slot"),"design":source("design")});
    let document = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":content,"instances":[]}},"projectMedia":{"slot":{"source":source("slot"),"librarySourceId":"missing-original","name":"Replace","placeholder":true},"design":{"source":source("design"),"librarySourceId":"design-file","name":"Design"}}});
    let saved = state
        .db
        .create_owned_composition_project(
            "Reusable edit",
            &document,
            &["design-file".into(), "missing-original".into()],
            &owner.user.username,
        )
        .await
        .unwrap();
    let app = support::router(state.clone());
    let uri = format!("/api/composition-projects/{}/template", saved.id);
    let request = |token: Option<&str>| {
        let mut r = Request::get(&uri);
        if let Some(token) = token {
            r = r.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        r.body(Body::empty()).unwrap()
    };
    assert_eq!(
        app.clone().oneshot(request(None)).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(request(Some(&outsider.token)))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let response = app
        .clone()
        .oneshot(request(Some(&owner.token)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/vnd.video-kadr.template"
    );
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "private, no-store"
    );
    assert!(response.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .contains(".vktemplate"));
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    let stage = dir.path().join("parsed-template");
    std::fs::create_dir(&stage).unwrap();
    let parsed =
        video_kadr_backend::template_bundle::read(std::io::Cursor::new(bytes), &stage).unwrap();
    assert_eq!(parsed.assets.len(), 1);
    assert_eq!(
        std::fs::read(parsed.assets[0].path()).unwrap(),
        std::fs::read(&path).unwrap()
    );
    assert!(parsed.manifest.project.project_media
        [&video_kadr_backend::domain::composition::SourceId::parse("slot").unwrap()]
        .library_source_id
        .is_none());
    drop(parsed);
    let response = app
        .clone()
        .oneshot(
            Request::get(&uri)
                .header(
                    header::COOKIE,
                    format!("video_kadr_session={}", owner.token),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    drop(response);
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    let poster_uri = format!("{uri}?posterTimeUs=500000&posterCompositionId=root");
    let poster_request = || {
        Request::get(&poster_uri)
            .header(
                header::COOKIE,
                format!("video_kadr_session={}", owner.token),
            )
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(poster_request())
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let original_path = state.sources_dir().join("original.png");
    image::RgbaImage::from_pixel(16, 8, image::Rgba([12, 90, 160, 255]))
        .save(&original_path)
        .unwrap();
    let mut original_entry = state.library.get("design-file").await.unwrap();
    original_entry.id = "missing-original".into();
    original_entry.filename = "original.png".into();
    original_entry.url = "/files/sources/original.png".into();
    original_entry.size_bytes = Some(std::fs::metadata(&original_path).unwrap().len());
    assert!(state.library.add(original_entry).await);
    let response = app.clone().oneshot(poster_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let parsed =
        video_kadr_backend::template_bundle::read(std::io::Cursor::new(bytes), &stage).unwrap();
    assert_eq!(parsed.assets.len(), 1);
    let png = parsed.poster_png.as_ref().unwrap();
    let image = image::load_from_memory(png).unwrap().to_rgba8();
    assert!(image.width() <= 320 && image.height() <= 180);
    assert_eq!(
        image.get_pixel(image.width() / 2, image.height() / 2).0,
        [12, 90, 160, 255]
    );
    drop(parsed);
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    let save_request = || {
        Request::post(&poster_uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
            .body(Body::empty())
            .unwrap()
    };
    let response = app.clone().oneshot(save_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let saved_template: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(saved_template["hasPoster"], true);
    let download = Request::get(format!(
        "/api/template-library/{}",
        saved_template["id"].as_str().unwrap()
    ))
    .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(download).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let parsed =
        video_kadr_backend::template_bundle::read(std::io::Cursor::new(bytes), &stage).unwrap();
    assert!(parsed.poster_png.is_some());
    drop(parsed);
    assert_eq!(
        app.clone().oneshot(save_request()).await.unwrap().status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        std::fs::read_dir(state.storage.join("template-library"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        app.oneshot(request(Some(&owner.token)))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    assert_eq!(
        state
            .db
            .get_composition_project_for(&saved.id, &owner.user.username)
            .await
            .unwrap()
            .unwrap()
            .document,
        document
    );
}
