mod support;
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    composition::{CompositionSource, SourceId},
    concat_project_import::{import, ImportRequest},
    nested_composition::NestedCompositionProject,
    project_media_fill::fill,
};
fn fixture(kind: &str) -> NestedCompositionProject {
    let source = json!({"id":"old-file","kind":kind,"durationTicks":if kind=="image"{0}else{4_000_000},"width":if kind=="audio"{0}else{16},"height":if kind=="audio"{0}else{16},"hasAudio":kind!="image"});
    let timelines:Vec<_>=["main","shelved"].into_iter().map(|id|json!({"id":id,"video":{"width":16,"height":16,"rateNum":4,"rateDen":1},"tracks":[{"id":"lane"}],"clips":[{"id":"clip","trackId":"lane","mediaId":"slot","kind":kind,"name":"Creator","start":1.,"duration":2.,"sourceStart":0.25,"speed":1.5}]})).collect();
    let request:ImportRequest=serde_json::from_value(json!({"schemaVersion":1,"preserveMediaIds":true,"document":{"version":1,"media":[{"id":"slot","kind":kind,"name":"Creator","placeholder":true,"origin":"speech"},{"id":"alias","kind":kind,"name":"Other","placeholder":true}],"timelines":timelines},"mediaBindings":{"slot":source,"alias":source}})).unwrap();
    let report = import(request).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    report.project.unwrap()
}
fn replacement(kind: &str) -> (Value, CompositionSource) {
    let source:CompositionSource=serde_json::from_value(json!({"id":"new-file","kind":kind,"durationTicks":if kind=="image"{0}else{500000},"width":if kind=="audio"{0}else{32},"height":if kind=="audio"{0}else{16},"hasAudio":kind!="image"})).unwrap();
    (
        json!({"path":"library:new-file","name":"Replacement","kind":kind,"duration":if kind=="image"{Value::Null}else{json!(0.5)},"width":source.width,"height":source.height,"hasAudio":source.has_audio,"colorSpace":"sdr","audioTracks":[],"origin":"processed"}),
        source,
    )
}
#[test]
fn fill_all_nine_media_kind_pairs_preserves_slots_and_editorial_windows_in_all_timelines() {
    for old_kind in ["video", "audio", "image"] {
        for new_kind in ["video", "audio", "image"] {
            let before = fixture(old_kind);
            let immutable = serde_json::to_value(&before).unwrap();
            let id = SourceId::parse("slot").unwrap();
            let (item, source) = replacement(new_kind);
            let after = fill(&before, &id, &item, &source)
                .unwrap_or_else(|e| panic!("{old_kind} -> {new_kind}: {e:#}"));
            assert_eq!(after.root, before.root);
            assert_eq!(after.timeline_order, before.timeline_order);
            assert_eq!(
                after.project_media[&SourceId::parse("alias").unwrap()],
                before.project_media[&SourceId::parse("alias").unwrap()]
            );
            let entry = &after.project_media[&id];
            assert!(!entry.placeholder);
            assert_eq!(entry.source.id, id);
            assert_eq!(
                entry.library_source_id.as_ref().unwrap().as_str(),
                "new-file"
            );
            assert_eq!(entry.name, "Replacement");
            assert_eq!(entry.concat_metadata["origin"], "speech");
            for node in after.compositions.values() {
                let wire = serde_json::to_value(&node.content).unwrap();
                let track = &wire["tracks"][0];
                let clip = &track["clips"][0];
                assert_eq!(track["kind"], new_kind);
                assert_eq!(clip["sourceId"], "slot");
                assert_eq!(wire["metadata"]["clipNames"]["clip"], "Replacement");
                if new_kind == "image" {
                    assert_eq!(clip["timelineStartTick"], 1_000_000);
                    assert_eq!(clip["durationTicks"], 2_000_000);
                    assert_eq!(clip["concatSourceTiming"]["sourceStart"], 0.);
                    assert_eq!(clip["concatSourceTiming"]["speed"], 1.5);
                } else {
                    assert_eq!(clip["placement"]["timelineStartTick"], 1_000_000);
                    assert_eq!(clip["placement"]["sourceInTick"], 0);
                    assert_eq!(clip["placement"]["sourceOutTick"], 3_000_000);
                    assert_eq!(clip["placement"]["speed"], 1.5);
                    assert_eq!(clip["placement"]["concatSourceEof"], true);
                }
            }
            assert_eq!(serde_json::to_value(before).unwrap(), immutable);
        }
    }
}
#[test]
fn unsupported_retained_controls_and_non_slots_refuse_atomically() {
    let project = fixture("audio");
    let (item, source) = replacement("image");
    let id = SourceId::parse("slot").unwrap();
    let immutable = serde_json::to_value(&project).unwrap();
    let mut bad = project.clone();
    bad.project_media.get_mut(&id).unwrap().placeholder = false;
    assert!(fill(&bad, &id, &item, &source).is_err());
    let mut wire = immutable.clone();
    for node in wire["compositions"].as_object_mut().unwrap().values_mut() {
        node["content"]["tracks"][0]["clips"][0]["pan"] = json!({"mode":"constant","value":0.5});
    }
    let bad: NestedCompositionProject = serde_json::from_value(wire).unwrap();
    assert!(fill(&bad, &id, &item, &source).is_err());
    assert_eq!(serde_json::to_value(project).unwrap(), immutable);
}

#[tokio::test]
async fn registered_native_fill_is_private_pure_and_renders_changed_slot_without_rebinding_its_alias(
) {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    use video_kadr_backend::library::MediaEntry;
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("native-fill-owner", "native fill password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("native-fill-outsider", "native fill password")
        .await
        .unwrap()
        .unwrap();
    for (id, color) in [
        ("old-file", [10, 20, 80, 255]),
        ("new-file", [180, 30, 40, 255]),
    ] {
        let filename = format!("{id}.png");
        image::RgbaImage::from_pixel(16, 16, image::Rgba(color))
            .save(state.sources_dir().join(&filename))
            .unwrap();
        state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"image","width":999,"height":999}))).await;
    }
    let mut value = serde_json::to_value(fixture("image")).unwrap();
    for node in value["compositions"].as_object_mut().unwrap().values_mut() {
        let mut alias = node["content"]["tracks"][0]["clips"][0].clone();
        alias["id"] = json!("alias-clip");
        alias["sourceId"] = json!("alias");
        alias["timelineStartTick"] = json!(3_000_000);
        alias["durationTicks"] = json!(1_000_000);
        node["content"]["tracks"][0]["clips"]
            .as_array_mut()
            .unwrap()
            .push(alias);
    }
    let project: NestedCompositionProject = serde_json::from_value(value.clone()).unwrap();
    project.dependency_order().unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Private slots")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Native slots",
            &value,
            &["old-file".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Replacement",
            &json!({"schemaVersion":1,"sources":{"new-file":{"id":"new-file"}}}),
            &["new-file".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let app = support::router(state.clone());
    let request = |token: Option<&str>| {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/api/compositions/nested-media-fill")
            .header("content-type", "application/json");
        if let Some(token) = token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        builder
            .body(Body::from(
                json!({"project":project,"mediaId":"slot","sourceId":"new-file"}).to_string(),
            ))
            .unwrap()
    };
    assert_eq!(
        app.clone().oneshot(request(None)).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(request(Some(&outsider.token)))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = app
        .clone()
        .oneshot(request(Some(&owner.token)))
        .await
        .unwrap();
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "private, no-store"
    );
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let reply: Value = serde_json::from_slice(&bytes).unwrap();
    let filled: NestedCompositionProject =
        serde_json::from_value(reply["project"].clone()).unwrap();
    let slot = SourceId::parse("slot").unwrap();
    assert_eq!(filled.project_media[&slot].source.width, 16);
    assert_eq!(
        filled.project_media[&slot]
            .library_source_id
            .as_ref()
            .unwrap()
            .as_str(),
        "new-file"
    );
    assert_eq!(
        filled.project_media[&SourceId::parse("alias").unwrap()],
        project.project_media[&SourceId::parse("alias").unwrap()]
    );
    assert!(!String::from_utf8_lossy(&bytes).contains(state.sources_dir().to_str().unwrap()));
    assert_eq!(serde_json::to_value(&project).unwrap(), value);
    assert_eq!(
        state.library.get("new-file").await.unwrap().width,
        Some(999)
    );
    let saved = state
        .db
        .create_space_composition_project(
            "Filled",
            &reply["project"],
            &["new-file".into(), "old-file".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    assert_eq!(saved.source_ids, vec!["new-file", "old-file"]);
    for (time, color) in [
        (1_500_000, [180, 30, 40, 255]),
        (3_500_000, [10, 20, 80, 255]),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/compositions/nested-preview")
                    .header("content-type", "application/json")
                    .header("accept", "image/png")
                    .header("authorization", format!("Bearer {}", owner.token))
                    .body(Body::from(
                        json!({"project":filled,"timeUs":time,"renderedOutputs":{}}).to_string(),
                    ))
                    .unwrap(),
            )
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
        let frame = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert!(frame.pixels().all(|pixel| pixel.0 == color));
    }
}

#[test]
fn native_fill_resets_stream_selection_and_supports_a_slot_without_any_placement() {
    for kind in ["video", "audio"] {
        let original = fixture(kind);
        let mut wire = serde_json::to_value(&original).unwrap();
        for node in wire["compositions"].as_object_mut().unwrap().values_mut() {
            node["content"]["tracks"][0]["clips"][0]["audioStream"] = json!(3);
        }
        let project: NestedCompositionProject = serde_json::from_value(wire).unwrap();
        let (item, source) = replacement(kind);
        let changed = fill(&project, &SourceId::parse("slot").unwrap(), &item, &source).unwrap();
        for node in changed.compositions.values() {
            let value = serde_json::to_value(&node.content).unwrap();
            assert!(value["tracks"][0]["clips"][0].get("audioStream").is_none());
        }
    }
    let mut project = fixture("image");
    for node in project.compositions.values_mut() {
        node.content.tracks.clear();
        node.content.sources.clear();
        node.content.metadata = None;
    }
    let before = project.compositions.clone();
    let (item, source) = replacement("video");
    let changed = fill(&project, &SourceId::parse("slot").unwrap(), &item, &source).unwrap();
    assert_eq!(changed.compositions, before);
    assert_eq!(
        changed.project_media[&SourceId::parse("slot").unwrap()]
            .library_source_id
            .as_ref()
            .unwrap()
            .as_str(),
        "new-file"
    );
    assert!(!changed.project_media[&SourceId::parse("slot").unwrap()].placeholder);
}

fn run_ffmpeg(arguments: &[&str]) -> Vec<u8> {
    let result = std::process::Command::new("ffmpeg")
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}
async fn export_filled_range(
    state: &video_kadr_backend::state::AppState,
    app: &axum::Router,
    project: &NestedCompositionProject,
    token: &str,
    start: u64,
    end: u64,
) -> std::path::PathBuf {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/nested-export").header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(Body::from(json!({"schemaVersion":1,"mode":"video_audio","project":project,"compositionId":project.root,"startUs":start,"endUs":end,"renderedOutputs":{}}).to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let queued: Value = serde_json::from_slice(&bytes).unwrap();
    let id = queued["jobId"].as_str().unwrap();
    let job = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if job.status.is_terminal() {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("owned native export job timeout");
    assert_eq!(
        job.status,
        video_kadr_backend::model::JobStatus::Done,
        "{:?}",
        job.error
    );
    state
        .outputs_dir()
        .join(job.result.unwrap()["filename"].as_str().unwrap())
}
fn pcm(path: &std::path::Path) -> Vec<f32> {
    run_ffmpeg(&[
        "-v",
        "error",
        "-i",
        path.to_str().unwrap(),
        "-map",
        "0:a:0",
        "-f",
        "f32le",
        "-acodec",
        "pcm_f32le",
        "-ar",
        "48000",
        "-ac",
        "2",
        "-",
    ])
    .chunks_exact(4)
    .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
    .collect()
}
#[tokio::test]
#[ignore = "requires actual FFmpeg/ffprobe; real native filled moving/audio export jobs"]
async fn native_filled_video_and_audio_use_new_physical_files_hold_last_frame_and_pad_eof_with_silence(
) {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    use video_kadr_backend::library::MediaEntry;
    let colors = [
        [40u8, 20, 10, 255],
        [10, 90, 20, 255],
        [20, 10, 140, 255],
        [170, 20, 30, 255],
    ];
    for new_kind in ["video", "audio"] {
        let (mut state, _dir) = support::make_state(true, false).await;
        let mut tools = state.tools.as_ref().clone();
        tools
            .ffmpeg_encoders
            .extend(["ffv1".into(), "pcm_f32le".into()]);
        tools.ffmpeg_muxers.push("matroska".into());
        state.tools = std::sync::Arc::new(tools);
        let owner = state
            .db
            .register_auth_user("moving-fill-owner", "moving fill password")
            .await
            .unwrap()
            .unwrap();
        let filename = if new_kind == "video" {
            "new-file.mkv"
        } else {
            "new-file.wav"
        };
        let path = state.sources_dir().join(filename);
        if new_kind == "video" {
            let raw = state.sources_dir().join("reference.rgba");
            let bytes: Vec<_> = colors
                .iter()
                .flat_map(|color| color.repeat(16 * 16))
                .collect();
            std::fs::write(&raw, bytes).unwrap();
            run_ffmpeg(&[
                "-y",
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
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000:duration=1",
                "-c:v",
                "ffv1",
                "-pix_fmt",
                "bgra",
                "-c:a",
                "pcm_f32le",
                "-t",
                "1",
                path.to_str().unwrap(),
            ]);
        } else {
            run_ffmpeg(&[
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=660:sample_rate=48000:duration=1",
                "-c:a",
                "pcm_f32le",
                path.to_str().unwrap(),
            ]);
        }
        state.library.add(MediaEntry::from_result("source",&json!({"id":"new-file","filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":new_kind,"duration":999,"width":999,"height":999}))).await;
        let old_color = [5u8, 30, 100, 255];
        let old_filename = if new_kind == "video" {
            "old-file.png"
        } else {
            "old-file.mkv"
        };
        let old_path = state.sources_dir().join(old_filename);
        if new_kind == "video" {
            image::RgbaImage::from_pixel(16, 16, image::Rgba(old_color))
                .save(&old_path)
                .unwrap();
        } else {
            let raw = state.sources_dir().join("old-reference.rgba");
            std::fs::write(&raw, old_color.repeat(16 * 16 * 16)).unwrap();
            run_ffmpeg(&[
                "-y",
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
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1100:sample_rate=48000:duration=4",
                "-c:v",
                "ffv1",
                "-pix_fmt",
                "bgra",
                "-c:a",
                "pcm_f32le",
                "-t",
                "4",
                old_path.to_str().unwrap(),
            ]);
        }
        state.library.add(MediaEntry::from_result("source",&json!({"id":"old-file","filename":old_filename,"url":format!("/files/sources/{old_filename}"),"mediaType":if new_kind=="video"{"image"}else{"video"},"width":16,"height":16,"duration":4}))).await;
        let before = mixed_fixture(if new_kind == "video" {
            "image"
        } else {
            "video"
        });
        let snapshot = serde_json::to_value(&before).unwrap();
        let app = support::router(state.clone());
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/compositions/nested-media-fill")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {}", owner.token))
                    .body(Body::from(
                        json!({"project":before,"mediaId":"slot","sourceId":"new-file"})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        let reply: Value = serde_json::from_slice(&bytes).unwrap();
        let filled: NestedCompositionProject =
            serde_json::from_value(reply["project"].clone()).unwrap();
        assert_eq!(reply["source"]["durationTicks"], 1_000_000);
        assert_eq!(
            filled.project_media[&SourceId::parse("slot").unwrap()]
                .library_source_id
                .as_ref()
                .unwrap()
                .as_str(),
            "new-file"
        );
        assert_eq!(serde_json::to_value(&before).unwrap(), snapshot);
        let full =
            export_filled_range(&state, &app, &filled, &owner.token, 1_000_000, 4_000_000).await;
        let video = run_ffmpeg(&[
            "-v",
            "error",
            "-i",
            full.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-",
        ]);
        assert_eq!(video.len(), 12 * 16 * 16 * 4);
        for (frame, index) in video
            .chunks_exact(16 * 16 * 4)
            .zip([0, 1, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4])
        {
            let color = if index == 4 {
                old_color
            } else if new_kind == "video" {
                colors[index]
            } else {
                [0, 0, 0, 255]
            };
            assert!(
                frame.chunks_exact(4).all(|pixel| pixel == color),
                "{new_kind}: wrong moving/held frame index {index}"
            );
        }
        let samples = pcm(&full);
        assert_eq!(samples.len(), 144_000 * 2);
        assert!(samples.iter().all(|sample| sample.is_finite()));
        assert!(samples[..28_800].iter().any(|sample| sample.abs() > 0.01));
        assert!(
            samples[96_000..192_000]
                .iter()
                .all(|sample| sample.abs() < 1e-7),
            "{new_kind}: late source EOF is not silent"
        );
        if new_kind == "audio" {
            assert!(
                samples[201_600..278_400]
                    .iter()
                    .any(|sample| sample.abs() > 0.01),
                "retained alias audio disappeared"
            );
        } else {
            assert!(samples[192_000..].iter().all(|sample| sample.abs() < 1e-7));
        }
        let beyond =
            export_filled_range(&state, &app, &filled, &owner.token, 2_000_000, 2_500_000).await;
        let selected = pcm(&beyond);
        assert_eq!(selected.len(), 24_000 * 2);
        assert!(
            selected.iter().all(|sample| *sample == 0.),
            "{new_kind}: selected EOF-only audio must be silent"
        );
        let held = run_ffmpeg(&[
            "-v",
            "error",
            "-i",
            beyond.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-",
        ]);
        assert_eq!(held.len(), 2 * 16 * 16 * 4);
        let color = if new_kind == "video" {
            colors[3]
        } else {
            [0, 0, 0, 255]
        };
        assert!(held.chunks_exact(4).all(|pixel| pixel == color));
    }
}

fn mixed_fixture(kind: &str) -> NestedCompositionProject {
    let mut value = serde_json::to_value(fixture(kind)).unwrap();
    for node in value["compositions"].as_object_mut().unwrap().values_mut() {
        let mut alias = node["content"]["tracks"][0]["clips"][0].clone();
        alias["id"] = json!("alias-clip");
        alias["sourceId"] = json!("alias");
        if kind == "image" {
            alias["timelineStartTick"] = json!(3_000_000);
            alias["durationTicks"] = json!(1_000_000);
        } else {
            alias["placement"]["timelineStartTick"] = json!(3_000_000);
            alias["placement"]["sourceOutTick"] = json!(1_750_000);
        }
        node["content"]["tracks"][0]["clips"]
            .as_array_mut()
            .unwrap()
            .push(alias);
        node["content"]["metadata"]["clipNames"]["alias-clip"] = json!("Untouched alias");
        node["content"]["metadata"]["clipLinks"] = json!({"linked":["clip","alias-clip"]});
    }
    let project: NestedCompositionProject = serde_json::from_value(value).unwrap();
    project.dependency_order().unwrap();
    project
}
fn clip_value(project: &NestedCompositionProject, node: &str, id: &str) -> Value {
    let value = serde_json::to_value(&project.compositions[node].content).unwrap();
    value["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|track| track["clips"].as_array().unwrap())
        .find(|clip| clip["id"] == id)
        .unwrap()
        .clone()
}
#[test]
fn mixed_source_lanes_split_all_kind_pairs_without_changing_other_media_clips_or_links() {
    for old_kind in ["video", "audio", "image"] {
        for new_kind in ["video", "audio", "image"] {
            let before = mixed_fixture(old_kind);
            let original = serde_json::to_value(&before).unwrap();
            let (item, source) = replacement(new_kind);
            let id = SourceId::parse("slot").unwrap();
            let after = fill(&before, &id, &item, &source)
                .unwrap_or_else(|e| panic!("mixed {old_kind} -> {new_kind}: {e:#}"));
            for (node_id, node) in &after.compositions {
                assert_eq!(
                    clip_value(&after, node_id, "alias-clip"),
                    clip_value(&before, node_id, "alias-clip")
                );
                let old_track = &before.compositions[node_id].content.tracks[0];
                let retained = node
                    .content
                    .tracks
                    .iter()
                    .find(|track| track.id() == old_track.id())
                    .unwrap();
                let retained = serde_json::to_value(retained).unwrap();
                assert_eq!(retained["kind"], old_kind);
                assert_eq!(
                    node.content.metadata.as_ref().unwrap().clip_links,
                    before.compositions[node_id]
                        .content
                        .metadata
                        .as_ref()
                        .unwrap()
                        .clip_links
                );
                assert_eq!(
                    node.content.metadata.as_ref().unwrap().clip_names
                        [&video_kadr_backend::domain::composition::CompositionClipId::parse(
                            "alias-clip"
                        )
                        .unwrap()],
                    "Untouched alias"
                );
                assert_eq!(
                    node.content.tracks.len(),
                    if old_kind == new_kind { 1 } else { 2 }
                );
                if matches!(
                    (old_kind, new_kind),
                    ("video", "image") | ("image", "video")
                ) {
                    assert_eq!(
                        video_kadr_backend::domain::concat_lane_groups::groups(&node.content)
                            .unwrap()
                            .len(),
                        1
                    );
                }
                let changed = clip_value(&after, node_id, "clip");
                assert_eq!(changed["sourceId"], "slot");
            }
            assert_eq!(serde_json::to_value(before).unwrap(), original);
        }
    }
}
#[test]
fn refilling_grouped_visual_slots_reuses_siblings_and_keeps_other_clips_in_place() {
    use video_kadr_backend::domain::project_media::set_placeholder;
    for old_kind in ["video", "image"] {
        let first_kind = if old_kind == "video" {
            "image"
        } else {
            "video"
        };
        let original = mixed_fixture(old_kind);
        let id = SourceId::parse("slot").unwrap();
        let (item, source) = replacement(first_kind);
        let first = fill(&original, &id, &item, &source).unwrap();
        let (marked, _) = set_placeholder(&first, &id, true).unwrap();
        for second_kind in [old_kind, "audio"] {
            let (item, source) = replacement(second_kind);
            let second = fill(&marked, &id, &item, &source)
                .unwrap_or_else(|e| panic!("grouped {first_kind} -> {second_kind}: {e:#}"));
            for (node_id, node) in &second.compositions {
                assert_eq!(
                    node.content.tracks.len(),
                    if second_kind == "audio" { 3 } else { 2 }
                );
                assert_eq!(
                    node.content.metadata.as_ref().unwrap().concat_lane_groups,
                    first.compositions[node_id]
                        .content
                        .metadata
                        .as_ref()
                        .unwrap()
                        .concat_lane_groups
                );
                assert_eq!(
                    clip_value(&second, node_id, "alias-clip"),
                    clip_value(&original, node_id, "alias-clip")
                );
                assert_eq!(
                    video_kadr_backend::domain::concat_lane_groups::groups(&node.content)
                        .unwrap()
                        .len(),
                    1
                );
            }
        }
    }
}

#[test]
fn split_visual_lanes_rejoin_as_one_concat_lane_on_portable_export() {
    use std::collections::BTreeMap;
    use video_kadr_backend::domain::concat_project_export::export_with_paths;
    for (old_kind, new_kind) in [("video", "image"), ("image", "video")] {
        let mut original = mixed_fixture(old_kind);
        for node in original.compositions.values_mut() {
            node.content.metadata.as_mut().unwrap().clip_links.clear();
        }
        let (item, source) = replacement(new_kind);
        let after = fill(&original, &SourceId::parse("slot").unwrap(), &item, &source).unwrap();
        let output = export_with_paths(
            &after,
            &BTreeMap::from([
                ("slot".into(), "/inert/new".into()),
                ("alias".into(), "/inert/old".into()),
            ]),
        )
        .unwrap();
        for timeline in output["timelines"].as_array().unwrap() {
            assert_eq!(timeline["tracks"].as_array().unwrap().len(), 1);
            let clips = timeline["clips"].as_array().unwrap();
            assert_eq!(clips.len(), 2);
            assert_eq!(clips[0]["trackId"], clips[1]["trackId"]);
            let target = clips.iter().find(|clip| clip["id"] == "clip").unwrap();
            assert_eq!(target["kind"], new_kind);
            assert_eq!(target["start"], 1.);
            assert_eq!(target["duration"], 2.);
            assert_eq!(
                clips
                    .iter()
                    .find(|clip| clip["id"] == "alias-clip")
                    .unwrap()["kind"],
                old_kind
            );
        }
    }
}
#[test]
fn cross_kind_lane_split_respects_track_budget_and_existing_solo_or_primary_audio() {
    use video_kadr_backend::domain::composition::{CompositionTrack, TrackId};
    let id = SourceId::parse("slot").unwrap();
    let (item, source) = replacement("video");
    let mut full = mixed_fixture("image");
    for node in full.compositions.values_mut() {
        for index in 0..15 {
            node.content.tracks.push(CompositionTrack::Color {
                id: TrackId::parse(format!("empty-{index}")).unwrap(),
                name: "Other lane".into(),
                hidden: true,
                locked: false,
                clips: vec![],
            });
        }
    }
    full.dependency_order().unwrap();
    let before = serde_json::to_value(&full).unwrap();
    assert!(fill(&full, &id, &item, &source).is_err());
    assert_eq!(serde_json::to_value(full).unwrap(), before);
    let mut solo = mixed_fixture("audio");
    for node in solo.compositions.values_mut() {
        let mut track = node.content.tracks[0].clone();
        if let CompositionTrack::Audio {
            id, solo, clips, ..
        } = &mut track
        {
            *id = TrackId::parse("solo-lane").unwrap();
            *solo = true;
            clips.clear();
        }
        node.content.tracks.push(track);
    }
    solo.dependency_order().unwrap();
    assert!(fill(&solo, &id, &item, &source)
        .unwrap_err()
        .to_string()
        .contains("solo routing"));
    let mut legacy = mixed_fixture("audio");
    let video = fixture("video");
    let content = &video.compositions[&video.root].content;
    for node in legacy.compositions.values_mut() {
        let mut track = content.tracks[0].clone();
        let mut source = content.sources[&id].clone();
        source.id = SourceId::parse("legacy-file").unwrap();
        node.content
            .sources
            .insert(source.id.clone(), source.clone());
        if let CompositionTrack::Video { id, clips, .. } = &mut track {
            *id = TrackId::parse("legacy-lane").unwrap();
            for clip in clips {
                clip.id = video_kadr_backend::domain::composition::CompositionClipId::parse(
                    "legacy-clip",
                )
                .unwrap();
                clip.source_id = source.id.clone();
                clip.independent_audio = false;
            }
        }
        node.content.tracks.push(track);
    }
    legacy.dependency_order().unwrap();
    assert!(fill(&legacy, &id, &item, &source)
        .unwrap_err()
        .to_string()
        .contains("legacy primary-audio"));
}
