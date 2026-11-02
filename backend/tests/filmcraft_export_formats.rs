mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
async fn call(app: &Router, token: &str, command: &str, params: Value) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/compositions/filmcraft/command")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"command":command,"params":params}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        response.status().as_u16(),
        serde_json::from_slice(
            &to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap(),
    )
}
fn document() -> String {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    let sequence = &mut document["project"]["items"]["6"]["kind"]["Sequence"];
    sequence["settings"]["frame_rate"] = json!({"num":24,"den":1});
    sequence["settings"]["width"] = json!(64);
    sequence["settings"]["height"] = json!(48);
    for tracks in [
        "video_tracks",
        "audio_tracks",
        "submix_tracks",
        "caption_tracks",
    ] {
        sequence[tracks] = json!([]);
    }
    sequence["mark_in"] = json!(0);
    sequence["mark_out"] = json!(0);
    serde_json::to_string(&document).unwrap()
}
#[tokio::test]
async fn all_advertised_native_default_formats_publish_real_owned_outputs_without_ffmpeg() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("formats-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let other = state
        .db
        .register_auth_user("formats-other", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    assert_eq!(
        call(&app, "invalid", "export.formats", json!({})).await.0,
        401
    );
    let (status, formats) = call(&app, &owner.token, "export.formats", json!({})).await;
    assert_eq!(status, 200, "{formats}");
    let formats = formats.as_array().unwrap();
    assert_eq!(formats.len(), 14);
    assert_eq!(
        formats
            .iter()
            .map(|format| format["id"].as_str().unwrap())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        14
    );
    assert_eq!(state.db.load_jobs().await.unwrap().len(), 0);
    let unsupported = formats
        .iter()
        .filter(|format| format["available"] == false)
        .map(|format| format["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        unsupported,
        if cfg!(target_os = "macos") {
            vec![]
        } else {
            vec!["hevc"]
        }
    );
    let document = document();
    let mut exported = 0;
    for format in formats {
        if format["available"] == false {
            assert!(format["unavailableReason"].as_str().is_some());
            if format["id"]=="hevc" {assert_eq!(format["hostExtension"],"mp4");}else{assert!(format["hostExtension"].is_null());}
            assert_eq!(call(&app,&owner.token,"export.quick",json!({"document":document,"sequence":6,"sources":{},"format":format["id"],"wait":true})).await.0,400);
            continue;
        }
        assert_eq!(format["native"], true);
        let(status,result)=call(&app,&owner.token,"export.quick",json!({"document":document,"sequence":6,"sources":{},"format":format["id"],"wait":true})).await;
        assert_eq!(status, 200, "{}: {result}", format["id"]);
        assert_eq!(result["status"], "done", "{}: {result}", format["id"]);
        let output = &result["result"];
        assert_eq!(output["container"], format["hostExtension"]);
        let file = state
            .outputs_dir()
            .join(output["filename"].as_str().unwrap());
        assert!(tokio::fs::metadata(file).await.unwrap().len() > 0);
        for (token, allowed) in [(&owner.token, true), (&other.token, false)] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(output["url"].as_str().unwrap())
                        .header("authorization", format!("Bearer {token}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().is_success(), allowed, "{}", format["id"]);
        }
        if format["imageSequence"] == true || format["id"] == "mxf-opatom" {
            assert_eq!(format["hostExtension"], "zip");
        }
        if format["id"] == "aiff" {
            assert_eq!(format["extension"], "aif");
            assert_eq!(format["hostExtension"], "aiff");
        }
        if format["id"] == "gif" || format["imageSequence"] == true {
            assert_eq!(format["audio"], false);
        }
        if format["id"] == "wav" || format["id"] == "aiff" {
            assert_eq!(format["video"], false);
            assert_eq!(format["audio"], true);
        }
        exported += 1;
    }
    assert_eq!(exported, if cfg!(target_os = "macos") { 14 } else { 13 });
    assert_eq!(
        state.db.load_jobs().await.unwrap().len(),
        if cfg!(target_os = "macos") { 14 } else { 13 }
    );
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
#[tokio::test]
async fn external_encoder_flags_do_not_replace_the_native_hevc_probe() {
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("formats-external", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let (status, formats) = call(&app, &owner.token, "export.formats", json!({})).await;
    assert_eq!(status, 200);
    assert!(formats
        .as_array()
        .unwrap()
        .iter()
        .filter(|format| format["id"] == "hevc")
        .all(|format| format["available"] == cfg!(target_os = "macos")));
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn saved_hevc_presets_reach_quick_with_metadata_and_pixel_aspect() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("hevc-preset-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let(status,saved)=call(&app,&owner.token,"export.presets.save",json!({"name":"HEVC review","settings":{"format":"hevc","multiplexer":"mov","includeAudio":false,"pixelAspect":[4,3],"metadata":{"title":"HEVC review"},"bitrateKbps":1000,"maxBitrateKbps":2000}})).await;
    assert_eq!(status, 200, "{saved}");
    let (status, exported) = call(
        &app,
        &owner.token,
        "export.quick",
        json!({"document":document(),"sequence":6,"sources":{},"preset":"HEVC review","wait":true}),
    )
    .await;
    assert_eq!(status, 200, "{exported}");
    assert_eq!(exported["status"], "done", "{exported}");
    assert_eq!(exported["result"]["videoCodec"], "hevc");
    let filename = exported["result"]["filename"].as_str().unwrap();
    assert!(filename.ends_with(".mov"));
    let bytes = std::fs::read(state.outputs_dir().join(filename)).unwrap();
    let movie = filmcraft_isobmff::open(&bytes).unwrap();
    assert_eq!(movie.metadata.title(), Some("HEVC review"));
    assert_eq!(movie.tracks[0].video().unwrap().pixel_aspect, Some((4, 3)));
    let count = state.db.load_jobs().await.unwrap().len();
    let(status,error)=call(&app,&owner.token,"export.quick",json!({"document":document(),"sequence":6,"sources":{},"preset":"HEVC review","bitrateMode":"vbr2Pass","wait":true})).await;
    assert_eq!(status, 400, "{error}");
    assert_eq!(state.db.load_jobs().await.unwrap().len(), count);
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
