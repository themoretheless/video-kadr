use super::composition_native_video::{compile_with_native_video, needs_native_video};
use crate::{
    domain::composition::{CanvasSpec, Composition, TransformSpec},
    ports::{
        CompositionExportCommandCompiler, CompositionExportCompileRequest, CompositionExportRange,
        CompositionExportSpec,
    },
};
use std::{collections::BTreeMap, path::Path};

#[test]
fn graphic_only_delivery_needs_no_media_and_preserves_selected_duration() {
    let mut composition = Composition::new(CanvasSpec::default());
    composition.tracks = serde_json::from_value(serde_json::json!([{
        "kind":"graphic", "id":"graphics", "name":"Graphics", "clips":[{
            "id":"title", "timelineStartTick":250000, "timelineEndTick":1250000,
            "width":64, "height":48, "graphic":{"layers":[]},
            "transform":TransformSpec::default(), "opacity":{"mode":"constant","value":1.0}
        }]
    }]))
    .unwrap();
    composition.validate().unwrap();
    assert!(needs_native_video(&composition));
    let inputs = BTreeMap::new();
    let text = BTreeMap::new();
    let colors = BTreeMap::new();
    let stabilization = BTreeMap::new();
    let request = || CompositionExportCompileRequest {
        inputs: &inputs,
        text_resources: &text,
        color_commands: &colors,
        vidstab_transforms: &stabilization,
        destination: Path::new("/tmp/graphic-delivery.mp4"),
        parallel_jobs: 1,
        composition: &composition,
        output: CompositionExportSpec {
            range: Some(CompositionExportRange {
                start_ticks: 500000,
                end_ticks: 1000000,
            }),
            profile: Default::default(),
            video_quality: 23,
            video_bitrate_kbps: None,
            av1_encoder: None,
        },
    };
    assert!(
        super::FfmpegCompositionExportCompiler
            .compile(request())
            .is_err()
    );
    for pcm in [None, Some(Path::new("/tmp/graphic-audio.f32le"))] {
        let command =
            compile_with_native_video(request(), Path::new("/tmp/graphic-native.mkv"), pcm)
                .unwrap();
        assert_eq!(command.expected_duration_seconds, 0.5);
        assert!(
            command
                .arguments
                .windows(2)
                .any(|pair| pair == ["-map", "[v_native_delivery]"])
        );
        assert!(
            command
                .read_only_files
                .contains(&Path::new("/tmp/graphic-native.mkv").to_path_buf())
        );
    }
    assert!(composition.sources.is_empty());
    assert!(matches!(
        composition.tracks[0],
        crate::domain::composition::CompositionTrack::Graphic { .. }
    ));
}
