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
    panic!("Glow export did not finish")
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
async fn ordinary_glow_export_spreads_light_at_selected_source_clock() {
    let (state, _directory) = support::make_state(true, false).await;
    let mut input = Vec::new();
    for frame in 0..2 {
        for _y in 0..32 {
            for _x in 0..64 {
                let value = if frame == 1 && (24..40).contains(&_x) {
                    255
                } else {
                    0
                };
                input.extend_from_slice(&[value, value, value, 255]);
            }
        }
    }
    let raw = state.sources_dir().join("glow-delivery.rgba");
    std::fs::write(&raw, input).unwrap();
    let movie = state.sources_dir().join("glow-delivery.mov");
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
    state.library.add(MediaEntry::from_result("source",&json!({"id":"glow-source","filename":"glow-delivery.mov","url":"/files/sources/glow-delivery.mov","mediaType":"video","width":64,"height":32,"duration":0.5,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
    let app = support::router(state.clone());
    let migration=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"concat.glow","params":{"amount":10},"keys":{"amount":[{"at":0,"value":10},{"at":1,"value":80}]}}]}).to_string())).unwrap();
    let (status, migrated, _) = support::send(&app, migration).await;
    assert_eq!(status, StatusCode::OK, "{migrated}");
    let native_effect = migrated["nativeEffects"][0].clone();
    assert_eq!(native_effect["kind"], "concat_glow");
    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 32,
        fps_milli: 4000,
        ..Default::default()
    });
    composition.sources=serde_json::from_value(json!({"glow-source":{"id":"glow-source","kind":"video","durationTicks":500000,"width":64,"height":32,"hasAudio":false}})).unwrap();
    composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Screen","clips":[{"id":"keyed","sourceId":"glow-source","placement":{"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":500000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","sourceAudioEnabled":false,"effects":[native_effect]}]}])).unwrap();
    composition.validate().unwrap();
    assert!(video_kadr_backend::domain::flat_surface::requires_native_video(&composition));
    let mut filtered = None;
    for mode in 0..3 {
        let keyed = mode < 2;
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
        if mode == 1 {
            let video_kadr_backend::domain::composition::CompositionTrack::Video { clips, .. } =
                &mut authored.tracks[0]
            else {
                panic!()
            };
            let video_kadr_backend::domain::composition::VideoEffect::ConcatGlow {
                parameters,
                concat_keys,
            } = &mut clips[0].effects[0]
            else {
                panic!()
            };
            parameters.amount = 45.;
            *concat_keys = None;
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
        if mode == 0 {
            filtered = Some(pixels.clone());
        }
        if mode == 1 {
            assert_eq!(
                filtered.as_ref().unwrap(),
                &pixels,
                "animated midpoint must equal static45 H264 control"
            );
        }
        let at = |x: usize, y: usize| &pixels[(y * 64 + x) * 3..(y * 64 + x + 1) * 3];
        assert!(
            at(32, 16).iter().all(|v| *v >= 240),
            "selected source white centre {:?}",
            at(32, 16)
        );
        if keyed {
            assert!(
                at(16, 16).iter().all(|v| *v >= 25),
                "missing off-stripe halo {:?}",
                at(16, 16)
            );
        } else {
            assert!(
                at(16, 16).iter().all(|v| *v <= 10),
                "unfiltered black control {:?}",
                at(16, 16)
            );
        }
    }
    println!(
        "Actual Glow export: filtered/control H264 files, spatial halo and selected source frame verified"
    );
}

#[tokio::test]
#[ignore = "requires actual FFmpeg/FFprobe/qtrle and authenticated preview"]
async fn glow_saved_project_private_preview_and_revision_roundtrip() {
    use axum::body::to_bytes;
    use tower::ServiceExt;
    use video_kadr_backend::domain::composition::Rgba;
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("glow-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("glow-outsider", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let pixels: Vec<u8> = (0..16)
        .flat_map(|y| {
            (0..16).flat_map(move |x| {
                if (4..12).contains(&x) && (4..12).contains(&y) {
                    [255; 4]
                } else {
                    [0, 0, 0, 255]
                }
            })
        })
        .collect();
    let raw = state.sources_dir().join("private-glow.rgba");
    std::fs::write(&raw, pixels).unwrap();
    let source = state.sources_dir().join("private-glow.mov");
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
    state.library.add(MediaEntry::from_result("source",&json!({"id":"private-glow","filename":"private-glow.mov","url":"/files/sources/private-glow.mov","mediaType":"video","width":16,"height":16,"duration":0.25,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
    let space = state
        .db
        .create_space(&owner.user.username, "Chroma source")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Source claim",
            &json!({"schemaVersion":1,"sources":{"private-glow":{"id":"private-glow"}}}),
            &["private-glow".into()],
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
    content.sources=serde_json::from_value(json!({"private-glow":{"id":"private-glow","kind":"video","durationTicks":250000,"width":16,"height":16,"hasAudio":false}})).unwrap();
    content.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Key","clips":[{"id":"clip","sourceId":"private-glow","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":250000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","sourceAudioEnabled":false,"effects":[{"kind":"concat_glow","parameters":{}}]}]}])).unwrap();
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
    let save = |document: Value, revision: Option<u64>| json!({"schemaVersion":2,"mode":"composition","name":"Glow","spaceId":space.id,"document":document,"baseRevision":revision});
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
        ["parameters"] = json!({"amount":10});
    for (document, small_blocks) in [(loaded["document"].clone(), false), (changed.clone(), true)] {
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
                assert_eq!(p[3], 255, "Glow lost alpha at{x},{y}: {p:?}");
                if x == 0 && y == 0 {
                    if !small_blocks {
                        assert!(p[..3].iter().all(|v| *v >= 8), "45 halo corner {p:?}");
                    } else {
                        assert!(p[..3].iter().all(|v| *v <= 5), "10 halo control {p:?}");
                    }
                }
                if x == 8 && y == 8 {
                    assert!(
                        p[..3].iter().all(|v| *v >= 254),
                        "white source centre {p:?}"
                    );
                }
            }
        }
    }
    // Compare actual native image at original normalized midpoint with an
    // independently calculated static control; a rounded/linear curve differs.
    let mut animated = loaded["document"].clone();
    let effect_path =
        &mut animated["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0];
    effect_path["parameters"] = json!({"amount":10});
    effect_path["concatKeys"] =
        json!([{"at":0,"value":10},{"at":1,"value":100,"ease":[0.42,-0.5,0.58,1]}]);
    if let Ok(path) = std::env::var("CONCAT_GLOW_UI_EFFECT") {
        let effect: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(effect["kind"], "concat_glow");
        animated["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0] = effect;
    }
    let mut images = Vec::new();
    for shift in [None, Some(38.125), Some(55.)] {
        let mut document = animated.clone();
        if let Some(shift) = shift {
            document["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0] =
                json!({"kind":"concat_glow","parameters":{"amount":shift}});
        }
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
        images.push(to_bytes(response.into_body(), 1024 * 1024).await.unwrap());
    }
    assert_eq!(
        images[0], images[1],
        "source curve midpoint must be38.125px"
    );
    assert_ne!(images[0], images[2], "linear midpoint55. must differ");
    changed = animated;
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
    println!("Glow: private RGBA preview, glow-amount changes, saved reload and revision conflict verified");
}
