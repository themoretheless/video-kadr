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
fn reference(input: &[u8], amount: f64) -> Vec<u8> {
    input
        .chunks_exact(4)
        .flat_map(|p| {
            let mut out = [0u8; 4];
            out[3] = 255;
            for c in 0..3 {
                let encoded = f64::from(p[c]) / 255.;
                let light = if encoded <= 0.04045 {
                    encoded / 12.92
                } else {
                    ((encoded + 0.055) / 1.055).powf(2.4)
                };
                let graded = if light <= 0. {
                    light
                } else {
                    (0.18 * (light.max(1e-10) / 0.18).powf(1. + amount / 100.)).min(65504.)
                };
                let display = if graded <= 0.0031308 {
                    12.92 * graded
                } else {
                    1.055 * graded.powf(1. / 2.4) - 0.055
                };
                out[c] = (display.clamp(0., 1.) * 255.).round() as u8;
            }
            out
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
async fn actual_animated_contrast_private_preview_and_native_export() {
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("concat-contrast-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("concat-contrast-outsider", "public synthetic password")
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
    let raw = state.sources_dir().join("contrast.rgba");
    std::fs::write(&raw, &input).unwrap();
    let movie = state.sources_dir().join("contrast.mov");
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
    state.library.add(MediaEntry::from_result("source",&json!({"id":"source","filename":"contrast.mov","url":"/files/sources/contrast.mov","mediaType":"video","width":16,"height":16,"duration":1.0,"fps":4,"vcodec":"qtrle"}))).await;
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
    composition.tracks=serde_json::from_value(json!([{"id":"v","kind":"video","name":"Video","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","sourceAudioEnabled":false,"effects":[{"kind":"concat_contrast","parameters":{},"animation":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":-100},{"tick":500000,"value":100},{"tick":1000000,"value":-100}]}}}]}]}])).unwrap();
    composition.validate().unwrap();
    assert!(video_kadr_backend::domain::flat_surface::requires_native_video(&composition));
    let mut project = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":composition,"instances":[]}}});
    let app = support::router(state.clone());
    let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"concat.contrast","params":{"amount":25.0}}]}).to_string())).unwrap();
    let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");
    let effect=converted["nativeEffects"][0].clone();assert_eq!(effect["kind"],"concat_contrast");assert_eq!(effect["parameters"]["amount"].as_f64(),Some(25.0));
    project["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]["parameters"]=effect["parameters"].clone();
    let mut static_project=project.clone();static_project["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0].as_object_mut().unwrap().remove("animation");
    let response=app.clone().oneshot(request(&static_project,0,Some(&owner.token))).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
    let bytes=to_bytes(response.into_body(),1024*1024).await.unwrap();let expected=reference(&input[..1024],25.0);compare(&bytes,&expected,"API-converted static control");assert_ne!(bytes.as_ref(),&input[..1024],"static source parameter must affect pixels");

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
    let mut source_cases=Vec::new();
    for(ease,end,amount)in [(json!([0.,0.,1.,1.]),100.,50.),(json!([0.42,-0.5,0.58,1.]),100.,31.25),(json!([0.42,3.,0.58,3.]),100.,237.5)]{
        let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"concat.contrast","params":{"amount":-1},"keys":{"amount":[{"at":0,"value":0},{"at":1,"value":end,"ease":ease}]}}]}).to_string())).unwrap();
        let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");source_cases.push((converted["nativeEffects"][0].clone(),amount));
    }
    if let Ok(path)=std::env::var("CONCAT_CONTRAST_UI_INPUT") {let effect:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(effect["kind"],"concat_contrast");assert_eq!(effect["concatKeys"].as_array().unwrap().len(),2);source_cases.push((effect,31.25));}
    for(case_index,(source_effect,amount))in source_cases.into_iter().enumerate(){let mut keyed=project.clone();let clip=&mut keyed["compositions"]["root"]["content"]["tracks"][0]["clips"][0];clip["placement"]["timelineStartTick"]=json!(1000000);clip["effects"][0]=source_effect;
        let response=app.clone().oneshot(request(&keyed,1500000,Some(&owner.token))).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let bytes=to_bytes(response.into_body(),1024*1024).await.unwrap();compare(&bytes,&reference(&input[2048..3072],amount),"normalized source midpoint at shifted placement");
        let entry=state.library.get("source").await.unwrap();let source=state.library.resolve_media_path(&entry).await.unwrap();let output=state.outputs_dir().join(format!("contrast-source-{case_index}-{amount}.mkv"));let(progress,_receiver)=tokio::sync::mpsc::unbounded_channel();
        video_kadr_backend::tools::native_rgba_export::encode_project_video_only(&ProcessRuntime::local_default(),Path::new("ffmpeg"),&output,serde_json::from_value::<NestedCompositionProject>(keyed.clone()).unwrap(),"root".into(),BTreeMap::new(),BTreeMap::from([(SourceId::parse("source").unwrap(),source)]),1500000..1750000,4*1024,Duration::from_secs(30),&tokio_util::sync::CancellationToken::new(),&progress).await.unwrap();
        let decoded=command(&["-nostdin","-v","error","-i",output.to_str().unwrap(),"-map","0:v:0","-an","-pix_fmt","rgba","-f","rawvideo","pipe:1"]);compare(&decoded,&reference(&input[2048..3072],amount),"normalized source FFV1 export");
        keyed["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]["animation"]=json!({"mode":"constant","value":0});assert_eq!(app.clone().oneshot(request(&keyed,1500000,Some(&owner.token))).await.unwrap().status(),StatusCode::BAD_REQUEST);
    }
    let mut frames = Vec::new();
    for (frame, amount) in [-100., 0., 100., 0.].into_iter().enumerate() {
        let expected = reference(&input[frame * 1024..(frame + 1) * 1024], amount);
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
        ["parameters"] = json!({"amount":100.1});
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
    let output = state.outputs_dir().join("contrast.mkv");
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
