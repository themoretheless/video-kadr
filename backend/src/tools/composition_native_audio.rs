//! Trusted, already mixed selected-range stereo48k PCM delivery through ordinary profiles.
//! The caller owns PCM lifetime/admission; this compiler performs no mixing or filesystem IO.
use crate::domain::composition::CompositionTrack;
use crate::ports::{
    CompiledExportCommand, CompositionExportCommandCompiler, CompositionExportCompileRequest,
};
use anyhow::{Context, Result, ensure};
use std::path::Path;

fn clocked(value: &crate::domain::composition::AnimatableValue) -> bool {
    matches!(value, crate::domain::composition::AnimatableValue::Keyframes { track } if track.sample_clock.is_some())
}

pub fn needs_native_audio(composition: &crate::domain::composition::Composition) -> bool {
    if crate::domain::transition_export::has_independent_windows(composition) {return true}
    composition.tracks.iter().any(|track| match track {
        CompositionTrack::Audio {
            muted: false,
            clips,
            ..
        } => clips
            .iter()
            .any(|clip| clip.enabled && (clocked(&clip.gain) || clocked(&clip.pan) || clip.audio_stream.is_some() || !clip.concat_gain_keys.is_empty() || clip.dsp_effects.iter().any(|effect| !effect.bypass) || clip.placement.timeline_duration_ticks().is_ok_and(|duration|clip.fade_in_ticks>duration||clip.fade_out_ticks>duration))),
        CompositionTrack::Video{muted:false,clips,..}=>clips.iter().any(|clip|clip.enabled&&clip.source_audio_enabled&&composition.sources.get(&clip.source_id).is_some_and(|source|source.has_audio)&&(clocked(&clip.audio_gain)||clocked(&clip.audio_pan)||clip.audio_stream.is_some()||clip.independent_audio||!clip.concat_gain_keys.is_empty()||!clip.dsp_effects.is_empty()||clip.audio_fade_in_ticks>0||clip.audio_fade_out_ticks>0)),
        _ => false,
    })
}

/// Reuse native audio admission, conformance, cancellation and owned staging for flat projects.
#[allow(clippy::too_many_arguments)]
pub async fn prepare_native_audio(
    runtime: &crate::process_control::ProcessRuntime,
    staging: &Path,
    composition: &crate::domain::composition::Composition,
    inputs: &std::collections::BTreeMap<crate::domain::composition::SourceId, std::path::PathBuf>,
    range: std::ops::Range<u64>,
    timeout: std::time::Duration,
    cancel: &tokio_util::sync::CancellationToken,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<super::native_audio_export::MixedPcm> {
    use crate::domain::nested_composition::{NestedCompositionDocument, NestedCompositionProject};
    use std::collections::BTreeMap;
    let project = NestedCompositionProject {
        fonts:vec![],
        timeline_order: vec![],
        luts: vec![],
        schema_version: 1,
        root: "root".into(),
        compositions: BTreeMap::from([(
            "root".into(),
            NestedCompositionDocument {
                duration_us: None,
                content: composition.clone(),
                instances: vec![],
            },
        )]),
    };
    let layers = crate::domain::nested_audio::plan_audio_tracks(&project, "root", range.clone())?;
    crate::domain::nested_dsp::validate_export_processing(&layers)?;
    for track in &composition.tracks {
        if let CompositionTrack::Video {
            clips, transitions, ..
        } = track
        {
            ensure!(
                transitions.iter().all(|transition|transition.timeline.is_some())
                    || !clips
                        .iter()
                        .any(|clip| layers.iter().any(|layer| layer.clip.id == clip.id)),
                "native DSP export with embedded video audio transitions requires audio transition mapping"
            );
        }
    }
    let deadline=std::time::Instant::now().checked_add(timeout).context("native audio preparation deadline overflow")?;
    let remaining=||deadline.checked_duration_since(std::time::Instant::now()).context("native audio preparation timeout");
    let mut sources = BTreeMap::new();
    for layer in &layers {
        let id = layer.source_id();
        if sources.contains_key(id) { continue; }
        let path = inputs
            .get(id)
            .context("native composition audio input missing")?;
        let selected=super::native_audio_export::preferred_audio_stream(&layers,id);
        let probe=super::pcm_conform::probe_pcm_metadata_for_stream(runtime,Path::new("ffprobe"),path,remaining()?,cancel,selected).await?;
        let source=super::native_audio_export::PcmSource::from_probe(path.clone(),probe)?;
        if let Some(authored_channels)=composition.sources[id].audio_channels.filter(|_|selected.is_none()) {
            ensure!(source.channels==authored_channels,"native composition source audio channels changed");
        }
        sources.insert(id.clone(),source);
    }
    let frames = u64::try_from(
        (u128::from(
            range
                .end
                .checked_sub(range.start)
                .context("native audio range underflow")?,
        ) * 48000)
            .div_ceil(1000000),
    )?;
    super::native_audio_export::mix_project_audio_tracks(
        runtime,
        Path::new("ffmpeg"),
        staging,
        &project,
        "root",
        range,
        frames,
        &sources,
        8 * 1024 * 1024 * 1024,
        remaining()?,
        cancel,
        progress,
    )
    .await
}

pub fn compile_with_native_audio(
    request: CompositionExportCompileRequest<'_>,
    pcm: &Path,
) -> Result<CompiledExportCommand> {
    ensure!(
        pcm.is_absolute(),
        "native mixed PCM must be a trusted absolute staging path"
    );
    let pcm_argument = pcm
        .to_str()
        .context("native PCM staging path must be UTF-8")?;
    request
        .composition
        .validate_native_surface()
        .context("invalid native audio composition")?;
    let mut legacy = request.composition.clone();
    if request.output.profile.is_audio_only(){
        // PCM already owns all selected audio. Decoding discarded picture clips
        // can crash FFmpeg's split/nullsink scheduling and is not needed for timing.
        use crate::domain::composition::{Composition,TrackId,CompositionClipId,SolidColorClip,TransformSpec,AnimatableValue};
        let duration=request.composition.native_duration_ticks()?;
        legacy=Composition::new(request.composition.canvas.clone());
        legacy.time_base=request.composition.time_base;
        legacy.tracks=vec![CompositionTrack::Color{
            id:TrackId::parse("native-audio-timing")?,name:"Audio delivery timing".into(),hidden:false,locked:false,
            clips:vec![SolidColorClip{id:CompositionClipId::parse("native-audio-timing")?,timeline_start_tick:0,timeline_end_tick:duration,
                width:legacy.canvas.width,height:legacy.canvas.height,color:crate::domain::animated_color::Color{r:0,g:0,b:0,a:0},
                color_animation:None,transform:TransformSpec::default(),opacity:AnimatableValue::constant(1.),enabled:true}],
        }];
    }
    if crate::domain::transition_export::has_independent_windows(&legacy) {
        ensure!(request.output.profile.is_audio_only(), "independent visual transitions require prepared native video");
        crate::domain::transition_export::replace_independent_tracks_with_timing(&mut legacy)?;
    }
    for track in &mut legacy.tracks {
        if let CompositionTrack::Video {transitions,..}=track {
            if transitions.iter().any(|transition|transition.timeline.is_some()) {
                ensure!(request.output.profile.is_audio_only(),"independent visual transitions require prepared native video");
                transitions.retain(|transition|transition.timeline.is_none());
            }
        }
    }
    // Legacy source audio is disabled. The native mix already includes all
    // admitted source/clip envelopes and DSP; avoid reapplying it to the delivered PCM.
    for track in &mut legacy.tracks {
        if let CompositionTrack::Video{clips,..}=track{for clip in clips{clip.source_audio_enabled=false;clip.independent_audio=false;clip.audio_stream=None;clip.concat_gain_keys.clear();clip.audio_fade_in_ticks=0;clip.audio_fade_out_ticks=0;clip.dsp_effects.clear();}}
        if let CompositionTrack::Audio { clips, muted, .. } = track {
            *muted=true;
            for clip in clips {
                clip.audio_stream=None;clip.concat_gain_keys.clear();
                for effect in &mut clip.dsp_effects {
                    effect.bypass = true;
                }
            }
        }
    }
    let mut command =
        super::FfmpegCompositionExportCompiler.compile(CompositionExportCompileRequest {
            composition: &legacy,
            ..request
        })?;
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
    let audio_map = command
        .arguments
        .windows(2)
        .position(|pair| pair[0] == "-map" && matches!(pair[1].as_str(), "[aout]" | "[adelivery]"))
        .context("composition audio mapping missing")?
        + 1;
    let original_audio = command.arguments[audio_map].clone();
    command.arguments[graph_index].push_str(&format!(";{original_audio}anullsink;[{input_index}:a]atrim=duration={},asetpts=PTS-STARTPTS[a_native_delivery]", command.expected_duration_seconds));
    ensure!(
        command.arguments[graph_index].len() <= super::composition_args::MAX_FILTER_GRAPH_BYTES,
        "native composition filter graph exceeds compiler budget"
    );
    command.arguments[audio_map] = "[a_native_delivery]".into();
    command.arguments.splice(
        graph_index - 1..graph_index - 1,
        [
            "-f",
            "f32le",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-i",
            pcm_argument,
        ]
        .map(str::to_owned),
    );
    command.read_only_files.push(pcm.to_owned());
    command.read_only_files.sort();
    command.read_only_files.dedup();
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::composition::{CanvasSpec, Composition, SourceId};
    use crate::ports::{
        CompositionAudioCodec, CompositionExportProfile, CompositionExportRange,
        CompositionExportSpec,
    };
    use std::collections::BTreeMap;
    #[test]
    fn native_audio_retains_profile_and_range_without_double_mixing() {
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources = serde_json::from_value(serde_json::json!({"audio":{"id":"audio","kind":"audio","durationTicks":1000000,"width":0,"height":0,"hasAudio":true,"audioChannels":2}})).unwrap();
        composition.tracks = serde_json::from_value(serde_json::json!([{"kind":"audio","id":"a","name":"A","clips":[{"id":"c","sourceId":"audio","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"gain":{"mode":"constant","value":1.0},"pan":{"mode":"constant","value":0.0},"fadeInTicks":0,"fadeOutTicks":0,"dspEffects":[{"id":"filmcraft:delay","params":{"delay":0.1}}]}]}])).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original.wav");
        let mixed = directory.path().join("trusted mix.f32le");
        let available = std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_ok_and(|v| v.status.success());
        if available {
            assert!(
                std::process::Command::new("ffmpeg")
                    .args([
                        "-v",
                        "error",
                        "-f",
                        "lavfi",
                        "-i",
                        "anullsrc=r=48000:cl=stereo",
                        "-t",
                        "1"
                    ])
                    .arg(&original)
                    .status()
                    .unwrap()
                    .success()
            );
            let bytes: Vec<u8> = (0..24000)
                .flat_map(|_| [0.125f32, -0.25f32])
                .flat_map(f32::to_le_bytes)
                .collect();
            std::fs::write(&mixed, bytes).unwrap();
        }
        let inputs = BTreeMap::from([(SourceId::parse("audio").unwrap(), original)]);
        for (profile, file, codec) in [
            (
                CompositionExportProfile::default(),
                "/tmp/native.mp4",
                "libx264",
            ),
            (
                CompositionExportProfile::Audio {
                    codec: CompositionAudioCodec::Wav,
                },
                "/tmp/native.wav",
                "pcm_s16le",
            ),
        ] {
            let destination = directory.path().join(Path::new(file).file_name().unwrap());
            let empty = BTreeMap::new();
            let request = CompositionExportCompileRequest {
                inputs: &inputs,
                text_resources: &empty,
                color_commands: &BTreeMap::new(),
                vidstab_transforms: &BTreeMap::new(),
                destination: &destination,
                parallel_jobs: 1,
                composition: &composition,
                output: CompositionExportSpec {
                    profile,
                    range: Some(CompositionExportRange {
                        start_ticks: 250000,
                        end_ticks: 750000,
                    }),
                    video_quality: if profile.is_audio_only() { 0 } else { 23 },
                    video_bitrate_kbps: None,
                    av1_encoder: None,
                },
            };
            let command = compile_with_native_audio(request, &mixed).unwrap();
            assert_eq!(command.expected_duration_seconds, 0.5);
            assert!(command.arguments.iter().any(|value| value == codec));
            assert!(
                command
                    .arguments
                    .windows(2)
                    .any(|pair| pair == ["-map", "[a_native_delivery]"])
            );
            let graph = &command.arguments[command
                .arguments
                .iter()
                .position(|value| value == "-filter_complex")
                .unwrap()
                + 1];
            let pcm_index=usize::from(!profile.is_audio_only());
            assert!(graph.contains(&format!("[adelivery]anullsink;[{pcm_index}:a]atrim=duration=0.5")));
            if profile.is_audio_only(){assert!(!command.arguments.iter().any(|argument|argument==inputs.values().next().unwrap().to_str().unwrap()));}
            assert!(command.read_only_files.contains(&mixed));
            if available && profile.is_audio_only() {
                let rendered = std::process::Command::new("ffmpeg")
                    .args(&command.arguments)
                    .output()
                    .unwrap();
                assert!(
                    rendered.status.success(),
                    "{}",
                    String::from_utf8_lossy(&rendered.stderr)
                );
                let decoded = std::process::Command::new("ffmpeg")
                    .args(["-v", "error", "-i"])
                    .arg(&destination)
                    .args([
                        "-map",
                        "0:a:0",
                        "-c:a",
                        "pcm_f32le",
                        "-f",
                        "f32le",
                        "pipe:1",
                    ])
                    .output()
                    .unwrap();
                assert!(decoded.status.success());
                let expected: Vec<u8> = (0..24000)
                    .flat_map(|_| [0.125f32, -0.25f32])
                    .flat_map(f32::to_le_bytes)
                    .collect();
                assert_eq!(
                    decoded.stdout, expected,
                    "native selected PCM delivered once, preserving WAV profile"
                );
            }
            let CompositionTrack::Audio { clips, .. } = &composition.tracks[0] else {
                panic!("audio track")
            };
            assert!(!clips[0].dsp_effects[0].bypass);
        }
    }
}
