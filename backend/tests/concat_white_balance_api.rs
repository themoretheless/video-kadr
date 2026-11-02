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
// Independent f64 XYZ/Bradford reference; no native effect/working-frame calls.
fn reference(input: &[u8], temperature: f64, tint: f64) -> Vec<u8> {
    fn mv(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
        m.map(|r| r.into_iter().zip(v).map(|(a, b)| a * b).sum())
    }
    const XYZ: [[f64; 3]; 3] = [
        [0.4123907992659595, 0.357584339383878, 0.1804807884018343],
        [0.2126390058715104, 0.715168678767756, 0.0721923153607337],
        [0.0193308187155919, 0.119194779794626, 0.9505321522496607],
    ];
    const RGB: [[f64; 3]; 3] = [
        [3.2409699419045226, -1.537383177570094, -0.4986107602930034],
        [-0.9692436362808796, 1.8759675015077202, 0.0415550574071756],
        [0.0556300796969937, -0.2039769588889765, 1.0569715142428786],
    ];
    const BRADFORD: [[f64; 3]; 3] = [
        [0.8951, 0.2664, -0.1614],
        [-0.7502, 1.7135, 0.0367],
        [0.0389, -0.0685, 1.0296],
    ];
    const INVERSE: [[f64; 3]; 3] = [
        [0.9869929054667123, -0.1470542564209901, 0.1599626516637312],
        [0.4323052697233945, 0.5183602715367776, 0.0492912282128556],
        [-0.0085286645751773, 0.0400428216540849, 0.96848669578755],
    ];
    fn uv(t: f64) -> [f64; 2] {
        let k = 1000. / t;
        let x = if t <= 4000. {
            -0.2661239 * k.powi(3) - 0.2343589 * k.powi(2) + 0.8776956 * k + 0.179910
        } else {
            -3.0258469 * k.powi(3) + 2.1070379 * k.powi(2) + 0.2226347 * k + 0.240390
        };
        let y = if t <= 2222. {
            -1.1063814 * x.powi(3) - 1.34811020 * x.powi(2) + 2.18555832 * x - 0.20219683
        } else if t <= 4000. {
            -0.9549476 * x.powi(3) - 1.37418593 * x.powi(2) + 2.09137015 * x - 0.16748867
        } else {
            3.0817580 * x.powi(3) - 5.87338670 * x.powi(2) + 3.75112997 * x - 0.37001483
        };
        let d = -2. * x + 12. * y + 3.;
        [4. * x / d, 6. * y / d]
    }
    fn cone(t: f64, tint: f64) -> [f64; 3] {
        let [u, v] = uv(t);
        let next = uv(t * 1.01);
        let dx = next[0] - u;
        let dy = next[1] - v;
        let length = dx.hypot(dy);
        let sign = if -dx > 0. { -1. } else { 1. };
        let u = u + sign * dy / length * tint * 0.0007;
        let v = v - sign * dx / length * tint * 0.0007;
        let d = 2. * u - 8. * v + 4.;
        let x = 3. * u / d;
        let y = 2. * v / d;
        mv(BRADFORD, [x / y, 1., (1. - x - y) / y])
    }
    let target = cone(temperature, tint);
    let base = cone(6500., 0.);
    let gains: [f64; 3] = std::array::from_fn(|i| target[i] / base[i]);
    let adapt = |rgb: [f64; 3]| {
        let c = mv(BRADFORD, mv(XYZ, rgb));
        mv(RGB, mv(INVERSE, std::array::from_fn(|i| c[i] * gains[i])))
    };
    let grey = adapt([1.; 3]);
    let y = grey[0] * 0.2126 + grey[1] * 0.7152 + grey[2] * 0.0722;
    input
        .chunks_exact(4)
        .flat_map(|p| {
            let light = std::array::from_fn(|i| {
                let v = f64::from(p[i]) / 255.;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            });
            let graded = adapt(light).map(|v| v / y);
            let mut output = [0u8; 4];
            output[3] = 255;
            for i in 0..3 {
                let v = graded[i];
                let encoded = if v <= 0.0031308 {
                    v * 12.92
                } else {
                    1.055 * v.powf(1. / 2.4) - 0.055
                };
                output[i] = (encoded.clamp(0., 1.) * 255.).round() as u8;
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
async fn actual_animated_white_balance_private_preview_and_native_export() {
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("concat-white_balance-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("concat-white_balance-outsider", "public synthetic password")
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
    let raw = state.sources_dir().join("white_balance.rgba");
    std::fs::write(&raw, &input).unwrap();
    let movie = state.sources_dir().join("white_balance.mov");
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
    state.library.add(MediaEntry::from_result("source",&json!({"id":"source","filename":"white_balance.mov","url":"/files/sources/white_balance.mov","mediaType":"video","width":16,"height":16,"duration":1.0,"fps":4,"vcodec":"qtrle"}))).await;
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
    composition.tracks=serde_json::from_value(json!([{"id":"v","kind":"video","name":"Video","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","sourceAudioEnabled":false,"effects":[{"kind":"concat_white_balance","parameters":{},"animation":{"temperature":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":2000},{"tick":500000,"value":11000},{"tick":1000000,"value":2000}]}},"tint":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":-100},{"tick":500000,"value":100},{"tick":1000000,"value":-100}]}}}}]}]}])).unwrap();
    composition.validate().unwrap();
    assert!(video_kadr_backend::domain::flat_surface::requires_native_video(&composition));
    let mut project = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":composition,"instances":[]}}});
    let app = support::router(state.clone());
    let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(r#"{"schemaVersion": 1, "includeNativeEffects": true, "links": [{"id": "concat.white-balance", "params": {"temperature": 8500, "tint": 25}}]}"#)).unwrap();
    let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");
    let effect=converted["nativeEffects"][0].clone();assert_eq!(effect["kind"],"concat_white_balance");
    project["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]["parameters"]=effect["parameters"].clone();
    let mut static_project=project.clone();static_project["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0].as_object_mut().unwrap().remove("animation");
    let response=app.clone().oneshot(request(&static_project,0,Some(&owner.token))).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let bytes=to_bytes(response.into_body(),1024*1024).await.unwrap();let expected=reference(&input[..1024],8500.,25.);compare(&bytes,&expected,"API-converted static control");assert_ne!(bytes.as_ref(),&input[..1024]);

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
    for(ease,tint_end,temperature,tint)in [(json!([0.,0.,1.,1.]),100.,7000.,50.),(json!([0.42,-0.5,0.58,1.]),100.,6812.5,31.25),(json!([0.42,3.,0.58,3.]),100.,8875.,237.5)]{
        let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"concat.white-balance","keys":{"temperature":[{"at":0,"value":6500},{"at":1,"value":7500,"ease":ease}],"tint":[{"at":0,"value":0},{"at":1,"value":tint_end,"ease":ease}]}}]}).to_string())).unwrap();
        let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");source_cases.push((converted["nativeEffects"][0].clone(),temperature,tint));
    }
    if let Ok(path)=std::env::var("CONCAT_WHITE_BALANCE_UI_INPUT"){let effect:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(effect["kind"],"concat_white_balance");source_cases.push((effect,6812.5,0.));}
    for(case_index,(source_effect,temperature,tint))in source_cases.into_iter().enumerate(){let mut keyed=project.clone();let clip=&mut keyed["compositions"]["root"]["content"]["tracks"][0]["clips"][0];clip["placement"]["timelineStartTick"]=json!(1000000);clip["effects"][0]=source_effect;
        let response=app.clone().oneshot(request(&keyed,1500000,Some(&owner.token))).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let bytes=to_bytes(response.into_body(),1024*1024).await.unwrap();compare(&bytes,&reference(&input[2048..3072],temperature,tint),"normalized source midpoint at shifted placement");
        let entry=state.library.get("source").await.unwrap();let source=state.library.resolve_media_path(&entry).await.unwrap();let output=state.outputs_dir().join(format!("white-balance-source-{case_index}-{temperature}.mkv"));let(progress,_receiver)=tokio::sync::mpsc::unbounded_channel();
        video_kadr_backend::tools::native_rgba_export::encode_project_video_only(&ProcessRuntime::local_default(),Path::new("ffmpeg"),&output,serde_json::from_value::<NestedCompositionProject>(keyed.clone()).unwrap(),"root".into(),BTreeMap::new(),BTreeMap::from([(SourceId::parse("source").unwrap(),source)]),1500000..1750000,4*1024,Duration::from_secs(30),&tokio_util::sync::CancellationToken::new(),&progress).await.unwrap();
        let decoded=command(&["-nostdin","-v","error","-i",output.to_str().unwrap(),"-map","0:v:0","-an","-pix_fmt","rgba","-f","rawvideo","pipe:1"]);compare(&decoded,&reference(&input[2048..3072],temperature,tint),"normalized source FFV1 export");
        keyed["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]["animation"]=json!({"tint":{"mode":"constant","value":0}});assert_eq!(app.clone().oneshot(request(&keyed,1500000,Some(&owner.token))).await.unwrap().status(),StatusCode::BAD_REQUEST);
    }
    let mut frames = Vec::new();
    for (frame, (temperature, tint)) in [(2000., -100.), (6500., 0.), (11000., 100.), (6500., 0.)]
        .into_iter()
        .enumerate()
    {
        let expected = reference(&input[frame * 1024..(frame + 1) * 1024], temperature, tint);
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
        ["parameters"] = json!({"temperature":11001,"tint":0});
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
    let output = state.outputs_dir().join("white_balance.mkv");
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
