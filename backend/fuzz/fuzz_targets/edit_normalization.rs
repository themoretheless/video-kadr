#![no_main]

use libfuzzer_sys::fuzz_target;
use video_kadr_backend::domain::artifact_graph::Fingerprint;
use video_kadr_backend::model::EditRequest;
use video_kadr_backend::services::render::{EditPlan, SourceMediaMetadata};

fuzz_target!(|data: &[u8]| {
    if let Ok(request) = serde_json::from_slice::<EditRequest>(data) {
        let metadata = SourceMediaMetadata::new_with_audio(3840, 2160, 86_400.0, true).unwrap();
        let _ = EditPlan::compile(Fingerprint::digest(b"fuzz-source"), request, metadata);
    }
});
