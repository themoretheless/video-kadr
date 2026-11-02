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
// Independent f64 transfer-function / stop-distance reference. No native effect calls.
fn reference(input: &[u8], amount: f64, intensity: f64) -> Vec<u8> {
    fn display(v: f64) -> f64 {
        v.signum() * v.abs().powf(1.0 / 2.4)
    }
    fn linear(v: f64) -> f64 {
        v.signum() * v.abs().powf(2.4)
    }
    fn smooth(a: f64, b: f64, v: f64) -> f64 {
        let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
    input
        .chunks_exact(4)
        .flat_map(|p| {
            let light: [f64; 3] = std::array::from_fn(|i| {
                let v = f64::from(p[i]) / 255.0;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            });
            let rgb = light.map(display);
            let y = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
            let hi = rgb[0].max(rgb[1]).max(rgb[2]);
            let lo = rgb[0].min(rgb[1]).min(rgb[2]);
            let chroma = hi - lo;
            let factor = 1.0 + amount * (1.0 - chroma).max(0.0);
            let hue = if chroma < 0.0001 {
                0.0
            } else {
                let h = if hi == rgb[0] {
                    (rgb[1] - rgb[2]) / chroma
                } else if hi == rgb[1] {
                    2.0 + (rgb[2] - rgb[0]) / chroma
                } else {
                    4.0 + (rgb[0] - rgb[1]) / chroma
                };
                (h / 6.0).rem_euclid(1.0) * 360.0
            };
            let distance = (((hue - 25.0) / 360.0 + 0.5).rem_euclid(1.0) * 360.0 - 180.0).abs();
            let skin = (1.0 - smooth(20.0, 40.0, distance)) * smooth(0.0, 0.25, chroma);
            let graded: [f64; 3] = rgb.map(|v| v + (y + (v - y) * factor - v) * (1.0 - skin * 0.6));
            let mut output = [0u8; 4];
            output[3] = 255;
            for i in 0..3 {
                let v =
                    linear(graded[i]) * intensity / 100.0 + light[i] * (1.0 - intensity / 100.0);
                let encoded = if v <= 0.0031308 {
                    v * 12.92
                } else {
                    1.055 * v.powf(1.0 / 2.4) - 0.055
                };
                output[i] = (encoded.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            output
        })
        .collect()
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
async fn actual_animated_vibrance_private_preview_and_native_export() {
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("concat-vibrance-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("concat-vibrance-outsider", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
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
    let raw = state.sources_dir().join("vibrance.rgba");
    std::fs::write(&raw, &input).unwrap();
    let movie = state.sources_dir().join("vibrance.mov");
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
    state.library.add(MediaEntry::from_result("source",&json!({"id":"source","filename":"vibrance.mov","url":"/files/sources/vibrance.mov","mediaType":"video","width":16,"height":16,"duration":1.0,"fps":4,"vcodec":"qtrle"}))).await;
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
    composition.tracks=serde_json::from_value(json!([{"id":"v","kind":"video","name":"Video","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","sourceAudioEnabled":false,"effects":[{"kind":"concat_vibrance","parameters":{},"animation":{"amount":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.1},{"tick":500000,"value":2.0},{"tick":1000000,"value":0.1}]}},"intensity":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":100.0},{"tick":500000,"value":0.0},{"tick":1000000,"value":100.0}]}}}}]}]}])).unwrap();
    composition.validate().unwrap();
    assert!(video_kadr_backend::domain::flat_surface::requires_native_video(&composition));
    let mut project = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":composition,"instances":[]}}});
    let app = support::router(state.clone());
    let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"concat.vibrance","params":{"amount":0.75}}]}).to_string())).unwrap();
    let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");
    let effect=converted["nativeEffects"][0].clone();assert_eq!(effect["kind"],"concat_vibrance");assert_eq!(effect["parameters"]["amount"].as_f64(),Some(0.75));
    project["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]["parameters"]=effect["parameters"].clone();
    let mut static_project=project.clone();static_project["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0].as_object_mut().unwrap().remove("animation");
    let response=app.clone().oneshot(request(&static_project,0,Some(&owner.token))).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
    let bytes=to_bytes(response.into_body(),1024*1024).await.unwrap();let expected=reference(&input[..1024],0.75,100.);compare(&bytes,&expected,"API-converted static control");assert_ne!(bytes.as_ref(),&input[..1024],"static source parameter must affect pixels");

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
    for (frame, amount) in [0.1, 1.05, 2.0, 1.05].into_iter().enumerate() {
        let expected = reference(
            &input[frame * 1024..(frame + 1) * 1024],
            amount,
            [100.0, 50.0, 0.0, 50.0][frame],
        );
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
        ["parameters"] = json!({"amount":2.01});
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
    let output = state.outputs_dir().join("vibrance.mkv");
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
