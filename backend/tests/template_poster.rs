mod support;
use serde_json::{json, Value};
use std::io::{Cursor, Write};
use video_kadr_backend::{
    domain::{
        composition::{CanvasSpec, Composition},
        nested_composition::NestedCompositionProject,
    },
    template_bundle,
};
fn project() -> NestedCompositionProject {
    serde_json::from_value(json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":Composition::new(CanvasSpec::default()),"instances":[]}}})).unwrap()
}
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        width,
        height,
        image::Rgba([11, 25, 67, 255]),
    ))
    .write_to(&mut out, image::ImageFormat::Png)
    .unwrap();
    out.into_inner()
}
fn package(poster: &[u8]) -> Vec<u8> {
    template_bundle::write_with_poster(
        Cursor::new(Vec::new()),
        "Poster design",
        &project(),
        &[],
        Some(poster),
    )
    .unwrap()
    .into_inner()
}
#[test]
fn png_poster_roundtrips_pixels_and_rejects_untrusted_format_dimensions_and_digest() {
    let dir = tempfile::tempdir().unwrap();
    let image = png(320, 180);
    let bytes = package(&image);
    let parsed = template_bundle::read(Cursor::new(&bytes), dir.path()).unwrap();
    assert_eq!(parsed.poster_png.as_ref().unwrap(), &image);
    let descriptor = parsed.manifest.poster.as_ref().unwrap();
    assert_eq!((descriptor.width, descriptor.height), (320, 180));
    assert_eq!(
        image::load_from_memory(parsed.poster_png.as_ref().unwrap())
            .unwrap()
            .to_rgba8()
            .get_pixel(0, 0)
            .0,
        [11, 25, 67, 255]
    );
    drop(parsed);
    for invalid in [
        b"<svg/>".to_vec(),
        png(1025, 1),
        vec![0; template_bundle::MAX_POSTER_BYTES + 1],
    ] {
        assert!(template_bundle::write_with_poster(
            Cursor::new(Vec::new()),
            "Invalid",
            &project(),
            &[],
            Some(&invalid)
        )
        .is_err());
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut manifest: Value = serde_json::from_reader(zip.by_index(0).unwrap()).unwrap();
    manifest["poster"]["sha256"] = json!("0".repeat(64));
    let mut changed = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    changed.start_file("template.json", options).unwrap();
    changed
        .write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    changed.start_file("poster.png", options).unwrap();
    changed.write_all(&image).unwrap();
    assert!(template_bundle::read(
        Cursor::new(changed.finish().unwrap().into_inner()),
        dir.path()
    )
    .is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
#[tokio::test]
async fn private_poster_is_persisted_and_served_only_to_its_owner_without_changing_package_bytes() {
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request, StatusCode},
    };
    use tower::ServiceExt;
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("poster-owner", "poster test password")
        .await
        .unwrap()
        .unwrap();
    let other = state
        .db
        .register_auth_user("poster-other", "poster test password")
        .await
        .unwrap()
        .unwrap();
    let poster = png(320, 180);
    let bytes = package(&poster);
    let mut multipart=b"--POSTER\r\nContent-Disposition: form-data; name=\"file\"; filename=\"design.vktemplate\"\r\n\r\n".to_vec();
    multipart.extend_from_slice(&bytes);
    multipart.extend_from_slice(b"\r\n--POSTER--\r\n");
    let app = support::router(state.clone());
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/template-library")
                .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
                .header(header::CONTENT_TYPE, "multipart/form-data; boundary=POSTER")
                .body(Body::from(multipart))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let entry: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(entry["hasPoster"], true);
    let id = entry["id"].as_str().unwrap();
    let request = |token: &str, path: String| {
        Request::get(path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(request(
                &other.token,
                format!("/api/template-library/{id}/poster")
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let mut restarted = state.clone();
    restarted.db = video_kadr_backend::db::Db::open(&state.storage)
        .await
        .unwrap()
        .into();
    let reopened = support::router(restarted);
    let response = reopened
        .clone()
        .oneshot(request(
            &owner.token,
            format!("/api/template-library/{id}/poster"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(
        response.headers()[header::X_CONTENT_TYPE_OPTIONS],
        "nosniff"
    );
    assert_eq!(
        to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .as_ref(),
        poster.as_slice()
    );
    let response = reopened
        .oneshot(request(&owner.token, format!("/api/template-library/{id}")))
        .await
        .unwrap();
    assert_eq!(
        to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .as_ref(),
        bytes.as_slice()
    );
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
}
