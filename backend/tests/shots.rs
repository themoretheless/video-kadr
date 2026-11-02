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
