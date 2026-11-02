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
#[ignore = "requires host FFmpeg and pinned SlimSAM/ORT: actual neural identity inference"]
async fn saved_semantic_identity_reacquires_after_occlusion_with_actual_sam() {
    run_neural_http(false, false).await;
}
#[tokio::test]
#[ignore = "requires host FFmpeg and pinned SlimSAM/ORT: dual-stream native panorama plus gyro/reID"]
async fn raw_dual_stream_panorama_identity_and_gyro_camera_following() {
    run_neural_http(true, false).await;
}
#[tokio::test]
#[ignore = "requires pinned actual SAM: hash-valid stale propagated mask must not become identity truth"]
async fn stale_saved_exemplar_is_not_enrolled_as_an_identity() {
    run_neural_http(false, true).await;
}
async fn run_neural_http(dual: bool, stale_exemplar: bool) {
    let (state, _root) = support::make_state(true, false).await;
    let state = state.with_sam_config(Some(video_kadr_backend::services::local_sam::Config {
        executable: std::env::var("VIDEO_KADR_TEST_SAM_RUNTIME").unwrap().into(),
        encoder: std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap().into(),
        decoder: std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap().into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    }));
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
    let make = |kind: i32| -> Vec<u8> {
        (0..128 * 96)
            .flat_map(|i| {
                let x: i32 = i % 128;
                let y: i32 = i / 128;
                if kind != 0
                    && (x - if kind == 1 { 36 } else { 91 }).pow(2) + (y - 48).pow(2) < 18 * 18
                {
                    if kind == 3 {
                        [20, 230, 10, 255]
                    } else {
                        [220, 25, 45, 255]
                    }
                } else {
                    [22, 130, 180, 255]
                }
            })
            .collect()
    };
    let input: Vec<_> = [1, 0, 0, 3, 2].into_iter().flat_map(make).collect();
    let original_movie = state.storage.join("sources/raw.mkv");
    let mut command = std::process::Command::new("ffmpeg");
    command.args([
        "-nostdin",
        "-v",
        "error",
        "-f",
        "rawvideo",
        "-pixel_format",
        "rgba",
        "-video_size",
        "128x96",
        "-framerate",
        "1",
        "-i",
        "pipe:0",
    ]);
    if dual {
        command.args([
            "-f",
            "lavfi",
            "-i",
            "color=c=0x1682b4:s=128x96:r=1:d=5",
            "-map",
            "0:v",
            "-map",
            "1:v",
        ]);
    }
    let mut child = command
        .args(["-c:v", "ffv1", "-pix_fmt", "bgra", "-threads", "1"])
        .arg(&original_movie)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(&input).unwrap();
    assert!(child.wait().unwrap().success());
    let original_hash = format!(
        "{:x}",
        Sha256::digest(std::fs::read(&original_movie).unwrap())
    );
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"raw","filename":"raw.mkv","url":"/files/sources/raw.mkv","mediaType":"video","width":128,"height":96}))).await);
    let mut raw = vec![0; 5 * 256 * 256];
    for y in 0..256 {
        for x in 0..256 {
            let xx = x as f64 / 256. * 128.;
            let yy = y as f64 / 256. * 96.;
            if (xx - 36.).powi(2) + (yy - 48.).powi(2) < 18f64.powi(2) {
                raw[y * 256 + x] = 255;
            }
        }
    }
    if stale_exemplar {
        raw.copy_within(0..65536, 65536);
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
            frames: 5,
            frame_rate: FrameRateRatio {
                numerator: 1,
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
    let points = vec![[36. / 128., 0.5]];
    let mut region = json!({"sourceId":"raw","maskSourceId":"region","sourceHash":original_hash,"maskHash":mask_hash,"maskFileHash":metadata.file_hash,"encoderHash":"b".repeat(64),"decoderHash":"c".repeat(64),"runtimeHash":"d".repeat(64),"promptHash":format!("{:x}",Sha256::digest(serde_json::to_vec(&points).unwrap())),"points":points,"width":128,"height":96,"frames":5,"frameRate":{"numerator":1,"denominator":1},"seedFrame":0,"trackedFrames":1,"forwardLostAt":1,"backwardLostAt":null});
    if stale_exemplar {
        region["trackedFrames"] = json!(2);
        region["forwardLostAt"] = json!(2);
    }
    let mut document = serde_json::to_value(Composition::new(Default::default())).unwrap();
    document["sources"] = json!({"raw":{"id":"raw","kind":"video","width":128,"height":96,"durationTicks":5000000,"hasAudio":false},"copy":{"id":"copy","kind":"video","width":128,"height":96,"durationTicks":5000000,"hasAudio":false},"automatic":{"id":"automatic","kind":"video","width":512,"height":384,"durationTicks":5000000,"hasAudio":false},"region":{"id":"region","kind":"video","width":256,"height":256,"durationTicks":5000000,"hasAudio":false}});
    document["tracks"] = json!([{"kind":"video","id":"videos","name":"Videos","clips":[{"id":"clip","sourceId":"raw","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":5000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal"}]}]);
    document["metadata"] = json!({"mediaVersions":{"clip":{"originalSourceId":"raw","copySourceId":"copy","videoCutout":{"sourceId":"raw","maskSourceId":"automatic","sourceHash":original_hash,"maskHash":"e".repeat(64),"maskFileHash":"f".repeat(64),"width":128,"height":96,"maskWidth":512,"maskHeight":384,"frames":5,"frameRate":{"numerator":1,"denominator":1},"corrections":[],"trackedRegions":[region]}}}});
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
    let mut body = json!({"clipId":"clip","expectedRevision":project.revision,"region":region,"sourceTimesUs":[0,1000000,2000000,3000000,4000000],"reidentify":true});
    if dual {
        body["spherical"] = json!({"input":"dual_fisheye","sourceLayout":"dual_video_streams","orientation":[{"timeSeconds":0.,"quaternion":[std::f64::consts::FRAC_1_SQRT_2,0.,std::f64::consts::FRAC_1_SQRT_2,0.]}]});
        body["parameterStartUs"] = json!(0);
    }
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
    assert_eq!(centres.len(), 2, "{result}");
    assert_eq!(result["sampleIndices"], json!([0, 4]));
    assert_eq!(
        result["states"],
        json!(["seed", "occluded", "occluded", "occluded", "reacquired"])
    );
    let first = centres[0]["pan"].as_f64().unwrap();
    let last = centres[1]["pan"].as_f64().unwrap();
    if dual {
        assert!(
            (20.0..80.0).contains(&first),
            "rawpanorama gyrocentre={first}"
        );
        assert!(
            (100.0..160.0).contains(&last),
            "rawpanorama gyrocentre={last}"
        );
    } else {
        assert!(first < 0.);
        assert!(last > 0.);
    }

    let current = state
        .db
        .get_composition_project_for(&project.id, "owner")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.document, document);
    assert_eq!(current.revision, project.revision);
    assert!(
        std::fs::read_dir(state.storage.join("spherical-reidentification-stages"))
            .unwrap()
            .next()
            .is_none()
    );
}
