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
    let mut b = Request::builder()
        .method("POST")
        .uri("/api/compositions/nested-preview")
        .header("content-type", "application/json");
    if let Some(token) = token {
        b = b.header("authorization", format!("Bearer {token}"));
    }
    b.body(Body::from(
        json!({"project":project,"timeUs":time,"renderedOutputs":{}}).to_string(),
    ))
    .unwrap()
}
fn command(args: &[&str]) -> Vec<u8> {
    let out = std::process::Command::new("ffmpeg")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
fn compare(actual: &[u8], expected: &[u8], label: &str) {
    assert_eq!(actual.len(), expected.len(), "{label}");
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        if i % 4 == 3 {
            assert_eq!(a, e, "{label} alpha {i}");
        } else {
            assert!(a.abs_diff(*e) <= 1, "{label} byte {i}: {a}/{e}");
        }
    }
}
#[tokio::test]
#[ignore = "requires actual FFmpeg/FFprobe and lossless QTRLE/FFV1"]
async fn actual_noir_private_nonuniform_preview_and_native_export() {
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("noir-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("noir-outsider", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/concat-noir-spatial.json")).unwrap();
    assert_eq!(
        oracle["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(oracle["upstreamProbes"], 1);
    assert_eq!(oracle["adapter"]["backend"], "Metal");
    let cases: Vec<&Value> = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["side"] == 16)
        .collect();
    assert_eq!(cases.len(), 8);
    let mut input = vec![];
    for seed in 0..2 {
        let case = cases.iter().find(|c| c["seed"] == seed).unwrap();
        input.extend(serde_json::from_value::<Vec<u8>>(case["inputRgba"].clone()).unwrap());
    }
    let raw = state.sources_dir().join("noir.rgba");
    std::fs::write(&raw, &input).unwrap();
    let movie = state.sources_dir().join("noir.mov");
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
    assert_eq!(
        command(&[
            "-nostdin",
            "-v",
            "error",
            "-i",
            movie.to_str().unwrap(),
            "-pix_fmt",
            "rgba",
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "pipe:1"
        ]),
        input,
        "QTRLE must retain colored transparent input"
    );
    state.library.add(MediaEntry::from_result("source",&json!({"id":"source","filename":"noir.mov","url":"/files/sources/noir.mov","mediaType":"video","width":16,"height":16,"duration":0.5,"fps":4,"vcodec":"qtrle"}))).await;
    let space = state
        .db
        .create_space(&owner.user.username, "Adjust source")
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
    composition.canvas.background.alpha = 0.;
    composition.sources=serde_json::from_value(json!({"source":{"id":"source","kind":"video","durationTicks":500000,"width":16,"height":16,"hasAudio":false}})).unwrap();
    composition.tracks=serde_json::from_value(json!([{"id":"v","kind":"video","name":"Video","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":500000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","sourceAudioEnabled":false,"effects":[{"kind":"concat_noir","parameters":{}}]}]}])).unwrap();
    composition.validate().unwrap();
    assert!(video_kadr_backend::domain::flat_surface::requires_native_video(&composition));
    let mut project = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":composition,"instances":[]}}});
    let app = support::router(state.clone());
    let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"concat.noir"}]}).to_string())).unwrap();
    let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");
    let converted_effect=converted["nativeEffects"][0].clone();assert_eq!(converted_effect,json!({"kind":"concat_noir","parameters":{}}));
    project["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]=converted_effect;

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
    let parameters: Vec<Value> = cases
        .iter()
        .filter(|c| c["seed"] == 0)
        .map(|c| c["parameters"].clone())
        .collect();
    assert_eq!(parameters.len(), 1);
    for (index, parameters) in parameters.iter().enumerate() {
        project["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]
            ["parameters"] = parameters.clone();
        let mut frames = vec![];
        for seed in 0..2 {
            let case = cases
                .iter()
                .find(|c| c["seed"] == seed && c["parameters"] == *parameters)
                .unwrap();
            let mut expected: Vec<u8> =
                serde_json::from_value(case["expectedRgba"].clone()).unwrap();
            for p in expected.chunks_exact_mut(4) {
                if p[3] == 0 {
                    p[..3].fill(0);
                }
            }
            let response = app
                .clone()
                .oneshot(request(&project, seed * 250000, Some(&owner.token)))
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
            compare(
                &bytes,
                &expected,
                &format!("preview grade {index} frame {seed}"),
            );
            frames.extend(expected);
        }
        // API source preparation may replace the original library path; resolve the current entry for export.
        let entry = state.library.get("source").await.unwrap();
        let source = state.library.resolve_media_path(&entry).await.unwrap();
        assert!(source.is_file());
        let output = state.outputs_dir().join(format!("noir-{index}.mkv"));
        let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
        video_kadr_backend::tools::native_rgba_export::encode_project_video_only(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            &output,
            serde_json::from_value::<NestedCompositionProject>(project.clone()).unwrap(),
            "root".into(),
            BTreeMap::new(),
            BTreeMap::from([(SourceId::parse("source").unwrap(), source.clone())]),
            0..500000,
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
        compare(&decoded, &frames, &format!("native FFV1 grade {index}"));
    }
    project["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]
        ["parameters"] = json!({"age":101});
    assert_eq!(
        app.clone()
            .oneshot(request(&project, 0, Some(&owner.token)))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    state.begin_shutdown();
    assert!(state.wait_for_tasks(Duration::from_secs(15)).await);
}
