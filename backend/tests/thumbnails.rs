mod support;

use std::path::Path;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use serde_json::Value;
use tokio::process::Command;
use tower::ServiceExt;

use support::{assert_api_error, get, make_state, router};
use video_kadr_backend::db::Db;
use video_kadr_backend::library::{now_secs, Library, MediaEntry};
use video_kadr_backend::process_control::ProcessRuntime;
use video_kadr_backend::state::{AppState, ToolInfo};
use video_kadr_backend::tools;

async fn add_entry(state: &AppState, id: &str, filename: &str, media_type: &str, bytes: &[u8]) {
    add_entry_with_duration(state, id, filename, media_type, bytes, 1.0).await;
}

async fn add_entry_with_duration(
    state: &AppState,
    id: &str,
    filename: &str,
    media_type: &str,
    bytes: &[u8],
    duration: f64,
) {
    let path = state.storage.join("sources").join(filename);
    tokio::fs::write(&path, bytes).await.unwrap();
    assert!(
        state
            .library
            .add(MediaEntry {
                id: id.into(),
                kind: "source".into(),
                filename: filename.into(),
                url: format!("/files/sources/{filename}"),
                media_type: Some(media_type.into()),
                title: None,
                duration: Some(duration),
                width: None,
                height: None,
                fps: None,
                vcodec: None,
                acodec: None,
                size_bytes: Some(bytes.len() as u64),
                created_at: now_secs(),
            })
            .await
    );
}

#[tokio::test]
async fn unknown_and_traversal_ids_never_reach_media_or_filesystem() {
    let (state, _dir) = make_state(false, false).await;
    let app = router(state.clone());

    let response = app
        .clone()
        .oneshot(get("/api/library/missing/thumbnail"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app
        .clone()
        .oneshot(get("/api/library/..%2Fescape/thumbnail"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = app
        .clone()
        .oneshot(get("/api/library/missing/filmstrip"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app
        .oneshot(get("/api/library/..%2Fescape/filmstrip"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn missing_waveform_filter_has_a_typed_honest_fallback() {
    let (state, _dir) = make_state(true, false).await;
    add_entry(&state, "audio-1", "voice.wav", "audio", b"not decoded").await;
    let response = router(state)
        .oneshot(get("/api/library/audio-1/thumbnail"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_api_error(&json, "thumbnail_unavailable");
    assert!(json["error"]
        .as_str()
        .is_some_and(|message| message.contains("Waveform")));
}

#[tokio::test]
async fn filmstrip_rejects_non_video_and_missing_filter_without_spawning() {
    let (state, _dir) = make_state(true, false).await;
    add_entry(&state, "audio-1", "voice.wav", "audio", b"not decoded").await;
    add_entry(&state, "video-1", "clip.mp4", "video", b"not decoded").await;
    let app = router(state);

    let response = app
        .clone()
        .oneshot(get("/api/library/audio-1/filmstrip"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let response = app
        .oneshot(get("/api/library/video-1/filmstrip"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_api_error(&json, "filmstrip_unavailable");
}

#[tokio::test]
async fn thumbnail_and_filmstrip_routes_are_cors_preflight_enabled() {
    let (state, _dir) = make_state(false, false).await;
    let app = router(state);
    for uri in [
        "/api/library/video-1/thumbnail",
        "/api/library/video-1/filmstrip",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri(uri)
                    .header(header::ORIGIN, "http://localhost:5173")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
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
            .any(|allowed| allowed == "GET"));
    }
}

#[tokio::test]
async fn real_ffmpeg_thumbnail_is_bounded_cached_invalidated_and_deleted() {
    let runtime = ProcessRuntime::local_default();
    let (ffmpeg, ffmpeg_version) = tools::check_tool(&runtime, "ffmpeg", "-version").await;
    if !ffmpeg {
        eprintln!("skipping real thumbnail smoke: ffmpeg unavailable");
        return;
    }
    let (encoders, muxers, filters) = tools::inspect_ffmpeg_support(&runtime).await;
    if !encoders.iter().any(|value| value == "png")
        || !muxers.iter().any(|value| value == "image2")
        || ["scale", "pad", "setsar"]
            .iter()
            .any(|required| !filters.iter().any(|value| value == required))
    {
        eprintln!("skipping real thumbnail smoke: required FFmpeg components unavailable");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().to_path_buf();
    for subdirectory in ["sources", "outputs", "staging"] {
        tokio::fs::create_dir_all(storage.join(subdirectory))
            .await
            .unwrap();
    }
    let image_path = storage.join("sources/still.png");
    if !generate_fixture(&image_path).await {
        eprintln!("skipping real thumbnail smoke: could not generate fixture");
        return;
    }
    let library = Library::load(storage.clone()).await;
    let db = Db::open(&storage).await.unwrap();
    let state = AppState::new(
        storage.clone(),
        2,
        ToolInfo {
            ffmpeg: true,
            ytdlp: false,
            ffmpeg_version,
            ytdlp_version: None,
            ffmpeg_encoders: encoders,
            ffmpeg_muxers: muxers,
            ffmpeg_filters: filters,
        },
        library,
        db,
    );
    let fixture = tokio::fs::read(&image_path).await.unwrap();
    add_entry(&state, "still-1", "still.png", "image", &fixture).await;
    let app = router(state.clone());

    let stable = app
        .clone()
        .oneshot(get("/api/library/still-1/thumbnail"))
        .await
        .unwrap();
    assert_eq!(stable.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(stable.headers()[header::CACHE_CONTROL], "private, no-cache");
    let location = stable.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(location.starts_with("/api/library/still-1/thumbnail/"));

    let generated = app.clone().oneshot(get(&location)).await.unwrap();
    assert_eq!(generated.status(), StatusCode::OK);
    assert_eq!(generated.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(
        generated.headers()[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
    assert_eq!(generated.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        generated.headers()["content-security-policy"],
        "sandbox; default-src 'none'"
    );
    let etag = generated.headers()[header::ETAG].clone();
    let bytes = to_bytes(generated.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    assert!(bytes.len() > 24);
    assert_eq!(&bytes[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 320);
    assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 180);

    let conditional = Request::builder()
        .uri(&location)
        .header(header::IF_NONE_MATCH, etag)
        .body(Body::empty())
        .unwrap();
    let not_modified = app.clone().oneshot(conditional).await.unwrap();
    assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);

    let stable_again = app
        .clone()
        .oneshot(get("/api/library/still-1/thumbnail"))
        .await
        .unwrap();
    assert_eq!(stable_again.headers()[header::LOCATION], location);

    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    assert!(generate_fixture_with_color(&image_path, "blue").await);
    let invalidated = app
        .clone()
        .oneshot(get("/api/library/still-1/thumbnail"))
        .await
        .unwrap();
    let new_location = invalidated.headers()[header::LOCATION].to_str().unwrap();
    assert_ne!(new_location, location);
    let refreshed = app.clone().oneshot(get(new_location)).await.unwrap();
    assert_eq!(refreshed.status(), StatusCode::OK);

    let deleted = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/library/still-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let cache_root = storage.join("thumbnails/cache");
    let mut cache = tokio::fs::read_dir(cache_root).await.unwrap();
    assert!(cache.next_entry().await.unwrap().is_none());
}

#[tokio::test]
async fn real_ffmpeg_filmstrip_distributes_fixed_distinct_frames() {
    let runtime = ProcessRuntime::local_default();
    let (ffmpeg, ffmpeg_version) = tools::check_tool(&runtime, "ffmpeg", "-version").await;
    if !ffmpeg {
        eprintln!("skipping real filmstrip smoke: ffmpeg unavailable");
        return;
    }
    let (encoders, muxers, filters) = tools::inspect_ffmpeg_support(&runtime).await;
    if !encoders.iter().any(|value| value == "png")
        || !encoders.iter().any(|value| value == "mpeg4")
        || !muxers.iter().any(|value| value == "image2")
        || ["scale", "pad", "setsar", "hstack"]
            .iter()
            .any(|required| !filters.iter().any(|value| value == required))
    {
        eprintln!("skipping real filmstrip smoke: required FFmpeg components unavailable");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().to_path_buf();
    for subdirectory in ["sources", "outputs", "staging"] {
        tokio::fs::create_dir_all(storage.join(subdirectory))
            .await
            .unwrap();
    }
    let video_path = storage.join("sources/clip.mp4");
    if !generate_video_fixture(&video_path).await {
        eprintln!("skipping real filmstrip smoke: could not generate video fixture");
        return;
    }
    let library = Library::load(storage.clone()).await;
    let db = Db::open(&storage).await.unwrap();
    let state = AppState::new(
        storage.clone(),
        2,
        ToolInfo {
            ffmpeg: true,
            ytdlp: false,
            ffmpeg_version,
            ytdlp_version: None,
            ffmpeg_encoders: encoders,
            ffmpeg_muxers: muxers,
            ffmpeg_filters: filters,
        },
        library,
        db,
    );
    let fixture = tokio::fs::read(&video_path).await.unwrap();
    add_entry_with_duration(&state, "clip-1", "clip.mp4", "video", &fixture, 8.0).await;
    let app = router(state);

    let stable = app
        .clone()
        .oneshot(get("/api/library/clip-1/filmstrip"))
        .await
        .unwrap();
    assert_eq!(stable.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(stable.headers()[header::CACHE_CONTROL], "private, no-cache");
    let location = stable.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(location.starts_with("/api/library/clip-1/filmstrip/"));

    let generated = app.clone().oneshot(get(&location)).await.unwrap();
    assert_eq!(generated.status(), StatusCode::OK);
    assert_eq!(generated.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(
        generated.headers()[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
    assert_eq!(generated.headers()["x-filmstrip-cells"], "8");
    assert_eq!(generated.headers()["x-filmstrip-cell-width"], "160");
    assert_eq!(generated.headers()["x-filmstrip-cell-height"], "90");
    assert_eq!(generated.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        generated.headers()["content-security-policy"],
        "sandbox; default-src 'none'"
    );
    let etag = generated.headers()[header::ETAG].clone();
    let bytes = to_bytes(generated.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    assert!(bytes.len() > 24);
    assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 1280);
    assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 90);

    let sprite_path = storage.join("filmstrip-smoke.png");
    tokio::fs::write(&sprite_path, &bytes).await.unwrap();
    let mut cells = std::collections::HashSet::new();
    for index in 0..8 {
        let pixels = read_filmstrip_cell(&sprite_path, index).await.unwrap();
        assert_eq!(pixels.len(), 160 * 90 * 3);
        cells.insert(pixels);
    }
    assert!(
        cells.len() >= 4,
        "distributed filmstrip should contain visibly distinct frames"
    );

    let conditional = Request::builder()
        .uri(&location)
        .header(header::IF_NONE_MATCH, etag)
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(conditional).await.unwrap().status(),
        StatusCode::NOT_MODIFIED
    );

    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let mut changed_fixture = fixture.clone();
    changed_fixture.push(0);
    tokio::fs::write(&video_path, changed_fixture)
        .await
        .unwrap();
    let invalidated = app
        .clone()
        .oneshot(get("/api/library/clip-1/filmstrip"))
        .await
        .unwrap();
    let new_location = invalidated.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_owned();
    assert_ne!(new_location, location);
    assert_eq!(
        app.clone().oneshot(get(&location)).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.clone()
            .oneshot(get(&new_location))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let deleted = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/library/clip-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let mut cache = tokio::fs::read_dir(storage.join("thumbnails/cache"))
        .await
        .unwrap();
    assert!(cache.next_entry().await.unwrap().is_none());
}

async fn generate_fixture(path: &Path) -> bool {
    generate_fixture_with_color(path, "red").await
}

async fn generate_fixture_with_color(path: &Path, color: &str) -> bool {
    Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("color=c={color}:s=64x48"),
            "-frames:v",
            "1",
        ])
        .arg(path)
        .status()
        .await
        .is_ok_and(|status| status.success())
}

async fn generate_video_fixture(path: &Path) -> bool {
    Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=duration=8:size=160x90:rate=8",
            "-an",
            "-c:v",
            "mpeg4",
            "-q:v",
            "2",
        ])
        .arg(path)
        .status()
        .await
        .is_ok_and(|status| status.success())
}

async fn read_filmstrip_cell(path: &Path, index: u32) -> Option<Vec<u8>> {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args([
            "-vf",
            &format!("crop=160:90:{}:0", index * 160),
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "pipe:1",
        ])
        .output()
        .await
        .ok()?;
    output.status.success().then_some(output.stdout)
}
