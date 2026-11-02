mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, time::Duration};
use tower::ServiceExt;
use video_kadr_backend::{
    domain::{
        composition::{CanvasSpec, Composition, SourceId, TransformSpec},
        nested_composition::NestedCompositionProject,
    },
    library::MediaEntry,
    process_control::ProcessRuntime,
};

fn request(project: &Value, time: u64, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/compositions/nested-preview")
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder
        .body(Body::from(
            json!({"project":project,"timeUs":time,"renderedOutputs":{}}).to_string(),
        ))
        .unwrap()
}
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
fn compare(actual: &[u8], expected: &[u8], label: &str) {
    assert_eq!(actual.len(), expected.len(), "{label}");
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        if i % 4 == 3 {
            assert_eq!(a, e, "{label} alpha byte{i}");
        }
        assert!(a.abs_diff(*e) <= 1, "{label} byte{i}: {a} != {e}");
    }
}

#[tokio::test]
#[ignore = "requires actual FFmpeg/FFprobe and lossless QTRLE/FFV1 codecs"]
async fn actual_curves_private_preview_and_native_export_match_original_metal() {
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("concat-curves-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("concat-curves-outsider", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/concat-curves-media.json")).unwrap();
    assert_eq!(
        oracle["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(oracle["upstreamProbes"], 6);
    assert_eq!(oracle["adapter"]["backend"], "Metal");
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 4);
    let input: Vec<u8> = (0..4u32)
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
    let raw = state.sources_dir().join("curves.rgba");
    std::fs::write(&raw, &input).unwrap();
    let movie = state.sources_dir().join("curves.mov");
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
        movie.to_str().unwrap(),
    ]);
    state.library.add(MediaEntry::from_result("source",&json!({"id":"source","filename":"curves.mov","url":"/files/sources/curves.mov","mediaType":"video","width":16,"height":16,"duration":1.0,"fps":4,"vcodec":"qtrle"}))).await;
    let space = state
        .db
        .create_space(&owner.user.username, "Contrast source")
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
    let mut composition = Composition::new(CanvasSpec {
        width: 16,
        height: 16,
        fps_milli: 4000,
        ..Default::default()
    });
    composition.sources=serde_json::from_value(json!({"source":{"id":"source","kind":"video","durationTicks":1000000,"width":16,"height":16,"hasAudio":false}})).unwrap();
    composition.tracks=serde_json::from_value(json!([{"id":"v","kind":"video","name":"Video","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","sourceAudioEnabled":false,"effects":[{"kind":"concat_curves","parameters":oracle["parameters"]}]}]}])).unwrap();
    composition.validate().unwrap();
    assert!(video_kadr_backend::domain::flat_surface::requires_native_video(&composition));
    let mut project = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":composition,"instances":[]}}});
    let app = support::router(state.clone());
    let native_parameters=project["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]["parameters"].clone();
    let mut source_parameters=serde_json::Map::new();
    for channel in ["luma","red","green","blue"]{if let Some(points)=native_parameters[channel].as_array(){for(index,point)in points.iter().enumerate(){source_parameters.insert(format!("{channel}.{index}.x"),point[0].clone());source_parameters.insert(format!("{channel}.{index}.y"),point[1].clone());}}}
    let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"concat.curves","params":source_parameters}]}).to_string())).unwrap();
    let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");let effect=converted["nativeEffects"][0].clone();assert_eq!(effect["kind"],"concat_curves");
    project["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]=effect;

    assert_eq!(
        app.clone()
            .oneshot(request(&project, 0, None))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(request(&project, 0, Some(&outsider.token)))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let mut frames = Vec::new();
    for (frame, case) in oracle["cases"].as_array().unwrap().iter().enumerate() {
        let raw: Vec<u8> = serde_json::from_value(case["inputRgba"].clone()).unwrap();
        assert_eq!(raw, &input[frame * 1024..(frame + 1) * 1024]);
        let expected: Vec<u8> = serde_json::from_value(case["expectedRgba"].clone()).unwrap();
        assert_eq!(expected.len(), 1024);
        let response = app
            .clone()
            .oneshot(request(&project, frame as u64 * 250000, Some(&owner.token)))
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
        compare(&bytes, &expected, &format!("preview frame{frame}"));
        frames.extend(expected);
    }
    let mut invalid = project.clone();
    invalid["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]
        ["parameters"] = json!({"luma":vec![[0.0,0.0];9]});
    assert_eq!(
        app.clone()
            .oneshot(request(&invalid, 0, Some(&owner.token)))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let entry = state.library.get("source").await.unwrap();
    let source = state.library.resolve_media_path(&entry).await.unwrap();
    assert!(source.is_file());
    let output = state.outputs_dir().join("curves.mkv");
    let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
    video_kadr_backend::tools::native_rgba_export::encode_project_video_only(
        &ProcessRuntime::local_default(),
        Path::new("ffmpeg"),
        &output,
        serde_json::from_value::<NestedCompositionProject>(project).unwrap(),
        "root".into(),
        BTreeMap::new(),
        BTreeMap::from([(SourceId::parse("source").unwrap(), source)]),
        0..1000000,
        4 * 1024,
        Duration::from_secs(30),
        &tokio_util::sync::CancellationToken::new(),
        &progress,
    )
    .await
    .unwrap();
    let decoded = command(&[
        "-nostdin",
        "-v",
        "error",
        "-i",
        output.to_str().unwrap(),
        "-map",
        "0:v:0",
        "-an",
        "-pix_fmt",
        "rgba",
        "-fps_mode",
        "passthrough",
        "-f",
        "rawvideo",
        "pipe:1",
    ]);
    compare(&decoded, &frames, "native FFV1 export");
    state.begin_shutdown();
    assert!(state.wait_for_tasks(Duration::from_secs(15)).await);
}

#[tokio::test]
async fn concat_curve_graph_samples_are_bounded_and_use_native_curve_rules() {
    let (state, _dir) = support::make_state(true, false).await;
    let app = support::router(state.clone());
    let make = |body: Value| {
        Request::builder()
            .method("POST")
            .uri("/api/compositions/concat-curve")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let response = app
        .clone()
        .oneshot(make(json!({"points":[[0.2,0.3],[0.8,0.7]],"samples":3})))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    for (actual, expected) in result["values"]
        .as_array()
        .unwrap()
        .iter()
        .zip([0.3, 0.5, 0.7])
    {
        assert!((actual.as_f64().unwrap() - expected).abs() < 1e-6);
    }
    for body in [
        json!({"points":[],"samples":1}),
        json!({"points":[],"samples":4097}),
        json!({"points":vec![[0.0,0.0];9],"samples":256}),
        json!({"points":[[-0.1,0]],"samples":256}),
        json!({"points":[],"samples":256,"periodic":true}),
    ] {
        assert_eq!(
            app.clone().oneshot(make(body)).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        app.oneshot(make(json!({"points":vec![[0.0,0.0];9000],"samples":256})))
            .await
            .unwrap()
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    state.begin_shutdown();
    assert!(state.wait_for_tasks(Duration::from_secs(15)).await);
}
