mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
use video_kadr_backend::{
    domain::composition::{CanvasSpec, Composition, TransformSpec},
    library::MediaEntry,
};
fn command(args: &[&str]) -> Vec<u8> {
    let result = std::process::Command::new("ffmpeg")
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}
async fn terminal(app: &axum::Router, id: &str) -> Value {
    for _ in 0..3000 {
        let (status, body, _) = support::send(app, support::get(&format!("/api/jobs/{id}"))).await;
        assert_eq!(status, StatusCode::OK);
        if matches!(
            body["status"].as_str(),
            Some("done" | "error" | "cancelled")
        ) {
            return body;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("Chroma Key export did not finish")
}
fn decode(path: &Path) -> Vec<u8> {
    command(&[
        "-nostdin",
        "-v",
        "error",
        "-i",
        path.to_str().unwrap(),
        "-map",
        "0:v:0",
        "-an",
        "-pix_fmt",
        "rgb24",
        "-f",
        "rawvideo",
        "pipe:1",
    ])
}
#[tokio::test]
#[ignore = "requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn ordinary_chroma_key_export_removes_screen_at_selected_source_clock() {
    legacy_screen_export("green-screen", [0, 177, 64, 255]).await;
    legacy_screen_export("blue-screen", [48, 90, 210, 255]).await;
}
async fn legacy_screen_export(legacy_id: &str, screen: [u8; 4]) {
    let (state, _directory) = support::make_state(true, false).await;
    let mut input = Vec::new();
    for frame in 0..2 {
        for _y in 0..32 {
            for x in 0..64 {
                let green = (x < 32) == (frame == 1);
                input.extend_from_slice(if green {
                    &screen
                } else {
                    &[255, 255, 255, 255]
                });
            }
        }
    }
    let raw = state.sources_dir().join("chroma-key-delivery.rgba");
    std::fs::write(&raw, input).unwrap();
    let movie = state.sources_dir().join("chroma-key-delivery.mov");
    command(&[
        "-nostdin",
        "-v",
        "error",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
        "-s",
        "64x32",
        "-r",
        "4",
        "-i",
        raw.to_str().unwrap(),
        "-c:v",
        "qtrle",
        "-threads",
        "1",
        movie.to_str().unwrap(),
    ]);
    state.library.add(MediaEntry::from_result("source",&json!({"id":"chroma-key-source","filename":"chroma-key-delivery.mov","url":"/files/sources/chroma-key-delivery.mov","mediaType":"video","width":64,"height":32,"duration":0.5,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
    let app = support::router(state.clone());
    let conversion = json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":legacy_id,"params":{"similarity":0,"softness":0},"keys":{"similarity":[{"at":0,"value":0},{"at":0.5,"value":30,"ease":"inOut"},{"at":1,"value":0}]}}]});
    let request = Request::builder()
        .method("POST")
        .uri("/api/compositions/concat-effects/upgrade")
        .header("content-type", "application/json")
        .body(Body::from(conversion.to_string()))
        .unwrap();
    let (status, converted, _) = support::send(&app, request).await;
    assert_eq!(status, StatusCode::OK, "{converted}");
    let imported_effect = converted["nativeEffects"][0].clone();
    assert_eq!(imported_effect["kind"], "concat_chroma_key");

    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 32,
        fps_milli: 4000,
        ..Default::default()
    });
    composition.sources=serde_json::from_value(json!({"chroma-key-source":{"id":"chroma-key-source","kind":"video","durationTicks":500000,"width":64,"height":32,"hasAudio":false}})).unwrap();
    composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Screen","clips":[{"id":"keyed","sourceId":"chroma-key-source","placement":{"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":500000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","sourceAudioEnabled":false,"effects":[imported_effect]}]}])).unwrap();
    composition.validate().unwrap();
    assert!(video_kadr_backend::domain::flat_surface::requires_native_video(&composition));
    for keyed in [true, false] {
        let mut authored = composition.clone();
        if !keyed {
            let video_kadr_backend::domain::composition::CompositionTrack::Video { clips, .. } =
                &mut authored.tracks[0]
            else {
                panic!()
            };
            clips[0].effects.clear();
            clips[0].placement.timeline_start_tick = 0;
        }
        let range = if keyed {
            json!({"startTicks":1250000,"endTicks":1500000})
        } else {
            json!({"startTicks":250000,"endTicks":500000})
        };
        let body = json!({"schemaVersion":1,"composition":authored,"output":{"profile":{"container":"mp4","codec":"h264"},"range":range}});
        let request = Request::builder()
            .method("POST")
            .uri("/api/compositions/render")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let (status, accepted, _) = support::send(&app, request).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
        let done = terminal(&app, accepted["jobId"].as_str().unwrap()).await;
        assert_eq!(done["status"], "done", "keyed{keyed}: {done}");
        let output = state
            .outputs_dir()
            .join(done["result"]["filename"].as_str().unwrap());
        assert!(output.is_file());
        let pixels = decode(&output);
        assert_eq!(pixels.len(), 64 * 32 * 3);
        for y in 0..32 {
            for x in 4..60 {
                let p = &pixels[(y * 64 + x) * 3..(y * 64 + x + 1) * 3];
                if x < 24 {
                    if keyed {
                        assert!(
                            p.iter().all(|v| *v <= 4),
                            "keyed screen remains at{x},{y}: {p:?}"
                        );
                    } else {
                        assert!(
                            p.iter()
                                .zip(screen[..3].iter())
                                .all(|(a, b)| a.abs_diff(*b) <= 10),
                            "control lost green screen at{x},{y}: {p:?}"
                        );
                    }
                } else if x >= 40 {
                    assert!(
                        p.iter().all(|v| *v >= 248),
                        "foreground/selected clock lost at{x},{y}: {p:?}"
                    );
                }
            }
        }
    }
    println!(
        "Actual {legacy_id} export: migrated keyed/control H264 files and normalized source clock verified"
    );
}

#[tokio::test]
#[ignore = "requires actual FFmpeg/FFprobe/qtrle and authenticated preview"]
async fn chroma_key_saved_project_private_preview_and_revision_roundtrip() {
    use axum::body::to_bytes;
    use tower::ServiceExt;
    use video_kadr_backend::domain::composition::Rgba;
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("key-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("key-outsider", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let pixels: Vec<u8> = (0..16)
        .flat_map(|_| (0..16).flat_map(|x| if x < 8 { [0, 177, 64, 255] } else { [255; 4] }))
        .collect();
    let raw = state.sources_dir().join("private-key.rgba");
    std::fs::write(&raw, pixels).unwrap();
    let source = state.sources_dir().join("private-key.mov");
    command(&[
        "-nostdin",
        "-v",
        "error",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
        "-s",
        "16x16",
        "-r",
        "4",
        "-i",
        raw.to_str().unwrap(),
        "-c:v",
        "qtrle",
        "-pix_fmt",
        "argb",
        "-threads",
        "1",
        source.to_str().unwrap(),
    ]);
    state.library.add(MediaEntry::from_result("source",&json!({"id":"private-key","filename":"private-key.mov","url":"/files/sources/private-key.mov","mediaType":"video","width":16,"height":16,"duration":0.25,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
    let space = state
        .db
        .create_space(&owner.user.username, "Chroma source")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Source claim",
            &json!({"schemaVersion":1,"sources":{"private-key":{"id":"private-key"}}}),
            &["private-key".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let mut content = Composition::new(CanvasSpec {
        width: 16,
        height: 16,
        fps_milli: 4000,
        background: Rgba {
            alpha: 0.,
            ..Rgba::BLACK
        },
        ..Default::default()
    });
    content.sources=serde_json::from_value(json!({"private-key":{"id":"private-key","kind":"video","durationTicks":250000,"width":16,"height":16,"hasAudio":false}})).unwrap();
    content.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Key","clips":[{"id":"clip","sourceId":"private-key","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":250000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","sourceAudioEnabled":false,"effects":[{"kind":"concat_chroma_key","parameters":{"color":16711935,"similarity":1,"softness":0,"spill":0},"concatKeys":{"similarity":[{"at":0,"value":60},{"at":1,"value":1,"ease":"out"}]}}]}]}])).unwrap();
    let project = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":250000,"content":content,"instances":[]}}});
    let app = support::router(state.clone());
    let request = |method: &str, path: &str, body: Value, token: &str| {
        Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let save = |document: Value, revision: Option<u64>| json!({"schemaVersion":2,"mode":"composition","name":"Chroma Key","spaceId":space.id,"document":document,"baseRevision":revision});
    let (status, created, _) = support::send(
        &app,
        request(
            "POST",
            "/api/composition-projects",
            save(project.clone(), None),
            &owner.token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let path = format!(
        "/api/composition-projects/{}",
        created["id"].as_str().unwrap()
    );
    let (status, loaded, _) =
        support::send(&app, request("GET", &path, Value::Null, &owner.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(loaded["document"], project);
    let preview = |document: Value, token: &str| {
        request(
            "POST",
            "/api/compositions/nested-preview",
            json!({"project":document,"timeUs":0,"renderedOutputs":{}}),
            token,
        )
    };
    assert_eq!(
        app.clone()
            .oneshot(preview(project.clone(), &outsider.token))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let mut changed = loaded["document"].clone();
    changed["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]
        ["parameters"] = json!({"color":4294967295u32,"similarity":1,"softness":0,"spill":0});
    changed["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]
        .as_object_mut()
        .unwrap()
        .remove("concatKeys");
    for (document, white_key) in [(loaded["document"].clone(), false), (changed.clone(), true)] {
        let response = app
            .clone()
            .oneshot(preview(document, &owner.token))
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        assert_eq!(bytes.len(), 16 * 16 * 4);
        for y in 0..16 {
            for x in 0..16 {
                let p = &bytes[(y * 16 + x) * 4..(y * 16 + x + 1) * 4];
                let removed = if white_key { x >= 8 } else { x < 8 };
                assert_eq!(
                    p[3],
                    if removed { 0 } else { 255 },
                    "white_key{white_key} x{x} y{y}: {p:?}"
                );
                if !removed {
                    if white_key {
                        assert!(p[1] >= 170 && p[0] <= 2);
                    } else {
                        assert!(p[..3].iter().all(|v| *v >= 254));
                    }
                }
            }
        }
    }
    if let Ok(effect_path) = std::env::var("CONCAT_CHROMA_UI_EFFECT") {
        let effect: Value = serde_json::from_slice(&std::fs::read(effect_path).unwrap()).unwrap();
        assert_eq!(effect["kind"], "concat_chroma_key");
        changed["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0] = effect;
        let static_at = |similarity: f64| {
            let mut document = changed.clone();
            let effect = &mut document["compositions"]["root"]["content"]["tracks"][0]["clips"][0]
                ["effects"][0];
            effect["parameters"]["similarity"] = json!(similarity);
            effect.as_object_mut().unwrap().remove("concatKeys");
            document
        };
        // Symmetric x gives t0.5; custom y(-0.5,1) gives0.3125.
        // Incoming source key therefore yields60+(1-60)*0.3125 =41.5625.
        let mut frames = Vec::new();
        for document in [changed.clone(), static_at(41.5625), static_at(30.5)] {
            let response = app
                .clone()
                .oneshot(request(
                    "POST",
                    "/api/compositions/nested-preview",
                    json!({"project":document,"timeUs":125000,"renderedOutputs":{}}),
                    &owner.token,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            assert_eq!(bytes.len(), 16 * 16 * 4);
            frames.push(bytes);
        }
        assert_eq!(
            frames[0], frames[1],
            "UI easing midpoint must equal independent41.5625 similarity control"
        );
        assert_ne!(
            frames[0], frames[2],
            "UI easing must differ from linear30.5 similarity"
        );
        assert_eq!(frames[0][3], 0);
        assert_eq!(frames[2][3], 255);
        println!("UI-authored Chroma Key curve: actual private RGBA midpoint matches41.5625 control and differs from linear30.5");
    }
    let revision = loaded["revision"].as_u64().unwrap();
    let (status, updated, _) = support::send(
        &app,
        request(
            "PUT",
            &path,
            save(changed.clone(), Some(revision)),
            &owner.token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    let (status, reloaded, _) =
        support::send(&app, request("GET", &path, Value::Null, &owner.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reloaded["document"], changed);
    let (status, _, _) = support::send(
        &app,
        request("PUT", &path, save(project, Some(revision)), &owner.token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    println!("Chroma Key: private RGBA preview, unsigned white key, saved reload and revision conflict verified");
}
