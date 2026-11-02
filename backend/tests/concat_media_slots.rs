use serde_json::{json, Value};
use video_kadr_backend::domain::concat_media_slots::{fill, set_placeholder};
fn document() -> Value {
    let clip = json!({"id":"c","mediaId":"slot","kind":"video","name":"Old","start":3.5,"duration":10.,"speed":2.,"sourceStart":9.,"audioStream":3,"keys":[{"at":0.5,"value":2}],"videoEffects":[{"id":"future.effect"}],"futureClip":{"keep":true}});
    json!({"version":1,"futureProject":7,"media":[{"id":"slot","path":"/creator/old.mov","name":"Old","placeholder":true,"kind":"video","duration":30,"origin":{"kind":"generated"},"colorRange":"full","futureMedia":{"keep":true}},{"id":"other","path":"/other"}],"timelines":[{"id":"active","clips":[clip.clone(),{"id":"unrelated","mediaId":"other","audioStream":4}],"futureTimeline":9},{"id":"shelved","clips":[clip]},{"id":"empty","clips":[]}]})
}
#[test]
fn fill_keeps_identity_and_authored_controls_across_all_timelines_for_every_kind() {
    for kind in ["video", "audio", "image"] {
        let original = document();
        let replacement = json!({"path":"/never-open/new","name":"New","kind":kind,"duration":0.25,"width":8,"height":6,"hasAudio":true,"audioTracks":[{"index":0}],"frameRate":24.,"frameRateFraction":"24/1","videoCodec":"v","audioCodec":"a","colorSpace":"hlg","origin":"must not replace","colorRange":"limited","futureMedia":"must not replace"});
        let next = fill(&original, "slot", &replacement).unwrap();
        let media = &next["media"][0];
        assert_eq!(media["id"], "slot");
        assert_eq!(media["duration"], 0.25);
        assert!(media.get("placeholder").is_none());
        for key in ["origin", "colorRange", "futureMedia"] {
            assert_eq!(media[key], original["media"][0][key]);
        }
        assert_eq!(next["futureProject"], original["futureProject"]);
        for index in 0..2 {
            let clip = &next["timelines"][index]["clips"][0];
            assert_eq!(clip["mediaId"], "slot");
            assert_eq!(clip["kind"], kind);
            assert_eq!(clip["name"], "New");
            assert_eq!(clip["sourceStart"], 0.);
            assert!(clip.get("audioStream").is_none());
            for key in [
                "id",
                "start",
                "duration",
                "speed",
                "keys",
                "videoEffects",
                "futureClip",
            ] {
                assert_eq!(
                    clip[key], original["timelines"][index]["clips"][0][key],
                    "{key}"
                );
            }
        }
        assert_eq!(
            next["timelines"][0]["clips"][1],
            original["timelines"][0]["clips"][1]
        );
        assert_eq!(next["timelines"][2], original["timelines"][2]);
        assert_eq!(original, document());
        assert!(fill(&next, "slot", &replacement)
            .unwrap_err()
            .to_string()
            .contains("NotASlot"));
    }
}
#[test]
fn marking_noops_and_refusals_are_atomic_and_empty_slots_can_be_filled() {
    let original = document();
    assert!(!set_placeholder(&original, "missing", true).unwrap().1);
    assert!(!set_placeholder(&original, "slot", true).unwrap().1);
    let (ordinary, applied) = set_placeholder(&original, "slot", false).unwrap();
    assert!(applied);
    assert!(!set_placeholder(&ordinary, "slot", false).unwrap().1);
    let replacement = json!({"path":"/never-open/image","name":"Image","kind":"image"});
    assert!(fill(&ordinary, "slot", &replacement)
        .unwrap_err()
        .to_string()
        .contains("NotASlot"));
    assert!(fill(&original, "missing", &replacement)
        .unwrap_err()
        .to_string()
        .contains("SlotGone"));
    let mut empty = original.clone();
    for timeline in empty["timelines"].as_array_mut().unwrap() {
        timeline["clips"] = json!([]);
    }
    let filled = fill(&empty, "slot", &replacement).unwrap();
    assert_eq!(filled["timelines"], empty["timelines"]);
    assert_eq!(filled["media"][0]["hasAudio"], false);
    for key in ["kind", "duration", "audioTracks", "hasAudio", "width"] {
        let mut invalid = replacement.clone();
        invalid[key] = json!({"invalid":true});
        assert!(fill(&original, "slot", &invalid).is_err(), "{key}");
    }
    assert_eq!(original, document());
}

mod support;
#[tokio::test]
async fn source_document_http_edits_are_pure_and_refusals_return_no_partial_document() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let original = document();
    let send = |value: Value| {
        Request::builder()
            .method("POST")
            .uri("/api/compositions/concat-project/media-slots")
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap()
    };
    let (status,result,_)=support::send(&app,send(json!({"operation":"fillSlot","document":original,"mediaId":"slot","replacement":{"path":"/not-opened","name":"Replacement","kind":"audio"}}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["applied"], true);
    assert_eq!(
        result["document"]["timelines"][1]["clips"][0]["kind"],
        "audio"
    );
    let (status,refused,_)=support::send(&app,send(json!({"operation":"fillSlot","document":result["document"],"mediaId":"slot","replacement":{"path":"/not-opened","name":"Replacement","kind":"video"}}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(refused.get("document").is_none());
    let (status,noop,_)=support::send(&app,send(json!({"operation":"setPlaceholder","document":original,"mediaId":"missing","placeholder":true}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(noop["applied"], false);
    assert_eq!(noop["document"], original);
    assert_eq!(original, document());
}

#[test]
fn agrees_with_actual_pinned_concat_engine_on_all_slot_edits() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/concat-media-slots-oracle.json")).unwrap();
    assert_eq!(
        fixture["sourceCommit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 170);
    let mut successes = 0;
    for (index, case) in cases.iter().enumerate() {
        let before = &fixture["documents"][case["documentIndex"].as_u64().unwrap() as usize];
        let command = &case["command"];
        let id = command["mediaId"].as_str().unwrap();
        let result = match command["op"].as_str().unwrap() {
            "fillSlot" => fill(before, id, &command["item"]).map(|document| (document, true)),
            "setMediaPlaceholder" => {
                set_placeholder(before, id, command["placeholder"].as_bool().unwrap())
            }
            _ => unreachable!(),
        };
        assert_eq!(
            result.is_ok(),
            case["ok"].as_bool().unwrap(),
            "case {index}: {result:?}"
        );
        match result {
            Ok((document, applied)) => {
                successes += 1;
                assert_eq!(applied, case["applied"].as_bool().unwrap(), "case {index}");
                assert_eq!(
                    document,
                    fixture["afters"][case["afterIndex"].as_u64().unwrap() as usize],
                    "case {index}"
                );
            }
            Err(error) => assert!(
                error.to_string().contains(case["error"].as_str().unwrap()),
                "case {index}: {error}"
            ),
        }
    }
    assert_eq!(successes, 35);
}

#[tokio::test]
async fn filled_image_slot_import_preserves_placements_and_retained_image_timing() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let original = json!({"version":1,"media":[{"id":"slot","name":"Creator","path":"/never/creator","kind":"video","placeholder":true}],"activeTimelineId":"shelved","timelines":[{"id":"active","tracks":[{"id":"v"}],"clips":[{"id":"a","trackId":"v","mediaId":"slot","kind":"video","start":1.,"duration":2.,"sourceStart":7.,"speed":2.}]},{"id":"shelved","tracks":[{"id":"v"}],"clips":[{"id":"b","trackId":"v","mediaId":"slot","kind":"video","start":3.,"duration":4.,"sourceStart":2.}]}]});
    let send = |path: &str, value: Value| {
        Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap()
    };
    let unsafe_timing = fill(&original,"slot",&json!({"path":"library:new-image","name":"New image","kind":"image","hasAudio":false,"width":16,"height":16})).unwrap();
    let (_,refused,_) = support::send(&app,send("/api/compositions/concat-project/import",json!({"schemaVersion":1,"document":unsafe_timing,"mediaBindings":{"slot":{"id":"new-image","kind":"image","durationTicks":0,"width":16,"height":16,"hasAudio":false}}}))).await;
    assert_eq!(refused["status"], "converted", "{refused}");
    assert_eq!(refused["sourceDocument"], unsafe_timing);
    let (status,filled,_)=support::send(&app,send("/api/compositions/concat-project/media-slots",json!({"operation":"fillSlot","document":original,"mediaId":"slot","replacement":{"path":"library:new-image","name":"New image","kind":"image","hasAudio":false,"width":16,"height":16}}))).await;
    assert_eq!(status, StatusCode::OK, "{filled}");
    let (status,imported,_)=support::send(&app,send("/api/compositions/concat-project/import",json!({"schemaVersion":1,"document":filled["document"],"mediaBindings":{"slot":{"id":"new-image","kind":"image","durationTicks":0,"width":16,"height":16,"hasAudio":false}}}))).await;
    assert_eq!(status, StatusCode::OK, "{imported}");
    assert_eq!(imported["status"], "converted", "{imported}");
    assert_eq!(imported["sourceDocument"], filled["document"]);
    assert_eq!(imported["project"]["root"], "concat-timeline-1");
    for (index, start, duration) in [(0, 1000000, 2000000), (1, 3000000, 4000000)] {
        let node = &imported["project"]["compositions"][format!("concat-timeline-{index}")];
        let track = node["content"]["tracks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|track| track["kind"] == "image")
            .unwrap();
        let clip = &track["clips"][0];
        assert_eq!(clip["sourceId"], "new-image");
        assert_eq!(clip["timelineStartTick"], start);
        assert_eq!(clip["durationTicks"], duration);
        if index == 0 {
            assert_eq!(
                clip["concatSourceTiming"],
                json!({"sourceStart":0.,"speed":2.})
            );
        }
    }
}
