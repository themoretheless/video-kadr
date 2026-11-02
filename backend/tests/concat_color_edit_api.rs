mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use tower::ServiceExt;
use video_kadr_backend::library::MediaEntry;
async fn preview(app: &axum::Router, project: &Value, time: u64, token: &str) -> Vec<u8> {
    let request = Request::builder()
        .method("POST")
        .uri("/api/compositions/nested-preview")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"project":project,"timeUs":time,"renderedOutputs":{}}).to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    assert_eq!(bytes.len(), 1024);
    bytes.to_vec()
}
#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe QTRLE and current frontend-generated native edit fixtures"]
async fn actual_concat_color_split_trim_duplicate_preserve_video_and_animation() {
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("concat-color-edit-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let raw = state.sources_dir().join("editing.rgba");
    let data: Vec<u8> = (0..40u32)
        .flat_map(|f| {
            (0..256u32).flat_map(move |p| {
                [
                    ((p * 37 + f * 13) % 256) as u8,
                    ((p * 73 + f * 29) % 256) as u8,
                    ((p * 11 + f * 47) % 256) as u8,
                    255,
                ]
            })
        })
        .collect();
    std::fs::write(&raw, data).unwrap();
    let movie = state.sources_dir().join("editing.mov");
    let result = std::process::Command::new("ffmpeg")
        .args([
            "-nostdin", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", "16x16", "-r",
            "4", "-i",
        ])
        .arg(&raw)
        .args(["-c:v", "qtrle", "-pix_fmt", "argb", "-threads", "1"])
        .arg(&movie)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    state.library.add(MediaEntry::from_result("source",&json!({"id":"source","filename":"editing.mov","url":"/files/sources/editing.mov","mediaType":"video","width":16,"height":16,"duration":10.0,"fps":4,"vcodec":"qtrle"}))).await;
    let space = state
        .db
        .create_space(&owner.user.username, "Edited Concat source")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Source claim",
            &json!({"schemaVersion":1,"sources":{"source":{"id":"source"}}}),
            &["source".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/concat-color-edit.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 198);
    let app = support::router(state.clone());
    // Opt-in local browser qualification only: private fixture DB/source, bounded lifetime.
    if let Ok(directory) = std::env::var("VIDEO_KADR_TEST_WHEEL_LIVE_DIR") {
        let directory = std::path::PathBuf::from(directory);
        assert!(directory.is_absolute() && directory.is_dir());
        let case = cases
            .iter()
            .find(|c| c["kind"] == "concat_color_wheels" && c["mode"] == "freeze")
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serving = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap()
        });
        let connection = directory.join("connection.json");
        std::fs::write(&connection,serde_json::to_vec(&json!({"url":format!("http://{address}"),"token":owner.token,"project":case["original"]})).unwrap()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&connection, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        eprintln!("private wheel browser fixture ready");
        let finished = tokio::time::timeout(Duration::from_secs(180), async {
            while !directory.join("stop").exists() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await;
        let _ = stop.send(());
        serving.await.unwrap();
        state.begin_shutdown();
        assert!(state.wait_for_tasks(Duration::from_secs(15)).await);
        assert!(finished.is_ok(), "browser fixture timed out");
        return;
    }
    let mut comparisons = 0;
    for case in cases {
        let mut originals = BTreeMap::new();
        let mut signatures = BTreeSet::new();
        for edit in case["edits"].as_array().unwrap() {
            for time in edit["times"].as_array().unwrap() {
                let time = time.as_u64().unwrap();
                if !originals.contains_key(&time) {
                    let bytes = preview(&app, &case["original"], time, &owner.token).await;
                    signatures.insert(bytes.clone());
                    originals.insert(time, bytes);
                }
                let actual = preview(
                    &app,
                    &edit["project"],
                    time + edit["offset"].as_u64().unwrap(),
                    &owner.token,
                )
                .await;
                let expected = if edit["referenceProject"].is_object() {
                    preview(&app, &edit["referenceProject"], time + edit["referenceOffset"].as_u64().unwrap(), &owner.token).await
                } else { originals[&time].clone() };
                assert_eq!(
                    &actual, &expected,
                    "{} {} at{time}",
                    case["name"], edit["name"]
                );
                comparisons += 1;
            }
        }
        if ["concat_curves","concat_sharpen","concat_vignette","concat_contrast_pop","concat_bleach_bypass","concat_retro_cartoon","concat_marquee","concat_autumn","concat_neon","concat_pink_fuzz","concat_night","concat_spooky","concat_cool","concat_warm","concat_teal_orange","concat_noir","concat_dreamy","concat_high_contrast_mono","concat_landscape_pop","concat_natural","concat_retro_70s","concat_silver","concat_tropical","concat_vaporwave","concat_winter_cool","concat_classic_chrome","concat_cyberpunk","concat_delicious","concat_rosy","concat_sci_fi","concat_clean","concat_matcha","concat_sage","concat_ivory","concat_milk","concat_bronze","concat_cool_blue","concat_fresh","concat_vivid","concat_soft_light","concat_moody","concat_rain","concat_umber","concat_fair","concat_peach","concat_polaroid","concat_vintage","concat_cloudless_sky","concat_lagoon","concat_golden_hour","concat_daydream"].iter().any(|kind|case["kind"]==*kind) && case["mode"] == "freeze" {
            assert_eq!(
                signatures.len(),
                1,
                "static curve/spatial filter on a frozen source must remain constant"
            );
        } else {
            assert!(
                signatures.len() > 1,
                "{} animated or moving reference is constant",
                case["name"]
            );
        }
    }
    assert_eq!(comparisons, 4356);
    state.begin_shutdown();
    assert!(state.wait_for_tasks(Duration::from_secs(15)).await);
}
