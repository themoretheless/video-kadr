mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use video_kadr_backend::{
    domain::composition::{Composition, FrameRateRatio, TransformSpec},
    library::MediaEntry,
    process_control::ProcessRuntime,
    services::local_person_mask_video::{self, Input, Tools},
};
fn request(path: &str, token: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}
#[tokio::test]
#[ignore = "requires host FFmpeg/FFprobe; synthetic saved SAM region, no model inference"]
async fn saved_semantic_camera_native_http_ownership_identity_ffv1_and_stale_revision() {
    let (state, _root) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("outsider", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let mut raw = vec![0; 3 * 256 * 256];
    for frame in 0..3 {
        for y in 120..136 {
            for dx in 0..4 {
                let x = (254 + frame * 2 + dx) % 256;
                raw[frame * 256 * 256 + y * 256 + x] = 255;
            }
        }
    }
    let stage = state.storage.join("staging/regions.raw");
    std::fs::write(&stage, &raw).unwrap();
    let movie = state.storage.join("sources/region.mkv");
    let mask_hash = format!("{:x}", Sha256::digest(&raw));
    let cancel = tokio_util::sync::CancellationToken::new();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let ffmpeg = std::path::PathBuf::from("/opt/homebrew/bin/ffmpeg");
    let ffprobe = std::path::PathBuf::from("/opt/homebrew/bin/ffprobe");
    let metadata = local_person_mask_video::encode(
        &ProcessRuntime::local_default(),
        Tools {
            ffmpeg: &ffmpeg,
            ffprobe: &ffprobe,
        },
        Input {
            masks: &stage,
            mask_hash: &mask_hash,
            width: 256,
            height: 256,
            frames: 3,
            frame_rate: FrameRateRatio {
                numerator: 10,
                denominator: 1,
            },
            staging_bytes: 16 * 1024 * 1024,
        },
        &movie,
        &cancel,
        std::time::Duration::from_secs(30),
        &tx,
    )
    .await
    .unwrap();
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"region","filename":"region.mkv","url":"/files/sources/region.mkv","mediaType":"video","width":256,"height":256}))).await);
    let points = vec![[0.99, 0.5]];
    let region = json!({"sourceId":"raw","maskSourceId":"region","sourceHash":"a".repeat(64),"maskHash":mask_hash,"maskFileHash":metadata.file_hash,"encoderHash":"b".repeat(64),"decoderHash":"c".repeat(64),"runtimeHash":"d".repeat(64),"promptHash":format!("{:x}",Sha256::digest(serde_json::to_vec(&points).unwrap())),"points":points,"width":64,"height":32,"frames":3,"frameRate":{"numerator":10,"denominator":1},"frameTimesUs":[0,100000,400000,500000],"seedFrame":1,"trackedFrames":3,"forwardLostAt":null,"backwardLostAt":null});
    let mut document = serde_json::to_value(Composition::new(Default::default())).unwrap();
    document["sources"] = json!({"raw":{"id":"raw","kind":"video","width":64,"height":32,"durationTicks":500000,"hasAudio":false},"copy":{"id":"copy","kind":"video","width":64,"height":32,"durationTicks":500000,"hasAudio":false},"automatic":{"id":"automatic","kind":"video","width":512,"height":256,"durationTicks":500000,"hasAudio":false},"region":{"id":"region","kind":"video","width":256,"height":256,"durationTicks":500000,"hasAudio":false}});
    document["tracks"] = json!([{"kind":"video","id":"videos","name":"Videos","clips":[{"id":"clip","sourceId":"raw","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":500000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal"}]}]);
    document["metadata"] = json!({"mediaVersions":{"clip":{"originalSourceId":"raw","copySourceId":"copy","videoCutout":{"sourceId":"raw","maskSourceId":"automatic","sourceHash":"a".repeat(64),"maskHash":"e".repeat(64),"maskFileHash":"f".repeat(64),"width":64,"height":32,"maskWidth":512,"maskHeight":256,"frames":3,"frameRate":{"numerator":10,"denominator":1},"corrections":[],"trackedRegions":[region]}}}});
    let composition: Composition = serde_json::from_value(document.clone()).unwrap();
    composition.validate().unwrap();
    let project = state
        .db
        .create_owned_composition_project(
            "Semantic camera",
            &document,
            &document["sources"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            "owner",
        )
        .await
        .unwrap();
    let path = format!(
        "/api/composition-projects/{}/spherical-mask-track",
        project.id
    );
    let body = json!({"clipId":"clip","expectedRevision":project.revision,"region":region,"sourceTimesUs":[50000,200000,450000]});
    let app = support::router(state.clone());
    assert_eq!(
        app.clone()
            .oneshot(request(&path, &outsider.token, &body))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.clone()
            .oneshot(request(&path, "", &body))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = app
        .clone()
        .oneshot(request(&path, &owner.token, &body))
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
    let result: Value = serde_json::from_slice(&bytes).unwrap();
    let centres = result["centres"].as_array().unwrap();
    assert_eq!(centres.len(), 3);
    assert!((centres[0]["pan"].as_f64().unwrap().abs() - 180.).abs() < 1e-8);
    assert!((centres[1]["pan"].as_f64().unwrap() + 177.1875).abs() < 1e-8);
    assert!(centres.iter().all(|c| c.get("fov").is_none()));
    let mut gyro = body.clone();
    gyro["spherical"] = json!({"orientation":[{"timeSeconds":0.,"quaternion":[1.,0.,0.,0.]},{"timeSeconds":0.2,"quaternion":[std::f64::consts::FRAC_1_SQRT_2,0.,std::f64::consts::FRAC_1_SQRT_2,0.]}]});
    gyro["parameterStartUs"] = json!(50000);
    let response = app
        .clone()
        .oneshot(request(&path, &owner.token, &gyro))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let measured: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert!((measured["centres"][0]["pan"].as_f64().unwrap().abs() - 180.).abs() < 1e-8);
    assert!(
        (measured["centres"][1]["pan"].as_f64().unwrap() + 109.6875).abs() < 1e-6,
        "{measured}"
    );
    gyro["parameterStartUs"] = json!(60000);
    assert_eq!(
        app.clone()
            .oneshot(request(&path, &owner.token, &gyro))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let mut raw = body.clone();
    raw["spherical"] = json!({"input":"dual_fisheye"});
    assert_eq!(
        app.clone()
            .oneshot(request(&path, &owner.token, &raw))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let mut framed = body.clone();
    framed["framing"] = json!({"aspectRatio":16.0/9.0,"targetFill":0.65,"initialFov":90.});
    let response = app
        .clone()
        .oneshot(request(&path, &owner.token, &framed))
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
    let result: Value = serde_json::from_slice(&bytes).unwrap();
    let framed_centres = result["centres"].as_array().unwrap();
    assert_eq!(framed_centres[0]["fov"], json!(90.));
    assert!(framed_centres[1]["fov"].as_f64().unwrap() < 90.);
    for pair in framed_centres.windows(2) {
        assert!(
            (pair[0]["fov"].as_f64().unwrap() - pair[1]["fov"].as_f64().unwrap()).abs() <= 7.500001
        );
    }
    let mut invalid = framed;
    invalid["framing"]["targetFill"] = json!(0.0);
    assert_eq!(
        app.clone()
            .oneshot(request(&path, &owner.token, &invalid))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    for mode in ["revision", "region", "times", "range"] {
        let mut wrong = body.clone();
        match mode {
            "revision" => wrong["expectedRevision"] = json!(project.revision + 1),
            "region" => wrong["region"]["maskHash"] = json!("0".repeat(64)),
            "times" => wrong["sourceTimesUs"] = json!([200000, 50000]),
            _ => wrong["sourceTimesUs"] = json!([50000, 500000]),
        };
        assert!(
            app.clone()
                .oneshot(request(&path, &owner.token, &wrong))
                .await
                .unwrap()
                .status()
                .is_client_error(),
            "{mode}"
        );
    }
    let current = state
        .db
        .get_composition_project_for(&project.id, "owner")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.document, document);
    assert_eq!(current.revision, project.revision);
    let mut altered = std::fs::read(&movie).unwrap();
    altered.push(0);
    std::fs::write(&movie, altered).unwrap();
    assert_eq!(
        app.oneshot(request(&path, &owner.token, &body))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert!(
        std::fs::read_dir(state.storage.join("concat-video-region-stages"))
            .unwrap()
            .next()
            .is_none()
    );
}
