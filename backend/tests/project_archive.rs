//! HTTP round-trip coverage for portable `.veproj` composition archives.

mod support;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use video_kadr_backend::library::MediaEntry;
use video_kadr_backend::project_archive::ARCHIVE_MEDIA_TYPE;
use video_kadr_backend::tools;

use support::{assert_api_error, make_state, router};

const BOUNDARY: &str = "VEPROJ_API_BOUNDARY";

fn tiny_wav() -> Vec<u8> {
    let sample_rate = 8_000_u32;
    let samples = vec![128_u8; 800];
    let mut wav = Vec::with_capacity(44 + samples.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36_u32 + samples.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&8_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    wav.extend_from_slice(&samples);
    wav
}

fn multipart_archive(bytes: &[u8]) -> Request<Body> {
    let mut body = Vec::with_capacity(bytes.len() + 256);
    body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"portable.veproj\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: application/vnd.video-editor.project\r\n\r\n");
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    Request::builder()
        .method("POST")
        .uri("/api/composition-projects/import")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(body))
        .unwrap()
}

fn document(source_id: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "timeBase": 1_000_000,
        "sources": {
            (source_id): {
                "id": source_id,
                "kind": "audio",
                "durationTicks": 100_000,
                "width": 0,
                "height": 0,
                "hasAudio": true
            }
        },
        "tracks": [{
            "kind": "audio",
            "id": "dialogue",
            "name": "Dialogue",
            "clips": [{
                "id": "voice-clip",
                "sourceId": source_id,
                "timelineStartTicks": 0,
                "sourceInTicks": 0,
                "sourceOutTicks": 100_000
            }]
        }]
    })
}

async fn local_probe_available(state: &video_kadr_backend::state::AppState) -> bool {
    tools::check_tool(&state.process_runtime, "ffmpeg", "-version")
        .await
        .0
        && tools::check_tool(&state.process_runtime, "ffprobe", "-version")
            .await
            .0
}

#[tokio::test]
async fn composition_project_archive_http_round_trip_relinks_sources_and_metadata() {
    let (state, directory) = make_state(true, false).await;
    if !local_probe_available(&state).await {
        eprintln!("skipping .veproj HTTP roundtrip: ffmpeg/ffprobe not on PATH");
        return;
    }
    let old_id = "portable-source";
    let old_filename = "portable-source.wav";
    let media = tiny_wav();
    tokio::fs::write(state.sources_dir().join(old_filename), &media)
        .await
        .unwrap();
    assert!(
        state
            .library
            .add(MediaEntry {
                id: old_id.into(),
                kind: "source".into(),
                filename: old_filename.into(),
                url: format!("/files/sources/{old_filename}"),
                media_type: Some("audio".into()),
                title: Some("Imported voice".into()),
                duration: Some(0.1),
                width: None,
                height: None,
                fps: None,
                vcodec: None,
                acodec: Some("pcm_s16le".into()),
                size_bytes: Some(media.len() as u64),
                created_at: 1,
            })
            .await
    );
    state
        .db
        .replace_library_metadata(
            old_id,
            Some("Portable dialogue".into()),
            true,
            vec!["dialogue".into(), "approved".into()],
        )
        .await
        .unwrap();
    let saved = state
        .db
        .create_composition_project("Portable edit", &document(old_id), &[old_id.into()])
        .await
        .unwrap();
    let app = router(state.clone());

    let export = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/composition-projects/{}/archive", saved.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(export.status(), StatusCode::OK);
    assert_eq!(export.headers()[header::CONTENT_TYPE], ARCHIVE_MEDIA_TYPE);
    assert_eq!(
        export.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"portable-edit.veproj\""
    );
    let archive = to_bytes(export.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(&archive[..8], b"VEPROJ\r\n");
    let archive_text = String::from_utf8_lossy(&archive);
    assert!(!archive_text.contains(directory.path().to_string_lossy().as_ref()));
    assert!(!archive_text.contains("/files/sources"));
    assert!(!archive_text.contains(&saved.id));

    let imported = app
        .clone()
        .oneshot(multipart_archive(&archive))
        .await
        .unwrap();
    assert_eq!(imported.status(), StatusCode::CREATED);
    let body: Value = serde_json::from_slice(
        &to_bytes(imported.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    let new_id = body["sourceMapping"][old_id].as_str().unwrap();
    assert_ne!(new_id, old_id);
    assert_eq!(body["project"]["name"], "Portable edit");
    assert_eq!(body["project"]["sourceIds"], json!([new_id]));
    assert!(body["project"]["document"]["sources"].get(new_id).is_some());
    assert_eq!(body["project"]["document"]["sources"][new_id]["id"], new_id);
    assert_eq!(
        body["project"]["document"]["tracks"][0]["clips"][0]["sourceId"],
        new_id
    );

    let imported_entry = state.library.get(new_id).await.unwrap();
    assert_eq!(imported_entry.media_type.as_deref(), Some("audio"));
    assert_eq!(
        tokio::fs::read(state.sources_dir().join(&imported_entry.filename))
            .await
            .unwrap(),
        media
    );
    let metadata = state
        .db
        .get_library_metadata(new_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata.title.as_deref(), Some("Portable dialogue"));
    assert!(metadata.favorite);
    assert_eq!(metadata.tags, ["dialogue", "approved"]);
    assert!(tokio::fs::read_dir(state.staging_dir())
        .await
        .unwrap()
        .next_entry()
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn project_archive_import_rejects_invalid_magic_before_probe_or_publish() {
    let (state, _directory) = make_state(true, false).await;
    let app = router(state.clone());
    let response = app
        .oneshot(multipart_archive(b"not-a-veproj"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_api_error(&body, "bad_request");
    assert!(state.library.list().await.is_empty());
    assert!(state
        .db
        .list_composition_projects()
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn project_archive_routes_are_cors_preflight_enabled() {
    let (state, _directory) = make_state(false, false).await;
    let app = router(state);
    for (uri, method) in [
        ("/api/composition-projects/import", "POST"),
        ("/api/composition-projects/project-id/archive", "GET"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri(uri)
                    .header(header::ORIGIN, "http://localhost:5173")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, method)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "http://localhost:5173"
        );
        assert!(response.headers()[header::ACCESS_CONTROL_ALLOW_METHODS]
            .to_str()
            .unwrap()
            .split(',')
            .map(str::trim)
            .any(|allowed| allowed == method));
    }
}
