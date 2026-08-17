//! The wire contract between the two halves of the app.
//!
//! `frontend-full-edit-request.json` is the literal body the frontend store
//! modules emit with every parity-wave feature switched on (produced by running
//! `featureModulePayload()` with a fully populated snapshot). `EditRequest` uses
//! `deny_unknown_fields`, so a key the frontend renames or adds fails here
//! rather than at runtime with an opaque 400.

use video_editor_backend::domain::artifact_graph::Fingerprint;
use video_editor_backend::domain::edit::EditExtensions;
use video_editor_backend::model::EditRequest;
use video_editor_backend::services::render::{EditPlan, SourceMediaMetadata};

const FULL_BODY: &str = include_str!("fixtures/frontend-full-edit-request.json");

#[test]
fn the_frontend_payload_deserializes_and_compiles() {
    let request: EditRequest = serde_json::from_str(FULL_BODY)
        .unwrap_or_else(|error| panic!("EditRequest rejected the frontend payload: {error}"));

    // Every module contributed something, so no accessor can silently be empty.
    let extensions = EditExtensions::from_request(&request).expect("extensions validate");
    assert!(!extensions.is_empty());

    let plan = EditPlan::compile(
        Fingerprint::digest(b"wire-contract"),
        request,
        SourceMediaMetadata::new_with_audio(1920, 1080, 10.0, true)
            .unwrap()
            .with_fps(Some(30.0)),
    )
    .expect("the full payload compiles into an edit plan");

    let edit = &plan.edit;
    assert!(edit.composition().is_some(), "clips");
    assert!(edit.segment_transition().is_some(), "segmentTransition");
    assert_eq!(edit.overlays().len(), 1, "overlays");
    assert_eq!(edit.titles().len(), 1, "titles");
    assert!(edit.subtitles().is_some(), "subtitles");
    assert_eq!(edit.audio_tracks().len(), 1, "audioTracks");
    assert!(edit.audio_dynamics().is_some(), "audioDynamics");
    assert!(edit.motion().is_some(), "motion");
    assert!(edit.speed_ramps().is_some(), "speedRamps");
    assert!(edit.reframe360().is_some(), "reframe360");
    assert!(edit.stabilize().is_some(), "stabilize");
    assert!(edit.lens_correction().is_some(), "lensCorrection");
    assert!(edit.color_advanced().is_some(), "colorAdvanced");
}

#[test]
fn an_untouched_request_still_carries_no_extensions() {
    let request: EditRequest = serde_json::from_str(r#"{"videoId":"x"}"#).unwrap();
    assert!(EditExtensions::from_request(&request).unwrap().is_empty());
}
