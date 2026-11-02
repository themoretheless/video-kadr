mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use video_kadr_backend::domain::concat_retime_edit::Clock;
fn rows() -> Vec<Value> {
    serde_json::from_slice(
        &std::fs::read(
            std::env::var("CONCAT_RELOCATION_ORACLE")
                .expect("run scripts/qualify-concat-relocation.py"),
        )
        .unwrap(),
    )
    .unwrap()
}
fn tick(value: f64) -> u64 {
    (value * 1e6).round() as u64
}
#[tokio::test]
#[ignore = "requires scripts/qualify-concat-relocation.py"]
async fn original_moves_keep_document_phase_through_actual_http() {
    let rows = rows();
    assert_eq!(rows.len(), 72);
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let mut matched = 0;
    let mut unrepresentable = 0;
    let mut premature_rounding_controls = 0;
    for row in rows {
        assert_eq!(row["undoRedoVerified"], true);
        let before: Clock = serde_json::from_value(row["before"][0].clone()).unwrap();
        let expected: Clock = serde_json::from_value(row["after"][0].clone()).unwrap();
        let target = row["wireTarget"].as_u64().unwrap();
        let request = Request::builder().method("POST").uri("/api/compositions/concat-retime-edit").header("content-type","application/json")
            .body(Body::from(json!({"clock":before,"rateNum":30000,"rateDen":1001,"fadeIn":0.3,"fadeOut":0.4,"tracks":{"clip":{"opacity":[{"at":0.375,"value":0.8}]}},"command":{"kind":"move","startTick":target}}).to_string())).unwrap();
        let (status, value, text) = support::send(&app, request).await;
        if tick(expected.start) != target {
            assert!(before.relocate(target).is_err(), "{}", row["name"]);
            assert!(status.is_client_error(), "{text}");
            unrepresentable += 1;
            continue;
        }
        assert_eq!(status, StatusCode::OK, "{}: {text}", row["name"]);
        let actual: Clock = serde_json::from_value(value["parts"][0]["clock"].clone()).unwrap();
        assert_eq!(
            serde_json::to_vec(&actual).unwrap(),
            serde_json::to_vec(&expected).unwrap(),
            "{}",
            row["name"]
        );
        assert_eq!(value["changed"], row["applied"], "{}", row["name"]);
        assert_eq!(value["rippleBy"], 0.0);
        assert_eq!(
            value["parts"][0]["tracks"]["clip"]["opacity"][0]["at"],
            0.375
        );
        assert_eq!(
            value["parts"][0]["tracks"]["clip"]["opacity"][0]["value"],
            0.8
        );
        assert_eq!(value["parts"][0]["fadeIn"], 0.3);
        assert_eq!(value["parts"][0]["fadeOut"], 0.4);
        let placement: video_kadr_backend::domain::composition::ClipPlacement =
            serde_json::from_value(value["parts"][0]["placement"].clone()).unwrap();
        let retained = placement.concat_retime.as_ref().unwrap();
        retained.validate_placement(&placement).unwrap();
        assert_eq!(placement.timeline_start_tick, target);
        let source = expected.placement(30000, 1001).unwrap();
        for offset in [0, 1, 100_000_000, 254_016_000_000] {
            let start = retained
                .timeline_window_ticks(target, placement.source_in_tick)
                .unwrap()
                .0;
            assert_eq!(
                retained
                    .sample(target, placement.source_in_tick, start + offset)
                    .unwrap(),
                source
                    .concat_retime
                    .as_ref()
                    .unwrap()
                    .sample(
                        source.timeline_start_tick,
                        source.source_in_tick,
                        start + offset
                    )
                    .unwrap()
            );
        }
        if actual.start.to_bits() != (target as f64 / 1e6).to_bits() {
            premature_rounding_controls += 1;
        }
        matched += 1;
    }
    assert!(matched >= 60);
    assert!(premature_rounding_controls >= 20);
    println!("{matched} original moves match HTTP clocks/placements/keys/fades; {premature_rounding_controls} controls detect phase loss; {unrepresentable} impossible wire targets explicitly refuse");
}
#[test]
fn relocation_preserves_owner_and_rejects_out_of_range() {
    let original = Clock {
        start: 2.00000049,
        duration: 3.,
        source_start: 1.5,
        speed: 1.75,
        speed_curve: None,
    };
    let moved = original.relocate(3_000_000).unwrap();
    assert_eq!(moved.start.to_bits(), 3.00000049f64.to_bits());
    assert_eq!(moved.duration.to_bits(), original.duration.to_bits());
    assert_eq!(
        moved.source_start.to_bits(),
        original.source_start.to_bits()
    );
    assert_eq!(original.start.to_bits(), 2.00000049f64.to_bits());
    assert!(original.relocate(360_000_000_001).is_err());
}

#[test]
fn ordinary_linked_native_move_preserves_all_other_authored_fields() {
    use video_kadr_backend::domain::{
        nested_clip_edit::{move_clips, ClipMove},
        nested_composition::NestedCompositionProject,
    };
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/concat-merge-preview.json")).unwrap();
    let project: NestedCompositionProject =
        serde_json::from_value(fixture["nativeFirstFree"][0]["after"].clone()).unwrap();
    let before = serde_json::to_value(&project).unwrap();
    let moves: Vec<_> = [1, 3]
        .into_iter()
        .map(|index| {
            let track = &before["compositions"]["new-timeline"]["content"]["tracks"][index];
            ClipMove {
                from_composition: "new-timeline".into(),
                to_composition: "new-timeline".into(),
                from_track_id: track["id"].as_str().unwrap().into(),
                to_track_id: track["id"].as_str().unwrap().into(),
                clip_id: track["clips"][0]["id"].as_str().unwrap().into(),
                start_us: 8_000_000,
                create_track: false,
            }
        })
        .collect();
    let result = move_clips(&project, &moves).unwrap();
    assert!(result.changed);
    let after = serde_json::to_value(&result.project).unwrap();
    for index in [1, 3] {
        let original =
            &before["compositions"]["new-timeline"]["content"]["tracks"][index]["clips"][0];
        let mut expected = original.clone();
        expected["placement"]["timelineStartTick"] = json!(8_000_000);
        assert_eq!(
            after["compositions"]["new-timeline"]["content"]["tracks"][index]["clips"][0],
            expected
        );
    }
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
}
