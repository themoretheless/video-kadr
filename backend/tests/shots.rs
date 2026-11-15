mod support;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use support::{make_state, router, send};
use tower::ServiceExt;
use video_kadr_backend::library::MediaEntry;

fn post(uri: &str, payload: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap()
}

#[tokio::test]
async fn real_source_analysis_cache_metrics_and_frame_endpoints() {
    let (state, _dir) = make_state(true, false).await;
    let path = state.storage.join("sources/scene.mkv");
    let generated = tokio::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=black:s=320x180:r=25:d=1",
            "-f",
            "lavfi",
            "-i",
            "color=white:s=320x180:r=25:d=1",
            "-filter_complex",
            "[0:v][1:v]concat=n=2:v=1:a=0",
            "-c:v",
            "ffv1",
            "-y",
        ])
        .arg(&path)
        .output()
        .await
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert!(
        state
            .library
            .add(MediaEntry::from_result(
                "source",
                &json!({
                    "id": "scene", "filename": "scene.mkv", "url": "/files/sources/scene.mkv",
                    "mediaType": "video", "duration": 2, "width": 320, "height": 180
                })
            ))
            .await
    );
    let app = router(state.clone());
    let (status, scan, _) = send(&app, post("/api/library/scene/shots", json!({}))).await;
    assert_eq!(status, StatusCode::OK, "{scan}");
    assert_eq!(scan["cuts"][0]["tick"], 1_000_000);
    describe_scene(&state, &scan).await;
    install_and_use_scene_model(&state, &scan).await;
    let mut dirs = tokio::fs::read_dir(state.storage.join("shot-analysis"))
        .await
        .unwrap();
    let cache = dirs
        .next_entry()
        .await
        .unwrap()
        .unwrap()
        .path()
        .join("scan.json");
    let modified = tokio::fs::metadata(&cache)
        .await
        .unwrap()
        .modified()
        .unwrap();
    let (_, cached, _) = send(&app, post("/api/library/scene/shots", json!({}))).await;
    assert_eq!(cached, scan);
    assert_eq!(
        tokio::fs::metadata(&cache)
            .await
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
    tokio::fs::write(&cache, b"corrupted").await.unwrap();
    assert_eq!(
        send(&app, post("/api/library/scene/shots", json!({})))
            .await
            .1,
        scan
    );
    let (status, stats, _) = send(
        &app,
        post(
            "/api/library/scene/shots/measure",
            json!({"spans": [
                {"startTicks":0,"endTicks":1_000_000}, {"startTicks":1_000_000,"endTicks":2_000_000}
            ]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{stats}");
    assert!(stats[0]["brightness"].as_f64().unwrap() < 0.01);
    assert!(stats[1]["brightness"].as_f64().unwrap() > 0.99);
    assert!(stats[0]["flags"]
        .as_array()
        .unwrap()
        .contains(&json!("black")));
    assert!(stats[1]["flags"]
        .as_array()
        .unwrap()
        .contains(&json!("freeze")));
    assert_eq!(send(&app, post("/api/library/scene/shots/measure", json!({"expectedSourceHash":"outdated", "spans":[{"startTicks":0,"endTicks":1_000_000}]}))).await.0, StatusCode::BAD_REQUEST);
    let (status, pair, _) = send(
        &app,
        post(
            "/api/library/scene/compare-frames",
            json!({"leftTick":500_000,"rightTick":1_500_000}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(pair["similarity"].as_f64().unwrap() < 0.1);
    let response = app
        .clone()
        .oneshot(support::get(
            "/api/library/scene/analysis-frame?tick=500000",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/png");
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    let bytes = to_bytes(response.into_body(), 2_000_000).await.unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    for payload in [
        json!({"spans": []}),
        json!({"spans":[{"startTicks":1,"endTicks":0}]}),
        json!({"spans":[{"startTicks":0,"endTicks":2_000_001}]}),
    ] {
        assert_eq!(
            send(&app, post("/api/library/scene/shots/measure", payload))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        send(
            &app,
            post(
                "/api/library/scene/compare-frames",
                json!({"leftTick":2_000_000,"rightTick":0})
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(&app, post("/api/library/missing/shots", json!({})))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

async fn install_and_use_scene_model(state: &video_kadr_backend::state::AppState, scan: &Value) {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use video_kadr_backend::analysis::{scene_description::Config, scene_model::MODEL};

    let install_actor = "scene-installer";
    let installer = state
        .db
        .register_auth_user(install_actor, "synthetic installer password")
        .await
        .unwrap()
        .unwrap();
    let viewer = state
        .db
        .register_auth_user("scene-viewer", "synthetic viewer password")
        .await
        .unwrap()
        .unwrap();
    let pulls = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(tokio::sync::Notify::new());
    let completions = Arc::new(AtomicUsize::new(0));
    let digest_version = Arc::new(AtomicUsize::new(0));
    let live = std::env::var("KADR_TEST_SCENE_OLLAMA_URL").ok();
    let (origin, mock) = if let Some(origin) = &live {
        (origin.clone(), None)
    } else {
        let model = axum::Router::new()
            .route("/api/tags", axum::routing::get({
                let versions = digest_version.clone();
                move || {
                    let version = versions.load(Ordering::SeqCst);
                    async move {
                        let models = if version == 0 { vec![] } else { vec![json!({"name": MODEL, "digest": format!("{version:064x}")})] };
                        axum::Json(json!({"models":models}))
                    }
                }
            }))
            .route("/api/pull", axum::routing::post({
                let (pulls, release, versions) = (pulls.clone(), release.clone(), digest_version.clone());
                move |axum::Json(body): axum::Json<Value>| {
                    let (pulls, release, versions) = (pulls.clone(), release.clone(), versions.clone());
                    async move {
                        assert_eq!(body, json!({"model":MODEL,"stream":false}));
                        pulls.fetch_add(1, Ordering::SeqCst);
                        release.notified().await;
                        versions.store(1, Ordering::SeqCst);
                        axum::Json(json!({"status":"success"}))
                    }
                }
            }))
            .route("/v1/chat/completions", axum::routing::post({
                let completions = completions.clone();
                move |axum::Json(body): axum::Json<Value>| {
                    completions.fetch_add(1, Ordering::SeqCst);
                    async move {
                        assert_eq!(body["model"], MODEL);
                        axum::Json(json!({"choices":[{"message":{"content":"{\"text\":\"Wide shot\",\"tags\":[\"cut\"]}"}}]}))
                    }
                }
            }));
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        (
            origin,
            Some(tokio::spawn(async move {
                axum::serve(listener, model).await.unwrap()
            })),
        )
    };
    let config = Config::new_ollama(&origin, install_actor.into()).unwrap();
    let app = router(state.clone().with_scene_description(Some(config)));
    let model_request = |method, token: &str| {
        Request::builder()
            .method(method)
            .uri("/api/scene-model")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        send(&app, support::get("/api/scene-model")).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&app, model_request("POST", &viewer.token)).await.0,
        StatusCode::FORBIDDEN
    );
    let (status, initial, _) = send(&app, model_request("GET", &viewer.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(initial["canInstall"], false);
    if mock.is_some() {
        assert_eq!(initial["phase"], "missing");
    }
    let (first, second) = tokio::join!(
        send(&app, model_request("POST", &installer.token)),
        send(&app, model_request("POST", &installer.token))
    );
    for (status, data, _) in [first, second] {
        assert_eq!(status, StatusCode::OK, "{data}");
        assert_eq!(data["model"], MODEL);
        assert!(data["phase"] == "installing" || data["phase"] == "ready");
    }
    release.notify_one();
    tokio::time::timeout(
        std::time::Duration::from_secs(if live.is_some() { 900 } else { 5 }),
        async {
            loop {
                let (_, status, _) = send(&app, model_request("GET", &viewer.token)).await;
                if status["phase"] == "ready" {
                    break;
                }
                assert_eq!(status["phase"], "installing", "{status}");
                tokio::time::sleep(std::time::Duration::from_millis(if live.is_some() {
                    1000
                } else {
                    20
                }))
                .await;
            }
        },
    )
    .await
    .unwrap();
    let request_description = || {
        let mut req = post(
            "/api/library/scene/shots/describe",
            json!({
                "span":{"startTicks":0,"endTicks":2_000_000},
                "expectedSourceHash": scan["sourceHash"], "language":"en",
            }),
        );
        req.headers_mut().insert(
            "authorization",
            format!("Bearer {}", viewer.token).parse().unwrap(),
        );
        req
    };
    for _ in 0..2 {
        let (status, result, _) = send(&app, request_description()).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert!(!result["text"].as_str().unwrap().is_empty());
    }
    if let Some(mock) = mock {
        assert_eq!(pulls.load(Ordering::SeqCst), 1);
        assert_eq!(completions.load(Ordering::SeqCst), 1);
        digest_version.store(2, Ordering::SeqCst);
        assert_eq!(send(&app, request_description()).await.0, StatusCode::OK);
        assert_eq!(completions.load(Ordering::SeqCst), 2);
        mock.abort();
    }
}

async fn describe_scene(state: &video_kadr_backend::state::AppState, scan: &Value) {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use std::sync::{Arc, Mutex};
    use video_kadr_backend::analysis::scene_description::Config;

    let user = state
        .db
        .register_auth_user("scene-reviewer", "synthetic scene password")
        .await
        .unwrap()
        .unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_model = requests.clone();
    let model = axum::Router::new().route("/v1/chat/completions", axum::routing::post(
        move |headers: axum::http::HeaderMap, axum::Json(body): axum::Json<Value>| {
            let requests = requests_for_model.clone();
            async move {
                assert_eq!(headers["authorization"], "Bearer model-key");
                requests.lock().unwrap().push(body);
                axum::Json(json!({"choices":[{"message":{"content": "{\"text\":\"A hard cut\",\"tags\":[\"cut\"]}"}}]}))
            }
        },
    ));
    let bind = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let config = Config::new(
        &format!("http://{}/v1/chat/completions", bind.local_addr().unwrap()),
        "test-vision".into(),
        Some("model-key".into()),
    )
    .unwrap();
    let model = tokio::spawn(async move { axum::serve(bind, model).await.unwrap() });
    let app = router(state.clone().with_scene_description(Some(config.clone())));
    let request = |token: Option<&str>, language: &str, hash: &Value| {
        let mut request = post(
            "/api/library/scene/shots/describe",
            json!({
                "span": {"startTicks":0,"endTicks":2_000_000},
                "expectedSourceHash":hash, "language":language, "focus":"shot-type"
            }),
        );
        if let Some(token) = token {
            request
                .headers_mut()
                .insert("authorization", format!("Bearer {token}").parse().unwrap());
        }
        request
    };
    let hash = &scan["sourceHash"];
    let cached = |language: &str, hash: &Value, token: Option<&str>| {
        let mut req = post(
            "/api/library/scene/shots/descriptions",
            json!({
                "spans":[{"startTicks":0,"endTicks":1_000_000},{"startTicks":0,"endTicks":2_000_000}],
                "expectedSourceHash":hash, "language":language, "focus":"shot-type"
            }),
        );
        if let Some(token) = token {
            req.headers_mut()
                .insert("authorization", format!("Bearer {token}").parse().unwrap());
        }
        req
    };
    assert_eq!(
        send(&app, cached("en", hash, None)).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, cold, _) = send(&app, cached("en", hash, Some(&user.token))).await;
    assert_eq!((status, cold), (StatusCode::OK, json!([])));
    assert_eq!(
        send(&app, request(None, "en", hash)).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&app, request(Some(&user.token), "en", &json!("stale")))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    for language in ["en", "en", "ru"] {
        let (status, result, _) = send(&app, request(Some(&user.token), language, hash)).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result, json!({"text":"A hard cut","tags":["cut"]}));
    }
    model.abort();
    let _ = model.await;
    let mut without_ffmpeg = state.clone().with_scene_description(Some(config));
    let mut tools = (*state.tools).clone();
    tools.ffmpeg = false;
    without_ffmpeg.tools = Arc::new(tools);
    let offline = router(without_ffmpeg);
    let (status, warm, _) = send(&offline, cached("en", hash, Some(&user.token))).await;
    assert_eq!(
        (status, warm),
        (
            StatusCode::OK,
            json!([{
                "span":{"startTicks":0,"endTicks":2_000_000},
                "description":{"text":"A hard cut","tags":["cut"]}
            }])
        )
    );
    assert_eq!(
        send(&offline, cached("en", &json!("stale"), Some(&user.token)))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["model"], "test-vision");
    let frames = requests[0]["messages"][0]["content"].as_array().unwrap();
    assert_eq!(frames.len(), 5);
    assert_eq!(frames[1]["text"], "Frame at 0.500000s:");
    assert_eq!(frames[3]["text"], "Frame at 1.500000s:");
    let sample = |index: usize| {
        let encoded = frames[index]["image_url"]["url"].as_str().unwrap();
        let png = STANDARD
            .decode(encoded.strip_prefix("data:image/png;base64,").unwrap())
            .unwrap();
        image::load_from_memory(&png)
            .unwrap()
            .to_rgb8()
            .get_pixel(0, 0)
            .0
    };
    assert_eq!(sample(2), [0; 3]);
    assert_eq!(sample(4), [255; 3]);
    assert!(requests[1]["messages"][0]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Russian"));
}

#[tokio::test]
async fn unavailable_analysis_reports_missing_native_tool() {
    let (state, _dir) = make_state(false, false).await;
    assert_eq!(
        send(&router(state), post("/api/library/scene/shots", json!({})))
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}
