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
    panic!("Gaussian Blur export did not finish")
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
async fn ordinary_gaussian_blur_export_samples_imported_box_keys_at_selected_source_clock() {
    let (state, _directory) = support::make_state(true, false).await;
    let mut input = Vec::new();
    for frame in 0..2 {
        for _y in 0..32 {
            for x in 0..64 {
                let value = if frame == 1 && x >= 32 { 255 } else { 0 };
                input.extend_from_slice(&[value, value, value, 255]);
            }
        }
    }
    let raw = state.sources_dir().join("gaussian_blur-delivery.rgba");
    std::fs::write(&raw, input).unwrap();
    let movie = state.sources_dir().join("gaussian_blur-delivery.mov");
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
    state.library.add(MediaEntry::from_result("source",&json!({"id":"gaussian_blur-source","filename":"gaussian_blur-delivery.mov","url":"/files/sources/gaussian_blur-delivery.mov","mediaType":"video","width":64,"height":32,"duration":0.5,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
    let app = support::router(state.clone());
    let middle_box_radius = (-1. + 1201f64.sqrt()) / 2.;
    let conversion = json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"box-blur","params":{"radius":1},"keys":{"radius":[{"at":0,"value":1},{"at":0.5,"value":middle_box_radius,"ease":"out"},{"at":1,"value":1,"ease":"inOut"}]}}]});
    let request = Request::builder()
        .method("POST")
        .uri("/api/compositions/concat-effects/upgrade")
        .header("content-type", "application/json")
        .body(Body::from(conversion.to_string()))
        .unwrap();
    let (status, converted, _) = support::send(&app, request).await;
    assert_eq!(status, StatusCode::OK, "{converted}");
    let imported_effect = converted["nativeEffects"][0].clone();
    assert_eq!(imported_effect["kind"], "concat_gaussian_blur");

    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 32,
        fps_milli: 4000,
        ..Default::default()
    });
    composition.sources=serde_json::from_value(json!({"gaussian_blur-source":{"id":"gaussian_blur-source","kind":"video","durationTicks":500000,"width":64,"height":32,"hasAudio":false}})).unwrap();
    composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Screen","clips":[{"id":"keyed","sourceId":"gaussian_blur-source","placement":{"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":500000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","sourceAudioEnabled":false,"effects":[imported_effect]}]}])).unwrap();
    composition.validate().unwrap();
    assert!(video_kadr_backend::domain::flat_surface::requires_native_video(&composition));
    for mode in ["normal", "control", "undershoot", "overshoot", "boundary"] {
        let keyed = mode != "control";
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
        if matches!(mode, "undershoot" | "overshoot" | "boundary") {
            let video_kadr_backend::domain::composition::CompositionTrack::Video { clips, .. } =
                &mut authored.tracks[0]
            else {
                panic!()
            };
            let effect = if mode == "boundary" {
                json!({"kind":"concat_gaussian_blur","parameters":{"radius":50}})
            } else {
                let y = if mode == "undershoot" { -2. } else { 20. };
                json!({"kind":"concat_gaussian_blur","parameters":{"radius":10},"concatKeys":[{"at":0,"value":1},{"at":1,"value":10,"ease":[0.42,y,0.58,y]}]})
            };
            clips[0].effects = vec![serde_json::from_value(effect).unwrap()];
        }
        authored.validate().unwrap();
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
        for y in 4..28 {
            let xs = if matches!(mode, "overshoot" | "boundary") {
                4..60
            } else {
                30..34
            };
            for x in xs {
                let p = &pixels[(y * 64 + x) * 3..(y * 64 + x + 1) * 3];
                match mode {
                    "normal" => assert!(
                        p.iter().all(|v| (160..=215).contains(v)),
                        "normal{x},{y}: {p:?}"
                    ),
                    "overshoot" => assert!(
                        p.iter().all(|v| (170..=205).contains(v)),
                        "unclamped137.125px{x},{y}: {p:?}"
                    ),
                    "boundary" if x == 4 => assert!(
                        p.iter().all(|v| *v <= 165),
                        "50px boundary black{x},{y}: {p:?}"
                    ),
                    "boundary" if x == 59 => assert!(
                        p.iter().all(|v| *v >= 215),
                        "50px boundary white{x},{y}: {p:?}"
                    ),
                    "undershoot" if x == 30 => {
                        assert!(p.iter().all(|v| *v <= 8), "sigma0.5 black{x},{y}: {p:?}")
                    }
                    "undershoot" if x == 33 => {
                        assert!(p.iter().all(|v| *v >= 247), "sigma0.5 white{x},{y}: {p:?}")
                    }
                    "control" if x < 32 => {
                        assert!(p.iter().all(|v| *v <= 4), "control black{x},{y}: {p:?}")
                    }
                    "control" => {
                        assert!(p.iter().all(|v| *v >= 248), "control white{x},{y}: {p:?}")
                    }
                    _ => {}
                }
            }
        }
    }
    println!(
        "Actual Gaussian Blur export: normal/control/negative/137.125px/50px H264 files, unclamped radius and selected source frame verified"
    );
}

#[tokio::test]
#[ignore = "requires actual FFmpeg/FFprobe/qtrle and authenticated preview"]
async fn gaussian_blur_saved_project_private_preview_and_revision_roundtrip() {
    use axum::body::to_bytes;
    use tower::ServiceExt;
    use video_kadr_backend::domain::composition::Rgba;
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("gaussian_blur-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("gaussian_blur-outsider", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let pixels: Vec<u8> = (0..16)
        .flat_map(|_| (0..16).flat_map(|x| if x < 8 { [0, 0, 0, 255] } else { [255; 4] }))
        .collect();
    let raw = state.sources_dir().join("private-gaussian_blur.rgba");
    std::fs::write(&raw, pixels).unwrap();
    let source = state.sources_dir().join("private-gaussian_blur.mov");
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
    state.library.add(MediaEntry::from_result("source",&json!({"id":"private-gaussian_blur","filename":"private-gaussian_blur.mov","url":"/files/sources/private-gaussian_blur.mov","mediaType":"video","width":16,"height":16,"duration":0.25,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
    let space = state
        .db
        .create_space(&owner.user.username, "Chroma source")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Source claim",
            &json!({"schemaVersion":1,"sources":{"private-gaussian_blur":{"id":"private-gaussian_blur"}}}),
            &["private-gaussian_blur".into()],
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
    content.sources=serde_json::from_value(json!({"private-gaussian_blur":{"id":"private-gaussian_blur","kind":"video","durationTicks":250000,"width":16,"height":16,"hasAudio":false}})).unwrap();
    content.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Key","clips":[{"id":"clip","sourceId":"private-gaussian_blur","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":250000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","sourceAudioEnabled":false,"effects":[{"kind":"concat_gaussian_blur","parameters":{}}]}]}])).unwrap();
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
    let save = |document: Value, revision: Option<u64>| json!({"schemaVersion":2,"mode":"composition","name":"Gaussian Blur","spaceId":space.id,"document":document,"baseRevision":revision});
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
        ["parameters"] = json!({"radius":10});
    changed["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]
        ["concatKeys"] = json!([{"at":0,"value":1,"ease":"out"},{"at":1,"value":10}]);
    if let Ok(path) = std::env::var("CONCAT_SOURCE_KEYS_UI_EFFECT") {
        let effect: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(effect["kind"], "concat_gaussian_blur");
        assert_eq!(effect["concatKeys"][0]["value"], 1);
        changed["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0] = effect;
        println!(
            "Actual private preview and save use the mounted source-key editor's exported effect"
        );
        // At progress0.5, this editor-authored symmetric-x curve has y0.59375:
        // radius1 + (10-1)*0.59375 =6.34375, independently of the native sampler.
        let mut static_mid = changed.clone();
        let effect = &mut static_mid["compositions"]["root"]["content"]["tracks"][0]["clips"][0]
            ["effects"][0];
        effect["parameters"] = json!({"radius":6.34375});
        effect.as_object_mut().unwrap().remove("concatKeys");
        let mut frames = Vec::new();
        for document in [changed.clone(), static_mid, project.clone()] {
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
            frames.push(to_bytes(response.into_body(), 1024 * 1024).await.unwrap());
        }
        assert_eq!(
            frames[0], frames[1],
            "UI source curve midpoint must equal independent6.34375px control"
        );
        assert_ne!(
            frames[0], frames[2],
            "UI source curve must not fall back to held10px radius"
        );
    }
    for (document, small_radius) in [(loaded["document"].clone(), false), (changed.clone(), true)] {
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
                assert_eq!(p[3], 255, "Gaussian Blur lost alpha at{x},{y}: {p:?}");
                if !small_radius && (7..=8).contains(&x) {
                    assert!(
                        p[..3].iter().all(|v| (175..=200).contains(v)),
                        "default blur at{x},{y}: {p:?}"
                    );
                } else if small_radius && x < 4 {
                    assert!(p[..3].iter().all(|v| *v <= 4), "1px black at{x},{y}: {p:?}");
                } else if small_radius && x >= 12 {
                    assert!(
                        p[..3].iter().all(|v| *v >= 250),
                        "1px white at{x},{y}: {p:?}"
                    );
                }
            }
        }
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
    println!("Gaussian Blur: private RGBA preview, radius changes, saved reload and revision conflict verified");
}
