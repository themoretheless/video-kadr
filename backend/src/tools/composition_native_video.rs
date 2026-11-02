//! Native selected-range visual rendering delivered through ordinary export profiles.
use crate::domain::composition::{Composition, CompositionTrack, SourceId};
use crate::ports::{
    CompiledExportCommand, CompositionExportCommandCompiler, CompositionExportCompileRequest,
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    ops::Range,
    path::{Path, PathBuf},
    time::Duration,
};

pub fn needs_native_video(composition: &Composition) -> bool {
    crate::domain::flat_surface::requires_native_video(composition)
}

/// Own the prepared movie until its delivery command has terminated.
pub struct PreparedNativeVideo {
    path: PathBuf,
}
impl PreparedNativeVideo {
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for PreparedNativeVideo {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn prepare_native_video(
    runtime: &crate::process_control::ProcessRuntime,
    staging: &Path,
    composition: &Composition,
    inputs: &BTreeMap<SourceId, PathBuf>,
    range: Range<u64>,
    timeout: Duration,
    cancel: &tokio_util::sync::CancellationToken,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<PreparedNativeVideo> {
    use crate::domain::nested_composition::{NestedCompositionDocument, NestedCompositionProject};
    ensure!(
        staging.is_absolute(),
        "native video staging must be absolute"
    );
    let owned = PreparedNativeVideo {
        path: staging.join(format!("{}.native-video.mkv", uuid::Uuid::new_v4())),
    };
    let project = NestedCompositionProject {
        project_media: Default::default(),
        fonts:vec![],
        timeline_order: vec![],
        schema_version: 1,
        root: "root".into(),
        luts: vec![],
        compositions: BTreeMap::from([(
            "root".into(),
            NestedCompositionDocument {
                duration_us: None,
                content: composition.clone(),
                instances: vec![],
            },
        )]),
    };
    super::native_rgba_export::encode_project_video_only(
        runtime,
        Path::new("ffmpeg"),
        &owned.path,
        project,
        "root".into(),
        BTreeMap::new(),
        inputs.clone(),
        range,
        8 * 1024 * 1024 * 1024,
        timeout,
        cancel,
        progress,
    )
    .await?;
    Ok(owned)
}

/// Replace the legacy visual delivery with an already rendered selected-range movie.
/// Legacy audio remains intact, or may be replaced by an independently prepared DSP mix.
pub fn compile_with_native_video(
    request: CompositionExportCompileRequest<'_>,
    movie: &Path,
    pcm: Option<&Path>,
) -> Result<CompiledExportCommand> {
    ensure!(
        movie.is_absolute(),
        "native video must be a trusted absolute staging path"
    );
    ensure!(
        !request.output.profile.is_audio_only(),
        "native video requires a video delivery profile"
    );
    request
        .composition
        .validate()
        .context("invalid native video composition")?;
    ensure!(
        request.composition.time_base == 1000000,
        "native video requires microsecond composition time base"
    );
    let movie_argument = movie
        .to_str()
        .context("native movie staging path must be UTF-8")?;
    let mut legacy = request.composition.clone();
    if crate::domain::transition_export::sparse_primary_video_track(&legacy).is_some(){
        ensure!(pcm.is_some(),"sparse native video requires prepared native audio delivery");
        crate::domain::transition_export::replace_sparse_primary_with_timing(&mut legacy)?;
    }
    if crate::domain::transition_export::has_independent_windows(&legacy) {
        ensure!(pcm.is_some(), "independent video transitions require prepared native audio delivery");
        crate::domain::transition_export::replace_independent_tracks_with_timing(&mut legacy)?;
    }
    crate::domain::transition_export::replace_native_graphics_with_timing(&mut legacy);
    // The discarded graph supplies duration, profile and ordinary audio routing.
    // All visual effects execute exactly once in the prepared native movie.
    for track in &mut legacy.tracks {
        if let CompositionTrack::Image { clips, .. } = track {
            for clip in clips { clip.effects.clear(); }
        }
        if let CompositionTrack::Video { clips, transitions, .. } = track {
            if transitions.iter().any(|transition| transition.timeline.is_some()) {
                ensure!(pcm.is_some(), "independent video transitions require prepared native audio delivery");
                // Both streams are already rendered. Only timing/profile placeholders remain
                // in the discarded graph; the legacy compiler has no independent-window clock.
                transitions.retain(|transition| transition.timeline.is_none());
            }
            for clip in clips {
                clip.effects.clear();
            }
        }
    }
    let request = CompositionExportCompileRequest {
        composition: &legacy,
        ..request
    };
    let mut command = if let Some(pcm) = pcm {
        super::composition_native_audio::compile_with_native_audio(request, pcm)?
    } else {
        super::FfmpegCompositionExportCompiler.compile(request)?
    };
    let input_index = command
        .arguments
        .iter()
        .filter(|value| value.as_str() == "-i")
        .count();
    let graph_index = command
        .arguments
        .iter()
        .position(|value| value == "-filter_complex")
        .context("composition filter graph missing")?
        + 1;
    let map_index = command
        .arguments
        .windows(2)
        .position(|pair| pair[0] == "-map" && matches!(pair[1].as_str(), "[vout]" | "[vdelivery]"))
        .context("composition video mapping missing")?
        + 1;
    let original_video = command.arguments[map_index].clone();
    command.arguments[graph_index].push_str(&format!(
        ";{original_video}nullsink;[{input_index}:v]trim=duration={},setpts=PTS-STARTPTS[v_native_delivery]",
        command.expected_duration_seconds));
    ensure!(
        command.arguments[graph_index].len() <= super::composition_args::MAX_FILTER_GRAPH_BYTES,
        "native composition filter graph exceeds compiler budget"
    );
    command.arguments[map_index] = "[v_native_delivery]".into();
    command.arguments.splice(
        graph_index - 1..graph_index - 1,
        ["-i", movie_argument].map(str::to_owned),
    );
    command.read_only_files.push(movie.to_owned());
    command.read_only_files.sort();
    command.read_only_files.dedup();
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::composition::{CanvasSpec, TransformSpec};
    use crate::ports::{CompositionExportRange, CompositionExportSpec};
    #[test]
    fn native_visual_delivery_preserves_range_audio_and_effect_admission() {
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources = serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":"video","durationTicks":1000000,"width":16,"height":16,"hasAudio":false}})).unwrap();
        composition.tracks = serde_json::from_value(serde_json::json!([{"kind":"video","id":"v","name":"V","clips":[{"id":"c","sourceId":"source","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"cineon_converter","parameters":{}}]}]}])).unwrap();
        assert!(needs_native_video(&composition));
        let inputs = BTreeMap::from([(
            SourceId::parse("source").unwrap(),
            PathBuf::from("/tmp/source.mkv"),
        )]);
        let empty = BTreeMap::new();
        for pcm in [None, Some(Path::new("/tmp/trusted.f32le"))] {
            let request = CompositionExportCompileRequest {
                inputs: &inputs,
                text_resources: &empty,
                color_commands: &BTreeMap::new(),
                vidstab_transforms: &BTreeMap::new(),
                destination: Path::new("/tmp/delivery.mp4"),
                parallel_jobs: 1,
                composition: &composition,
                output: CompositionExportSpec {
                    range: Some(CompositionExportRange {
                        start_ticks: 250000,
                        end_ticks: 750000,
                    }),
                    profile: Default::default(),
                    video_quality: 23,
                    video_bitrate_kbps: None,
                    av1_encoder: None,
                },
            };
            let command =
                compile_with_native_video(request, Path::new("/tmp/native.mkv"), pcm).unwrap();
            assert_eq!(command.expected_duration_seconds, 0.5);
            assert!(
                command
                    .arguments
                    .windows(2)
                    .any(|p| p == ["-map", "[v_native_delivery]"])
            );
            let graph = &command.arguments[command
                .arguments
                .iter()
                .position(|v| v == "-filter_complex")
                .unwrap()
                + 1];
            let last_input = command
                .arguments
                .iter()
                .filter(|v| v.as_str() == "-i")
                .count()
                - 1;
            assert!(graph.contains(&format!("[vdelivery]nullsink;[{last_input}:v]trim=duration=0.5,setpts=PTS-STARTPTS[v_native_delivery]")));
            assert!(command.arguments.windows(2).any(|p| p[0] == "-map"
                && p[1]
                    == if pcm.is_some() {
                        "[a_native_delivery]"
                    } else {
                        "[adelivery]"
                    }));
            assert!(command.arguments.iter().any(|v| v == "libx264"));
            assert!(
                command
                    .read_only_files
                    .contains(&PathBuf::from("/tmp/native.mkv"))
            );
            if let Some(pcm) = pcm {
                assert!(command.read_only_files.contains(&pcm.to_owned()));
            }
        }
        let CompositionTrack::Video { clips, hidden, .. } = &mut composition.tracks[0] else {
            panic!("video")
        };
        *hidden = true;
        assert!(clips[0].enabled);
        assert!(!needs_native_video(&composition));
        let CompositionTrack::Video { clips, hidden, .. } = &mut composition.tracks[0] else {
            panic!("video")
        };
        *hidden = false;
        clips[0].enabled = false;
        assert!(!needs_native_video(&composition));
    }
}
