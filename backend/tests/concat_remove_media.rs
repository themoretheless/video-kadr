mod support;
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    composition::SourceId,
    concat_project_import::{import, ImportRequest},
    nested_composition::NestedCompositionProject,
    project_media::remove,
};
fn converted(document: &Value) -> NestedCompositionProject {
    let bindings:serde_json::Map<String,Value>=document["media"].as_array().unwrap().iter().map(|media|{
        let id=media["id"].as_str().unwrap();
        (id.to_owned(),json!({"id":id,"kind":media["kind"],"durationTicks":if media["kind"]=="image"{0}else{4_000_000},"width":if media["kind"]=="audio"{0}else{16},"height":if media["kind"]=="audio"{0}else{16},"hasAudio":media["kind"]=="audio"}))
    }).collect();
    let request: ImportRequest = serde_json::from_value(
        json!({"schemaVersion":1,"document":document,"mediaBindings":bindings}),
    )
    .unwrap();
    let report = import(request).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    report.project.unwrap()
}
#[test]
fn all_timelines_match_pinned_original_remove_media() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/concat-remove-media-oracle.json")).unwrap();
    assert_eq!(
        fixture["sourceCommit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 18);
    for case in fixture["cases"].as_array().unwrap() {
        let before =
            converted(&fixture["documents"][case["documentIndex"].as_u64().unwrap() as usize]);
        let expected = converted(&fixture["afters"][case["afterIndex"].as_u64().unwrap() as usize]);
        let snapshot = serde_json::to_value(&before).unwrap();
        let id = SourceId::parse(case["command"]["mediaId"].as_str().unwrap()).unwrap();
        let (actual, applied) = remove(&before, &id).unwrap();
        assert_eq!(case["ok"], true);
        assert_eq!(applied, case["applied"].as_bool().unwrap());
        assert_eq!(actual, expected, "{:?}", case["command"]);
        assert_eq!(serde_json::to_value(&before).unwrap(), snapshot);
        assert_eq!(remove(&actual, &id).unwrap(), (actual, false));
    }
}
#[tokio::test]
async fn native_remove_http_is_pure_and_unknown_target_is_noop() {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/concat-remove-media-oracle.json")).unwrap();
    let project = converted(&fixture["documents"][0]);
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    for id in ["used", "missing"] {
        let expected = remove(&project, &SourceId::parse(id).unwrap()).unwrap();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/compositions/nested-media-remove")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({"project":project,"sourceId":id})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let reply: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), 4 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(reply["applied"], expected.1);
        assert_eq!(
            serde_json::from_value::<NestedCompositionProject>(reply["project"].clone()).unwrap(),
            expected.0
        );
    }
}

#[test]
fn native_audio_removal_clears_dependent_crossfade_links_and_clip_names() {
    use video_kadr_backend::domain::composition::CompositionTrack;
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/concat-remove-media-oracle.json")).unwrap();
    let doc = fixture["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|doc| doc["media"][0]["kind"] == "audio")
        .unwrap();
    let mut project = converted(doc);
    for node in project.compositions.values_mut() {
        for track in &mut node.content.tracks {
            if let CompositionTrack::Audio { clips, .. } = track {
                if clips.len() != 2 {
                    continue;
                }
                clips.sort_by_key(|clip| clip.placement.timeline_start_tick);
                for clip in clips.iter_mut() {
                    clip.placement.source_in_tick = 1_000_000;
                    clip.placement.source_out_tick = 2_000_000;
                }
                clips[1].placement.timeline_start_tick = 1_000_000;
                clips[1].crossfade_in_ticks = 200_000;
                node.content.metadata.as_mut().unwrap().clip_links.insert(
                    "pair".into(),
                    clips.iter().map(|clip| clip.id.clone()).collect(),
                );
            }
        }
    }
    project.dependency_order().unwrap();
    let (removed, applied) = remove(&project, &SourceId::parse("used").unwrap()).unwrap();
    assert!(applied);
    for node in removed.compositions.values() {
        for track in &node.content.tracks {
            if let CompositionTrack::Audio { clips, .. } = track {
                for clip in clips {
                    assert_eq!(clip.source_id.as_str(), "keep");
                    assert_eq!(clip.crossfade_in_ticks, 0);
                    assert_eq!(clip.placement.timeline_start_tick, 1_000_000);
                }
            }
        }
        let metadata = node.content.metadata.as_ref().unwrap();
        assert!(metadata.clip_links.is_empty());
        assert!(metadata
            .clip_names
            .keys()
            .all(|id| !id.as_str().contains("used")));
    }
    // Locked lanes have the original project-wide RemoveMedia semantics too.
    // A project-bin action is not a selected-clip delete operation.
    let mut locked = project.clone();
    for node in locked.compositions.values_mut() {
        for track in &mut node.content.tracks {
            if let CompositionTrack::Audio { locked, .. } = track {
                *locked = true;
            }
        }
    }
    assert!(remove(&locked, &SourceId::parse("used").unwrap()).is_ok());
}
