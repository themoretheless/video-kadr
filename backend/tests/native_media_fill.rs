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
