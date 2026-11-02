mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
fn document() -> Value {
    json!({"version":1,"media":[{"id":"slot","kind":"image","placeholder":true,"name":"Creator","origin":"speech","colorRange":null,"future":{"retain":true}}],"timelines":[{"id":"timeline","clips":[{"id":"clip","mediaId":"slot","kind":"image","name":"Creator","sourceStart":9.,"duration":3.,"start":2.,"speed":2.,"audioStream":7}]}]})
}
fn request(value: Value, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/compositions/concat-project/fill-registered-slot")
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(value.to_string())).unwrap()
}
async fn token(app: &axum::Router, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/register")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"username":name,"password":"slot test password"}).to_string(),
        ))
        .unwrap();
    let (status, body, _) = support::send(app, request).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["token"].as_str().unwrap().to_owned()
}
#[tokio::test]
async fn registered_fill_requires_authentication_before_any_probe() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let (status, body, _) = support::send(
        &app,
        request(
            json!({"document":document(),"mediaId":"slot","sourceId":"missing"}),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(body.get("document").is_none());
}
#[tokio::test]
#[ignore = "requires actual FFmpeg/ffprobe; generated source with two audio streams"]
async fn owner_fill_uses_fresh_probe_and_retains_slot_identity_without_exposing_a_disk_path() {
    use video_kadr_backend::{
        domain::composition::{CanvasSpec, Composition},
        library::MediaEntry,
    };
    let (state, _dir) = support::make_state(true, false).await;
    let app = support::router(state.clone());
    let owner = token(&app, "slot-owner").await;
    let outsider = token(&app, "slot-outsider").await;
    let id = uuid::Uuid::new_v4().to_string();
    let filename = format!("{id}.mkv");
    let path = state.sources_dir().join(&filename);
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=32x16:r=4:d=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000:duration=1",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-c:v",
            "ffv1",
            "-c:a",
            "pcm_s16le",
            "-metadata:s:a:0",
            "title=Desktop",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:1",
            "title=Mic",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":filename,"url":"/local","mediaType":"video","title":"Stale title","duration":99.,"width":999,"height":999,"fps":60.,"vcodec":"stale","acodec":"stale"}))).await;
    state
        .db
        .replace_library_metadata(&id, Some("Current library title".into()), false, vec![])
        .await
        .unwrap();
    let space = state
        .db
        .create_space("slot-owner", "Private source")
        .await
        .unwrap();
    let mut content = serde_json::to_value(Composition::new(CanvasSpec::default())).unwrap();
    content["sources"] = json!({(id.clone()):{"id":id,"kind":"video","durationTicks":1000000,"width":32,"height":16,"hasAudio":true}});
    let project = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":content,"instances":[]}}});
    state
        .db
        .create_space_composition_project(
            "Private source",
            &project,
            &[id.clone()],
            "slot-owner",
            &space.id,
        )
        .await
        .unwrap();
    let input = json!({"document":document(),"mediaId":"slot","sourceId":id});
    let (status, denied, _) = support::send(&app, request(input.clone(), Some(&outsider))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{denied}");
    assert!(denied.get("document").is_none());
    use tower::ServiceExt;
    let response = app
        .clone()
        .oneshot(request(input.clone(), Some(&owner)))
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let reply: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(status, StatusCode::OK, "{reply}");
    assert_eq!(headers.get("cache-control").unwrap(), "private, no-store");
    let item = &reply["document"]["media"][0];
    assert_eq!(item["id"], "slot");
    assert_eq!(item["path"], format!("library:{id}"));
    assert_eq!(item["name"], "Current library title");
    assert_eq!(item["origin"], "speech");
    assert_eq!(item["future"], document()["media"][0]["future"]);
    assert!(item.get("placeholder").is_none());
    assert_eq!(item["kind"], "video");
    assert_eq!(item["width"], 32);
    assert_eq!(item["height"], 16);
    assert_eq!(item["frameRate"], 4.);
    assert_eq!(item["frameRateFraction"], "4/1");
    assert_eq!(item["videoCodec"], "ffv1");
    assert_eq!(item["audioCodec"], "pcm_s16le");
    assert_eq!(item["audioTracks"][0]["index"], 1);
    assert_eq!(item["audioTracks"][1]["index"], 2);
    assert_eq!(item["audioTracks"][0]["sampleRate"], 44100);
    assert_eq!(item["audioTracks"][1]["sampleRate"], 48000);
    assert_eq!(item["audioTracks"][0]["title"], "Desktop");
    assert_eq!(item["audioTracks"][0]["language"], "eng");
    assert_eq!(reply["source"]["id"], id);
    assert_eq!(reply["source"]["width"], 32);
    assert_eq!(reply["source"]["durationTicks"], 1000000);
    let clip = &reply["document"]["timelines"][0]["clips"][0];
    assert_eq!(clip["sourceStart"], 0.);
    assert_eq!(clip["duration"], 3.);
    assert_eq!(clip["speed"], 2.);
    assert!(clip.get("audioStream").is_none());
    assert!(!reply
        .to_string()
        .contains(&path.to_string_lossy().to_string()));
    assert_eq!(input["document"], document());
    assert_eq!(state.library.get(&id).await.unwrap().width, Some(999));
}

#[test]
fn registered_probe_properties_keep_exact_rate_and_hdr_instead_of_assuming_sdr() {
    use video_kadr_backend::domain::{
        composition::{SourceId, SourceKind},
        concat_registered_media::replacement,
        media_probe::ProbeResult,
    };
    for (transfer, expected) in [
        ("arib-std-b67", "hlg"),
        ("smpte2084", "pq"),
        ("bt709", "sdr"),
    ] {
        let probe=ProbeResult::from_ffprobe_json(&json!({"format":{"duration":"1.0","format_name":"matroska"},"streams":[{"index":0,"codec_type":"video","codec_name":"hevc","width":16,"height":16,"avg_frame_rate":"30000/1001","r_frame_rate":"30000/1001","color_transfer":transfer},{"index":2,"codec_type":"audio","codec_name":"aac","sample_rate":"48000","channels":2,"tags":{"title":"Voice","language":"eng"}},{"index":3,"codec_type":"subtitle","codec_name":"subrip"}]})).unwrap();
        let (item, source) = replacement(
            SourceId::parse("registered").unwrap(),
            "Fresh",
            SourceKind::Video,
            &probe,
        )
        .unwrap();
        assert_eq!(item["frameRateFraction"], "30000/1001");
        assert_eq!(item["colorSpace"], expected);
        assert_eq!(item["audioTracks"].as_array().unwrap().len(), 1);
        assert_eq!(item["audioTracks"][0]["index"], 2);
        assert_eq!(source.duration_ticks, 1000000);
        assert_eq!(source.audio_channels, Some(2));
    }
}
