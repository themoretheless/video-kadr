//! Root-time placement for nested audio. DSP and media decoding consume this plan separately.
use super::{
    composition::{
        AnimatableValue, AudioClip, AudioVoiceEffect, CompositionTrack, PlaybackMode, SourceId,
        SpeedRampAudioPolicy,
    },
    nested_composition::{sample_value, NestedCompositionProject},
    video_clock::source_tick,
};
use anyhow::{ensure, Context, Result};
use std::ops::Range;

#[derive(Debug, Clone)]
pub struct VideoTransitionAudio {
    pub group: String,
    pub root_offset_us: i128,
    pub transitions: Vec<super::composition::ClipTransition>,
    pub ordinary_enabled: bool,
}
#[derive(Debug,Clone,Copy)]
pub struct SourceCurveOwner { pub root_offset_us:i128 }

#[derive(Debug, Clone)]
pub struct NestedAudioLayer {
    pub concat_retime_owner:Option<SourceCurveOwner>,
    /// Unrounded source coordinates survive planning until the source frame worker consumes them.
    pub concat_retime_piece:Option<super::concat_retime::AudioPiece>,
    pub gain_track:Option<super::concat_animation::PreparedTrack>,
    /// Concat export starts overlong fade-out at zero; live playback uses the clip tail.
    pub export_fade_clock:bool,
    pub video_transitions: Option<VideoTransitionAudio>,
    pub composition: String,
    pub instance_path: Vec<String>,
    /// Audible placement in focused composition time, clipped to every ancestor window.
    pub root_range_us: Range<u64>,
    /// Original clip envelope position at the beginning of root_range_us.
    pub clip_local_start_us: u64,
    /// Full authored clip retained for voice, pitch, tone, ducking and crossfade processing.
    pub clip: AudioClip,
    /// Authored neighbor transition, retained separately from clip/automation clocks.
    pub crossfade: super::audio_transition::AudioCrossfadePlan,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NestedAudioSample {
    pub source_time_us: u64,
    pub clip_local_us: u64,
    pub gain: f64,
    pub pan: f64,
}
impl NestedAudioLayer {
    pub fn source_id(&self) -> &SourceId {
        &self.clip.source_id
    }
    pub fn sample(&self, root_time_us: u64) -> Result<Option<NestedAudioSample>> {
        ensure!(self.concat_retime_piece.is_none()&&self.concat_retime_owner.is_none(),"source curve audio piece needs whole-frame preparation before sampling");
        if !self.root_range_us.contains(&root_time_us) {
            return Ok(None);
        }
        let local = self
            .clip_local_start_us
            .checked_add(root_time_us - self.root_range_us.start)
            .context("nested audio envelope clock overflow")?;
        let extended_duration = self.clip.placement.timeline_duration_ticks()?.checked_add(self.crossfade.head_ticks).and_then(|v| v.checked_add(self.crossfade.tail_ticks)).context("crossfade duration overflow")?;
        let authored_local = local.saturating_sub(self.crossfade.head_ticks);
        let mut source_placement = self.clip.placement.clone();
        if self.crossfade.head_ticks != 0 || self.crossfade.tail_ticks != 0 {
            ensure!(source_placement.speed == 1.0 && source_placement.speed_ramp.is_none() && !self.clip.reversed, "native crossfade requires forward unit speed");
            source_placement.timeline_start_tick = source_placement.timeline_start_tick.checked_sub(self.crossfade.head_ticks).context("crossfade timeline head underflow")?;
            source_placement.source_in_tick = source_placement.source_in_tick.checked_sub(self.crossfade.head_ticks).context("crossfade source head underflow")?;
            source_placement.source_out_tick = source_placement.source_out_tick.checked_add(self.crossfade.tail_ticks).context("crossfade source tail overflow")?;
        }
        let timeline = source_placement
            .timeline_start_tick
            .checked_add(local)
            .context("nested audio source clock overflow")?;
        let mode = if self.clip.reversed {
            PlaybackMode::Reverse
        } else {
            PlaybackMode::Forward
        };
        let Some(source_time_us) = source_tick(&source_placement, mode, timeline)? else {
            return Ok(None);
        };
        let duration = self.clip.placement.timeline_duration_ticks()?;
        let local = authored_local.min(duration);
        let fade_in = if self.clip.fade_in_ticks == 0 {
            1.0
        } else {
            (local as f64 / self.clip.fade_in_ticks as f64).min(1.0)
        };
        let fade_out = if self.clip.fade_out_ticks == 0 {
            1.0
        } else {
            ((if self.export_fade_clock {duration.max(self.clip.fade_out_ticks)}else{duration} - local) as f64 / self.clip.fade_out_ticks as f64).min(1.0)
        };
        let authored_gain = if let Some(track)=&self.gain_track {track.value_at(local as f64/duration as f64,sample_value(&self.clip.gain,local)?)?.max(0.)}else{sample_value(&self.clip.gain, local)?};
        ensure!(
            authored_gain.is_finite() && authored_gain>=0. && (self.gain_track.is_some() || authored_gain<=16.),
            "invalid nested audio gain sample"
        );
        let mut transition_gain = 1.0;
        if self.crossfade.fade_in_ticks > 0 {
            let progress = (self.clip_local_start_us + root_time_us - self.root_range_us.start) as f64 / self.crossfade.fade_in_ticks as f64;
            transition_gain *= f64::from(self.crossfade.in_curve.gains(progress.clamp(0.0,1.0) as f32)?.1);
        }
        if self.crossfade.fade_out_ticks > 0 {
            let position = self.clip_local_start_us + root_time_us - self.root_range_us.start;
            let start = extended_duration.checked_sub(self.crossfade.fade_out_ticks).context("crossfade exceeds clip")?;
            let progress = (position as f64 - start as f64) / self.crossfade.fade_out_ticks as f64;
            transition_gain *= f64::from(self.crossfade.out_curve.gains(progress.clamp(0.0,1.0) as f32)?.0).max(0.0);
        }
        let gain = authored_gain * self.clip.role_gain * fade_in * fade_out * transition_gain;
        let pan = sample_value(&self.clip.pan, local)?;
        ensure!(
            gain.is_finite() && gain >= 0.0 && pan.is_finite() && (-1.0..=1.0).contains(&pan),
            "invalid nested audio automation sample"
        );
        Ok(Some(NestedAudioSample {
            source_time_us,
            clip_local_us: local,
            gain,
            pan,
        }))
    }
}

/// Build a bounded explicit-audio and legacy/opt-in embedded-video audio plan.
/// Unlike visual traversal,
/// opacity, scale and blend mode do not mute audio. Muted/solo tracks use host rules.
pub fn plan_audio_tracks(
    project: &NestedCompositionProject,
    focus: &str,
    range: Range<u64>,
) -> Result<Vec<NestedAudioLayer>> {
    let durations = project.durations_us()?;
    let duration = *durations.get(focus).context("nested audio focus missing")?;
    ensure!(
        range.start < range.end && range.end <= duration,
        "invalid nested audio export range"
    );
    let mut output = Vec::new();
    walk(
        project,
        &durations,
        focus,
        0,
        range,
        &[],
        &mut 0,
        &mut output,
    )?;
    Ok(output)
}
/// Keep preceding/following clip context for latency compensation and seek preroll.
/// Only layers audible in the selected interval are returned; their ancestor windows
/// remain full-sized. The caller must still crop output to the requested interval.
pub fn plan_audio_tracks_with_context(
    project: &NestedCompositionProject,
    focus: &str,
    range: Range<u64>,
) -> Result<Vec<NestedAudioLayer>> {
    let durations = project.durations_us()?;
    let duration = *durations.get(focus).context("nested audio focus missing")?;
    ensure!(range.start < range.end && range.end <= duration, "invalid nested audio export range");
    let mut layers = plan_audio_tracks(project, focus, 0..duration)?;
    layers.retain(|layer| layer.root_range_us.start < range.end && layer.root_range_us.end > range.start);
    Ok(layers)
}
fn clipped(offset: i128, local: Range<u64>, window: &Range<u64>) -> Option<Range<u64>> {
    let start = (offset + i128::from(local.start)).max(i128::from(window.start));
    let end = (offset + i128::from(local.end)).min(i128::from(window.end));
    (start < end).then(|| start as u64..end as u64)
}
#[allow(clippy::too_many_arguments)]
fn walk(
    project: &NestedCompositionProject,
    durations: &std::collections::BTreeMap<String, u64>,
    id: &str,
    offset: i128,
    window: Range<u64>,
    path: &[String],
    visited: &mut usize,
    output: &mut Vec<NestedAudioLayer>,
) -> Result<()> {
    *visited += 1;
    ensure!(*visited <= 4096, "nested audio instance budget exceeded");
    let Some(window) = clipped(offset, 0..durations[id], &window) else {
        return Ok(());
    };
    let document = &project.compositions[id];
    let has_solo = document.content.tracks.iter().any(|t| {
        matches!(
            t,
            CompositionTrack::Audio {
                muted: false,
                solo: true,
                ..
            }
        )
    });
    for track in &document.content.tracks {
        let CompositionTrack::Audio {
            muted: false,
            solo,
            clips,
            ..
        } = track
        else {
            continue;
        };
        if has_solo && !solo {
            continue;
        }
        let crossfades = super::audio_transition::plan_crossfades(clips)?;
        for clip in clips.iter().filter(|clip| clip.enabled) {
            append_layer(id, offset, &window, path, clip, crossfades.get(&clip.id).copied().unwrap_or_default(), output)?;
        }
    }
    // Legacy clips retain primary-only audio. Imported independent clips and explicitly
    // authored DSP chains participate on every video lane; visibility does not mute those.
    // Audio solo retains its existing relation to embedded video audio.
    let primary = document
        .content
        .tracks
        .iter()
        .rev()
        .find(|track| match track {
            CompositionTrack::Video {
                hidden: false,
                clips,
                transitions,
                ..
            } => clips.iter().any(|clip| clip.enabled) || transitions.iter().any(|transition|transition.timeline.is_some()),
            CompositionTrack::Image {
                hidden: false,
                clips,
                ..
            } => clips.iter().any(|clip| clip.enabled),
            CompositionTrack::Text {
                hidden: false,
                clips,
                ..
            } => clips.iter().any(|clip| clip.enabled),
            CompositionTrack::Graphic { hidden: false, clips, .. } => clips.iter().any(|clip| clip.enabled),
            CompositionTrack::Color { hidden: false, clips, .. } => clips.iter().any(|clip| clip.enabled),
            _ => false,
        });
    for track in &document.content.tracks {
    let is_primary=primary.is_some_and(|candidate|std::ptr::eq(candidate,track));
    if let CompositionTrack::Video {
        muted: false,
        id: track_id,
        transitions,
        clips,
        ..
    } = track
    {
        let explicit = transitions.iter().any(|transition|transition.timeline.is_some());
        for video in clips.iter().filter(|clip| {
            (clip.enabled || (explicit && transitions.iter().any(|t|t.timeline.is_some()&&(t.from_clip_id.as_ref()==Some(&clip.id)||t.to_clip_id.as_ref()==Some(&clip.id)))))
                && (is_primary||clip.audio_stream.is_some()||clip.independent_audio||!clip.dsp_effects.is_empty()||!clip.concat_gain_keys.is_empty()||clip.audio_fade_in_ticks>0||clip.audio_fade_out_ticks>0)
                && clip.source_audio_enabled
                && !matches!(clip.playback_mode, PlaybackMode::Freeze { .. })
        }) {
            if !document.content.sources[&video.source_id].has_audio {
                continue;
            }
            let clip = AudioClip {
                audio_stream:video.audio_stream,detached_from:None,concat_detached_order:None,concat_gain_keys:video.concat_gain_keys.clone(),dsp_effects: video.dsp_effects.clone(),
                id: video.id.clone(),
                source_id: video.source_id.clone(),
                placement: video.placement.clone(),
                gain: video.audio_gain.clone(),
                pan: video.audio_pan.clone(),
                role_gain: 1.0,
                reversed: video.playback_mode == PlaybackMode::Reverse,
                fade_in_ticks: video.audio_fade_in_ticks,
                fade_out_ticks: video.audio_fade_out_ticks,
                voice_effect: AudioVoiceEffect::None,
                pitch_semitones: 0.0,
                tone_db: 0.0,
                crossfade_in_ticks: 0,
                crossfade_curve: crate::domain::audio_transition::AudioTransitionCurve::ConstantGain,
                ducking: None,
                enabled: true,
            };
            let source_overlap=transitions.iter().any(|transition|transition.concat_overlap.is_some());
            ensure!(!source_overlap||transitions.iter().all(|transition|transition.concat_overlap.is_some()),"Mixed Concat and native audio transition ownership");
            if source_overlap&&!video.enabled{continue;}
            let clip=if source_overlap{super::concat_project_transitions::audio_clip(video,transitions,clip)?}else{clip};
            let before=output.len();
            append_layer(id, offset, &window, path, &clip, Default::default(), output)?;
            if explicit && !source_overlap && output.len()>before {
                output.last_mut().expect("appended video audio").video_transitions=Some(VideoTransitionAudio{
                    group:format!("{id}/{}/{}",path.join("/"),track_id.as_str()),root_offset_us:offset,
                    transitions:transitions.clone(),ordinary_enabled:video.enabled,
                });
            }
        }
    }
    }
    for instance in &document.instances {
        let Some(child_window) = clipped(
            offset,
            instance.timing.timeline_start_us..instance.timing.timeline_end_us()?,
            &window,
        ) else {
            continue;
        };
        let child_offset = offset + i128::from(instance.timing.timeline_start_us)
            - i128::from(instance.timing.source_in_us);
        let mut child_path = path.to_vec();
        child_path.push(instance.id.clone());
        walk(
            project,
            durations,
            &instance.timing.composition,
            child_offset,
            child_window,
            &child_path,
            visited,
            output,
        )?;
    }
    Ok(())
}

/// Retain a complete source owner over the ancestor window. Processor tails
/// and whole-frame overlap cannot be clipped to individual authored pieces.
pub fn source_curve_owner_layer(id:&str,offset:i128,window:Range<u64>,path:&[String],clip:&AudioClip)->Result<NestedAudioLayer>{
    ensure!(window.start<window.end&&i128::from(window.start)>=offset,"invalid source owner ancestor window");
    clip.placement.concat_retime.as_ref().context("missing source owner curve")?.validate_placement(&clip.placement)?;
    Ok(NestedAudioLayer{concat_retime_owner:Some(SourceCurveOwner{root_offset_us:offset}),concat_retime_piece:None,gain_track:None,export_fade_clock:false,video_transitions:None,composition:id.into(),instance_path:path.to_vec(),root_range_us:window,clip_local_start_us:0,clip:clip.clone(),crossfade:Default::default()})
}

fn append_layer(
    id: &str,
    offset: i128,
    window: &Range<u64>,
    path: &[String],
    clip: &AudioClip,
    crossfade: super::audio_transition::AudioCrossfadePlan,
    output: &mut Vec<NestedAudioLayer>,
) -> Result<()> {
    if clip.placement.concat_retime.is_some(){
        ensure!(crossfade.head_ticks==0&&crossfade.tail_ticks==0&&crossfade.fade_in_ticks==0&&crossfade.fade_out_ticks==0,"source curve audio/native crossfade ownership conflict");
        ensure!(output.len()<4096,"nested audio layer budget exceeded");
        if clip.role_gain!=0. && !(clip.concat_gain_keys.is_empty()&&matches!(clip.gain,AnimatableValue::Constant{value}if value==0.)){
            output.push(source_curve_owner_layer(id,offset,window.clone(),path,clip)?);
        }
        return Ok(());
    }

    if clip
        .placement
        .speed_ramp
        .as_ref()
        .is_some_and(|r| r.audio_policy == SpeedRampAudioPolicy::Mute)
    {
        return Ok(());
    }
    if (clip.concat_gain_keys.is_empty() && matches!(&clip.gain, AnimatableValue::Constant { value } if *value == 0.0))
        || clip.role_gain == 0.0
    {
        return Ok(());
    }
    let Some(root_range_us) = clipped(
        offset,
        clip.placement.timeline_start_tick.checked_sub(crossfade.head_ticks).context("crossfade timeline underflow")?..clip.placement.timeline_end_tick()?.checked_add(crossfade.tail_ticks).context("crossfade end overflow")?,
        window,
    ) else {
        return Ok(());
    };
    ensure!(output.len() < 4096, "nested audio layer budget exceeded");
    let local =
        i128::from(root_range_us.start) - offset - i128::from(clip.placement.timeline_start_tick) + i128::from(crossfade.head_ticks);
    output.push(NestedAudioLayer {
        video_transitions: None, concat_retime_piece:None, concat_retime_owner:None,export_fade_clock:false,gain_track:if clip.concat_gain_keys.is_empty(){None}else{Some(super::concat_animation::PreparedTrack::new(&clip.concat_gain_keys)?)},
            composition: id.into(),
        instance_path: path.to_vec(),
        root_range_us,
        clip_local_start_us: u64::try_from(local)?,
        clip: clip.clone(),
        crossfade,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        composition::{CanvasSpec, Composition, TransformSpec},
        composition_graph::CompositionReferenceTiming,
        nested_composition::{CompositionInstance, NestedCompositionDocument},
    };
    use std::collections::BTreeMap;
    fn fixture() -> NestedCompositionProject {
        let mut child = Composition::new(CanvasSpec::default());
        child.sources = serde_json::from_value(serde_json::json!({"tone":{"id":"tone","kind":"audio","durationTicks":2000000,"width":0,"height":0,"hasAudio":true,"audioChannels":2}})).unwrap();
        child.tracks = serde_json::from_value(serde_json::json!([{"kind":"audio","id":"audio","name":"Audio","clips":[{"id":"tone-clip","sourceId":"tone","placement":{"timelineStartTick":100000,"sourceInTick":100000,"sourceOutTick":1100000,"speed":1.0},"gain":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":1000000,"value":1.0}]}},"pan":{"mode":"constant","value":-0.5},"roleGain":2.0,"fadeInTicks":500000,"fadeOutTicks":250000}]}])).unwrap();
        let instance = CompositionInstance {
            id: "group".into(),
            timing: CompositionReferenceTiming {
                composition: "child".into(),
                timeline_start_us: 500000,
                source_in_us: 300000,
                source_out_us: 1300000,
            },
            animation_offset_us: 0,
            transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(0.0),
            blend_mode: super::super::composition::BlendMode::Normal,
        };
        NestedCompositionProject {
            project_media: Default::default(),
            fonts:vec![],
            timeline_order: vec![],
            luts:vec![],
            schema_version: 1,
            root: "root".into(),
            compositions: BTreeMap::from([
                (
                    "root".into(),
                    NestedCompositionDocument {
                        duration_us: Some(1500000),
                        content: Composition::new(CanvasSpec::default()),
                        instances: vec![instance],
                    },
                ),
                (
                    "child".into(),
                    NestedCompositionDocument {
                        duration_us: Some(1200000),
                        content: child,
                        instances: vec![],
                    },
                ),
            ]),
        }
    }
    #[test]
    fn nested_layers_retain_neighbor_crossfades_without_rebasing_authored_curves() {
        let mut project=fixture();
        let child=&mut project.compositions.get_mut("child").unwrap().content;
        let CompositionTrack::Audio { clips,.. }=&mut child.tracks[0] else { unreachable!() };
        let mut first=clips[0].clone();
        first.placement.source_out_tick=500000;
        first.gain=AnimatableValue::constant(1.0);
        first.dsp_effects=vec![serde_json::from_value(serde_json::json!({"id":"amplify","animation":{"gain":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":-6.0},{"tick":300000,"value":0.0}]}}}})).unwrap()];
        first.fade_in_ticks=0; first.fade_out_ticks=0;
        let mut second=first.clone();
        second.id=super::super::composition::CompositionClipId::new();
        second.placement.timeline_start_tick=500000;
        second.placement.source_in_tick=500000;
        second.placement.source_out_tick=900000;
        second.crossfade_in_ticks=100001;
        second.crossfade_curve=super::super::audio_transition::AudioTransitionCurve::ExponentialFade;
        let gain=second.gain.clone();
        let dsp=second.dsp_effects.clone();
        *clips=vec![first,second];
        let layers=plan_audio_tracks(&project,"child",0..1200000).unwrap();
        assert_eq!(layers.len(),2);
        assert_eq!(layers[0].crossfade.tail_ticks,50001);
        assert_eq!(layers[1].crossfade.head_ticks,50000);
        assert_eq!(layers[1].clip.gain,gain);
        assert_eq!(layers[1].clip.dsp_effects,dsp);
        assert_eq!(layers[0].root_range_us,100000..550001);
        assert_eq!(layers[1].root_range_us,450000..900000);
        let out=layers[0].sample(500000).unwrap().unwrap();
        let input=layers[1].sample(500000).unwrap().unwrap();
        assert_eq!(out.source_time_us,500000);
        assert_eq!(input.source_time_us,500000);
        assert_eq!(input.clip_local_us,0);
        let (a,b)=super::super::audio_transition::AudioTransitionCurve::ExponentialFade.gains(50000.0/100001.0).unwrap();
        assert_eq!(out.gain,2.0*f64::from(a));
        assert_eq!(input.gain,2.0*f64::from(b));
        assert_eq!(layers[1].sample(450000).unwrap().unwrap().source_time_us,450000);
        assert!(layers[0].sample(550001).unwrap().is_none());
        assert_eq!(layers[1].clip.placement.timeline_start_tick,500000);
        assert_eq!(layers[1].crossfade.in_curve,super::super::audio_transition::AudioTransitionCurve::ExponentialFade);
        // Until extended-source sampling is bound, admission must still reject crossfade.
        assert!(super::super::nested_pcm::validate_processing(&layers).is_err());
    }
    fn embedded_fixture() -> NestedCompositionProject {
        let mut project = fixture();
        let child = project.compositions.get_mut("child").unwrap();
        child.content.sources = serde_json::from_value(serde_json::json!({"tone":{"id":"tone","kind":"video","durationTicks":2000000,"width":2,"height":2,"hasAudio":true,"audioChannels":2}})).unwrap();
        child.content.tracks.push(serde_json::from_value(serde_json::json!({"kind":"video","id":"video","name":"Video","clips":[{"id":"video-clip","sourceId":"tone","placement":{"timelineStartTick":100000,"sourceInTick":100000,"sourceOutTick":1100000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":0.0},"blendMode":"normal","audioGain":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":1000000,"value":1.0}]}},"audioPan":{"mode":"constant","value":0.25}}]})).unwrap());
        project
    }
    #[test]
    fn embedded_primary_video_uses_nested_source_clock_and_audio_envelopes() {
        let mut project = embedded_fixture();
        let plan = plan_audio_tracks(&project, "root", 600000..1000000).unwrap();
        assert_eq!(plan.len(), 2);
        let embedded = plan
            .iter()
            .find(|l| l.clip.id.as_str() == "video-clip")
            .unwrap();
        assert_eq!(embedded.instance_path, ["group"]);
        let sample = embedded.sample(700000).unwrap().unwrap();
        assert_eq!(sample.source_time_us, 500000);
        assert_eq!(sample.clip_local_us, 400000);
        assert!((sample.gain - 0.4).abs() < 1e-12);
        assert_eq!(sample.pan, 0.25);
        let CompositionTrack::Video { clips, .. } = &mut project
            .compositions
            .get_mut("child")
            .unwrap()
            .content
            .tracks[1]
        else {
            panic!()
        };
        clips[0].playback_mode = PlaybackMode::Reverse;
        let plan = plan_audio_tracks(&project, "root", 600000..1000000).unwrap();
        assert_eq!(
            plan.iter()
                .find(|l| l.clip.id.as_str() == "video-clip")
                .unwrap()
                .sample(700000)
                .unwrap()
                .unwrap()
                .source_time_us,
            699999
        );
        let split = project
            .split_instance("root", "group", "right", 800000)
            .unwrap();
        let after = plan_audio_tracks(&split, "root", 600000..1000000).unwrap();
        for time in [799999, 800000, 800001] {
            let embedded_samples = |plan: &[NestedAudioLayer]| {
                plan.iter()
                    .filter(|l| l.clip.id.as_str() == "video-clip")
                    .filter_map(|l| l.sample(time).unwrap())
                    .collect::<Vec<_>>()
            };
            assert_eq!(embedded_samples(&plan), embedded_samples(&after));
        }
    }
    #[test]
    fn embedded_audio_respects_enabled_mute_hidden_freeze_and_primary_overlay_rule() {
        for gate in 0..5 {
            let mut project = embedded_fixture();
            let CompositionTrack::Video {
                clips,
                muted,
                hidden,
                ..
            } = &mut project
                .compositions
                .get_mut("child")
                .unwrap()
                .content
                .tracks[1]
            else {
                panic!()
            };
            match gate {
                0 => clips[0].source_audio_enabled = false,
                1 => *muted = true,
                2 => *hidden = true,
                3 => {
                    clips[0].playback_mode = PlaybackMode::Freeze {
                        source_tick: 100000,
                    }
                }
                _ => clips[0].enabled = false,
            }
            let plan = plan_audio_tracks(&project, "root", 600000..1000000).unwrap();
            assert_eq!(plan.len(), 1, "gate {gate}");
            assert_eq!(plan[0].clip.id.as_str(), "tone-clip");
        }
        let mut project = embedded_fixture();
        let child = project.compositions.get_mut("child").unwrap();
        let mut overlay = child.content.tracks[1].clone();
        let CompositionTrack::Video { id, clips, .. } = &mut overlay else {
            panic!()
        };
        *id = super::super::composition::TrackId::parse("overlay").unwrap();
        clips[0].id = super::super::composition::CompositionClipId::parse("overlay-clip").unwrap();
        child.content.tracks.insert(0, overlay);
        let CompositionTrack::Audio { solo, .. } = &mut child.content.tracks[1] else {
            panic!()
        };
        *solo = true;
        let plan = plan_audio_tracks(&project, "root", 600000..1000000).unwrap();
        assert_eq!(plan.len(), 2);
        assert!(!plan.iter().any(|l| l.clip.id.as_str() == "overlay-clip"));
        assert!(plan.iter().any(|l| l.clip.id.as_str() == "video-clip"));
    }
    #[test]
    fn context_plan_retains_preroll_and_lookahead_inside_ancestor_windows() {
        let project = fixture();
        let selected = plan_audio_tracks(&project, "root", 600000..1000000).unwrap();
        let context = plan_audio_tracks_with_context(&project, "root", 600000..1000000).unwrap();
        assert_eq!(context.len(), selected.len());
        assert!(context[0].root_range_us.start < 600000);
        assert!(context[0].root_range_us.end > 1000000);
        assert_eq!(context[0].sample(700000).unwrap(), selected[0].sample(700000).unwrap());
        assert!(context[0].sample(599999).unwrap().is_some());
        assert!(context[0].sample(1000000).unwrap().is_some());
        assert!(context[0].sample(context[0].root_range_us.start - 1).unwrap().is_none());
        assert!(context[0].sample(context[0].root_range_us.end).unwrap().is_none());
        assert!(plan_audio_tracks_with_context(&project, "root", 1..1).is_err());
    }
    #[test]
    fn group_window_preserves_clip_envelope_source_clock_and_ignores_visual_opacity() {
        let project = fixture();
        let plan = plan_audio_tracks(&project, "root", 600000..1000000).unwrap();
        assert_eq!(plan.len(), 1);
        let layer = &plan[0];
        assert_eq!(layer.instance_path, ["group"]);
        assert_eq!(layer.root_range_us, 600000..1000000);
        assert_eq!(layer.clip_local_start_us, 300000);
        let sample = layer.sample(700000).unwrap().unwrap();
        assert_eq!(sample.source_time_us, 500000);
        assert_eq!(sample.clip_local_us, 400000);
        assert!((sample.gain - 0.64).abs() < 1e-12);
        assert_eq!(sample.pan, -0.5);
        assert!(layer.sample(599999).unwrap().is_none());
        assert!(layer.sample(1000000).unwrap().is_none());
        let focused = plan_audio_tracks(&project, "child", 400000..800000).unwrap();
        assert_eq!(
            focused[0].sample(500000).unwrap(),
            layer.sample(700000).unwrap()
        );
    }
    #[test]
    fn splitting_a_group_preserves_audio_samples_and_envelopes_at_the_boundary() {
        let project = fixture();
        let split = project
            .split_instance("root", "group", "right", 800000)
            .unwrap();
        let before = plan_audio_tracks(&project, "root", 500000..1300000).unwrap();
        let after = plan_audio_tracks(&split, "root", 500000..1300000).unwrap();
        assert_eq!(after.len(), 2);
        for time in [500000, 799999, 800000, 800001, 1299999] {
            let samples = |plan: &[NestedAudioLayer]| {
                plan.iter()
                    .filter_map(|layer| layer.sample(time).unwrap())
                    .collect::<Vec<_>>()
            };
            assert_eq!(samples(&before), samples(&after));
        }
    }
    #[test]
    fn duplicate_instances_keep_paths_and_reverse_reads_with_track_gating() {
        let mut project = fixture();
        let mut copy = project.compositions["root"].instances[0].clone();
        copy.id = "copy".into();
        project
            .compositions
            .get_mut("root")
            .unwrap()
            .instances
            .push(copy);
        let child = project.compositions.get_mut("child").unwrap();
        let CompositionTrack::Audio { clips, .. } = &mut child.content.tracks[0] else {
            panic!()
        };
        clips[0].reversed = true;
        let plan = plan_audio_tracks(&project, "root", 600000..1000000).unwrap();
        assert_eq!(plan.len(), 2);
        assert_ne!(plan[0].instance_path, plan[1].instance_path);
        assert_eq!(
            plan[0].sample(700000).unwrap().unwrap().source_time_us,
            699999
        );
        let CompositionTrack::Audio { muted, .. } = &mut project
            .compositions
            .get_mut("child")
            .unwrap()
            .content
            .tracks[0]
        else {
            panic!()
        };
        *muted = true;
        assert!(plan_audio_tracks(&project, "root", 600000..1000000)
            .unwrap()
            .is_empty());
        assert!(plan_audio_tracks(&project, "missing", 0..1).is_err());
        assert!(plan_audio_tracks(&project, "root", 1..1).is_err());
        assert!(plan_audio_tracks(&project, "root", 0..1500001).is_err());
    }
    #[test]
    fn solo_track_suppresses_unsoloed_tracks_and_clipped_document_tail() {
        let mut project = fixture();
        let child = project.compositions.get_mut("child").unwrap();
        let mut solo = child.content.tracks[0].clone();
        let CompositionTrack::Audio {
            id,
            solo: is_solo,
            clips,
            ..
        } = &mut solo
        else {
            panic!()
        };
        *id = super::super::composition::TrackId::parse("solo").unwrap();
        *is_solo = true;
        clips[0].id = super::super::composition::CompositionClipId::parse("solo-clip").unwrap();
        child.content.tracks.push(solo);
        child.duration_us = Some(700000);
        let plan = plan_audio_tracks(&project, "root", 600000..1200000).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].clip.id.as_str(), "solo-clip");
        assert_eq!(plan[0].root_range_us, 600000..900000);
    }
}
