//! Immutable planning for multi-source composition exports.
//!
//! Wire documents may name library assets, but paths and probe metadata never
//! cross the HTTP boundary. The handler resolves every id, probes and hashes
//! the corresponding regular file, then supplies the trusted records below.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, ensure, Result};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::domain::arithmetic::frame_count_ceil;
use crate::domain::artifact_graph::Fingerprint;
use crate::domain::composition::{
    AnimatableValue, BlendMode, ClipPlacement, Composition, CompositionSource, CompositionTrack,
    FrameInterpolation, MaskShape, PlaybackMode, SourceId, SourceKind, SpeedRampAudioPolicy,
    TransformSpec, VideoEffect, COMPOSITION_SCHEMA_VERSION, DEFAULT_TIME_BASE,
    MAX_ACTIVE_SPEED_RAMP_SEGMENTS, MAX_OPTICAL_FLOW_CLIP_TICKS, MAX_OPTICAL_FLOW_EDGE,
    MAX_OPTICAL_FLOW_PIXELS, MAX_OPTICAL_FLOW_PIXEL_FRAMES, MAX_REVERSE_CLIP_TICKS,
    MAX_REVERSE_EDGE, MAX_REVERSE_PIXELS, MAX_REVERSE_PIXEL_FRAMES, MAX_STABILIZATION_CLIP_TICKS,
    MAX_STABILIZATION_EDGE, MAX_STABILIZATION_PIXELS, MAX_STABILIZATION_PIXEL_FRAMES,
};
pub use crate::ports::{
    CompositionAv1Encoder, CompositionExportProfile, CompositionExportSpec, CompositionMp4Codec,
    CompositionProResProfile, CompositionWebmCodec,
};

pub const COMPOSITION_RENDER_SCHEMA_VERSION: u32 = 1;
const MAX_VISUAL_FILTER_DIMENSION: f64 = 8_192.0;
const MAX_VISUAL_POSITION: f64 = 32_768.0;
const MAX_ACTIVE_KEYFRAME_POINTS: usize = 2_048;
const MAX_ACTIVE_AUDIO_KEYFRAME_POINTS: usize = 2_048;
const MAX_AUDIO_GAIN: f64 = 16.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionRenderRequest {
    pub schema_version: u32,
    pub composition: Composition,
    #[serde(default)]
    pub output: CompositionOutputRequest,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionOutputFormat {
    #[default]
    Mp4,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionVideoCodec {
    #[default]
    H264,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionQualityTier {
    High,
    #[default]
    Medium,
    Compact,
}

impl CompositionQualityTier {
    pub fn video_quality(self, profile: CompositionExportProfile) -> u32 {
        match profile {
            CompositionExportProfile::Mp4 {
                codec: CompositionMp4Codec::H264,
            } => match self {
                Self::High => 18,
                Self::Medium => 23,
                Self::Compact => 30,
            },
            CompositionExportProfile::Mp4 {
                codec: CompositionMp4Codec::H265,
            } => match self {
                Self::High => 20,
                Self::Medium => 25,
                Self::Compact => 32,
            },
            CompositionExportProfile::Webm { .. } => match self {
                Self::High => 24,
                Self::Medium => 32,
                Self::Compact => 40,
            },
            CompositionExportProfile::Mov { .. } => match self {
                Self::High => 5,
                Self::Medium => 9,
                Self::Compact => 13,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionOutputRequest {
    pub profile: CompositionExportProfile,
    #[serde(default)]
    pub quality_tier: CompositionQualityTier,
}

impl Default for CompositionOutputRequest {
    fn default() -> Self {
        Self {
            profile: CompositionExportProfile::default(),
            quality_tier: CompositionQualityTier::Medium,
        }
    }
}

impl CompositionOutputRequest {
    pub fn export_spec(self, av1_encoder: Option<CompositionAv1Encoder>) -> CompositionExportSpec {
        CompositionExportSpec {
            profile: self.profile,
            video_quality: self.quality_tier.video_quality(self.profile),
            av1_encoder,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompositionOutputRequestWire {
    #[serde(default)]
    profile: Option<CompositionExportProfile>,
    #[serde(default)]
    quality_tier: CompositionQualityTier,
    #[serde(default)]
    format: Option<CompositionOutputFormat>,
    #[serde(default)]
    codec: Option<CompositionVideoCodec>,
}

impl<'de> Deserialize<'de> for CompositionOutputRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CompositionOutputRequestWire::deserialize(deserializer)?;
        let profile = match (wire.profile, wire.format, wire.codec) {
            (Some(profile), None, None) => profile,
            (Some(_), _, _) => {
                return Err(D::Error::custom(
                    "composition output cannot mix profile with legacy format/codec",
                ));
            }
            (None, _, _) => CompositionExportProfile::default(),
        };
        Ok(Self {
            profile,
            quality_tier: wire.quality_tier,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRequirement {
    Audio,
    Video,
    Image,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrustedCompositionSource {
    pub source: CompositionSource,
    pub fingerprint: Fingerprint,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionPlan {
    schema_version: u32,
    composition: Composition,
    source_fingerprints: BTreeMap<SourceId, Fingerprint>,
    output: CompositionOutputRequest,
    plan_fingerprint: Fingerprint,
}

impl CompositionPlan {
    pub fn compile(
        request: CompositionRenderRequest,
        trusted: BTreeMap<SourceId, TrustedCompositionSource>,
    ) -> Result<Self> {
        ensure!(
            request.schema_version == COMPOSITION_RENDER_SCHEMA_VERSION,
            "неподдерживаемая версия composition render"
        );
        ensure!(
            request.composition.schema_version == COMPOSITION_SCHEMA_VERSION,
            "неподдерживаемая версия composition document"
        );

        request.composition.validate()?;
        let required = source_requirements(&request.composition)?;
        let actual: BTreeSet<_> = trusted.keys().cloned().collect();
        let expected: BTreeSet<_> = required.keys().cloned().collect();
        ensure!(actual == expected, "не все источники композиции разрешены");

        let output = request.output;
        let mut composition = request.composition;
        // Inactive clips still belong to the saved document and therefore keep
        // their already domain-validated metadata. Only active render inputs
        // are resolved, fingerprinted, and replaced with trusted probe data.
        for (id, source) in &trusted {
            composition
                .sources
                .insert(id.clone(), source.source.clone());
        }
        for (id, requirement) in required {
            let source = composition
                .sources
                .get(&id)
                .expect("trusted source key set was checked");
            match requirement {
                SourceRequirement::Video => ensure!(
                    source.kind == SourceKind::Video,
                    "видеоклип ссылается не на видео"
                ),
                SourceRequirement::Image => ensure!(
                    source.kind == SourceKind::Image,
                    "слой изображения ссылается не на изображение"
                ),
                SourceRequirement::Audio => ensure!(
                    source.has_audio,
                    "аудиоклип ссылается на источник без звука"
                ),
            }
        }
        composition.validate()?;
        validate_renderable_v1(&composition)?;

        let source_fingerprints: BTreeMap<_, _> = trusted
            .into_iter()
            .map(|(id, source)| (id, source.fingerprint))
            .collect();
        let canonical_composition = serde_json::to_vec(&composition)?;
        let canonical_sources = serde_json::to_vec(&source_fingerprints)?;
        let canonical_output = serde_json::to_vec(&output)?;
        let schema_bytes = COMPOSITION_RENDER_SCHEMA_VERSION.to_be_bytes();
        let plan_fingerprint = Fingerprint::combine([
            b"composition-render-plan".as_slice(),
            schema_bytes.as_slice(),
            canonical_composition.as_slice(),
            canonical_sources.as_slice(),
            canonical_output.as_slice(),
        ]);
        Ok(Self {
            schema_version: COMPOSITION_RENDER_SCHEMA_VERSION,
            composition,
            source_fingerprints,
            output,
            plan_fingerprint,
        })
    }

    pub fn composition(&self) -> &Composition {
        &self.composition
    }

    pub fn output(&self) -> CompositionOutputRequest {
        self.output
    }

    pub fn fingerprint(&self) -> &Fingerprint {
        &self.plan_fingerprint
    }
}

pub fn source_requirements(
    composition: &Composition,
) -> Result<BTreeMap<SourceId, SourceRequirement>> {
    let mut requirements = BTreeMap::new();
    let has_solo = composition.tracks.iter().any(|track| {
        matches!(
            track,
            CompositionTrack::Audio {
                muted: false,
                solo: true,
                ..
            }
        )
    });
    for track in &composition.tracks {
        match track {
            CompositionTrack::Video {
                hidden: false,
                clips,
                ..
            } => {
                for clip in clips.iter().filter(|clip| clip.enabled) {
                    merge_requirement(
                        &mut requirements,
                        clip.source_id.clone(),
                        SourceRequirement::Video,
                    )?;
                }
            }
            CompositionTrack::Audio {
                muted, solo, clips, ..
            } if !*muted && (!has_solo || *solo) => {
                for clip in clips.iter().filter(|clip| {
                    clip.enabled
                        && clip
                            .placement
                            .speed_ramp
                            .as_ref()
                            .is_none_or(|ramp| ramp.audio_policy != SpeedRampAudioPolicy::Mute)
                }) {
                    merge_requirement(
                        &mut requirements,
                        clip.source_id.clone(),
                        SourceRequirement::Audio,
                    )?;
                }
            }
            CompositionTrack::Image {
                hidden: false,
                clips,
                ..
            } => {
                for clip in clips.iter().filter(|clip| clip.enabled) {
                    merge_requirement(
                        &mut requirements,
                        clip.source_id.clone(),
                        SourceRequirement::Image,
                    )?;
                }
            }
            CompositionTrack::Video { .. }
            | CompositionTrack::Audio { .. }
            | CompositionTrack::Image { .. }
            | CompositionTrack::Text { .. } => {}
        }
    }
    Ok(requirements)
}

fn merge_requirement(
    requirements: &mut BTreeMap<SourceId, SourceRequirement>,
    id: SourceId,
    next: SourceRequirement,
) -> Result<()> {
    use SourceRequirement::{Audio, Image, Video};
    let merged = match (requirements.get(&id).copied(), next) {
        (None, value) => value,
        (Some(Audio), Video) | (Some(Video), Audio | Video) => Video,
        (Some(Audio), Audio) => Audio,
        (Some(Image), Image) => Image,
        (Some(Image), Audio | Video) | (Some(Audio | Video), Image) => {
            bail!("один sourceId нельзя использовать как изображение и media stream")
        }
    };
    requirements.insert(id, merged);
    Ok(())
}

/// Composition v1 supports a gapless neutral bottom video track and animated
/// visual layers above it. Array index zero is the topmost visual track. Text,
/// exact handle-backed primary transitions, and clip-local Rectangle/Ellipse
/// masks are supported. Bounded optical flow, reverse/freeze playback, and
/// classical deshake stabilization are request-specific; Linear masks and
/// stabilization of a freeze frame remain explicitly fail-closed.
fn validate_renderable_v1(composition: &Composition) -> Result<()> {
    let mut keyframe_points = 0_usize;
    let mut audio_keyframe_points = 0_usize;
    let mut optical_flow_pixel_frames = 0_u128;
    let mut reverse_pixel_frames = 0_u128;
    let mut stabilization_pixel_frames = 0_u128;
    let mut speed_ramp_segments = 0_usize;
    let active_visual: Vec<_> = composition
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| match track {
            CompositionTrack::Video {
                hidden: false,
                clips,
                ..
            } => clips.iter().any(|clip| clip.enabled),
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
            _ => false,
        })
        .collect();
    let (primary_index, primary_track) = active_visual
        .last()
        .copied()
        .ok_or_else(|| anyhow::anyhow!("composition render v1 требует основную видеодорожку"))?;
    let CompositionTrack::Video {
        clips,
        muted: primary_muted,
        transitions,
        ..
    } = primary_track
    else {
        bail!("нижняя видимая visual-дорожка должна быть основной видеодорожкой")
    };
    let enabled_clips: Vec<_> = clips.iter().filter(|clip| clip.enabled).collect();
    ensure!(
        !enabled_clips.is_empty(),
        "основная видеодорожка должна содержать enabled clips"
    );

    let active_primary_ids: BTreeSet<_> = enabled_clips.iter().map(|clip| &clip.id).collect();
    let mut handle_ticks = BTreeMap::<&crate::domain::composition::CompositionClipId, u64>::new();
    for transition in transitions {
        if active_primary_ids.contains(&transition.from_clip_id)
            && active_primary_ids.contains(&transition.to_clip_id)
        {
            let before_edit = transition.duration_ticks / 2;
            let after_edit = transition.duration_ticks - before_edit;
            *handle_ticks.entry(&transition.from_clip_id).or_default() += after_edit;
            *handle_ticks.entry(&transition.to_clip_id).or_default() += before_edit;
        }
    }

    let mut primary = enabled_clips;
    primary.sort_by_key(|clip| (clip.placement.timeline_start_tick, clip.id.as_str()));
    let mut cursor = 0_u64;
    for clip in primary {
        count_speed_ramp_segments(&clip.placement, &mut speed_ramp_segments)?;
        ensure!(
            clip.placement.timeline_start_tick == cursor,
            "видеодорожка должна начинаться с нуля и не содержать gaps/overlaps"
        );
        ensure!(
            clip.transform == TransformSpec::default()
                && clip.opacity == AnimatableValue::constant(1.0)
                && clip.blend_mode == BlendMode::Normal
                && clip.effects.is_empty(),
            "слои, анимация и clip effects ещё не поддерживаются render v1"
        );
        if clip.frame_interpolation == FrameInterpolation::OpticalFlow {
            let render_duration_ticks = clip
                .placement
                .timeline_duration_ticks()?
                .checked_add(handle_ticks.get(&clip.id).copied().unwrap_or(0))
                .ok_or_else(|| anyhow::anyhow!("optical-flow duration overflow"))?;
            validate_optical_flow_work(
                composition.canvas.width,
                composition.canvas.height,
                render_duration_ticks,
                composition,
                clip.id.as_str(),
                &mut optical_flow_pixel_frames,
            )?;
        }
        if clip.playback_mode == PlaybackMode::Reverse {
            let source = &composition.sources[&clip.source_id];
            validate_reverse_work(
                source.width,
                source.height,
                clip.placement.source_out_tick - clip.placement.source_in_tick,
                composition,
                clip.id.as_str(),
                &mut reverse_pixel_frames,
            )?;
        }
        if clip.stabilization.is_enabled() {
            let timeline_duration_ticks = clip
                .placement
                .timeline_duration_ticks()?
                .checked_add(handle_ticks.get(&clip.id).copied().unwrap_or(0))
                .ok_or_else(|| anyhow::anyhow!("stabilization duration overflow"))?;
            let source_duration_ticks = if clip.placement.speed_ramp.is_some() {
                clip.placement.source_span_ticks()
            } else {
                scaled_source_duration_ticks(
                    timeline_duration_ticks,
                    clip.placement.speed,
                    "stabilization",
                )?
            };
            let source = &composition.sources[&clip.source_id];
            validate_stabilization_work(
                source.width,
                source.height,
                source_duration_ticks,
                composition,
                clip.id.as_str(),
                &mut stabilization_pixel_frames,
            )?;
        }
        let source = &composition.sources[&clip.source_id];
        if !*primary_muted
            && clip.source_audio_enabled
            && source.has_audio
            && !matches!(clip.playback_mode, PlaybackMode::Freeze { .. })
            && clip
                .placement
                .speed_ramp
                .as_ref()
                .is_none_or(|ramp| ramp.audio_policy != SpeedRampAudioPolicy::Mute)
        {
            validate_animatable(
                &clip.audio_gain,
                "source audio gain",
                clip.placement.timeline_duration_ticks()?,
                composition.time_base,
                0.0,
                MAX_AUDIO_GAIN,
                true,
                &mut audio_keyframe_points,
            )?;
            validate_animatable(
                &clip.audio_pan,
                "source audio pan",
                clip.placement.timeline_duration_ticks()?,
                composition.time_base,
                -1.0,
                1.0,
                true,
                &mut audio_keyframe_points,
            )?;
        }
        cursor = clip.placement.timeline_end_tick()?;
    }

    for track in &composition.tracks[..primary_index] {
        match track {
            CompositionTrack::Video {
                hidden: false,
                clips,
                transitions,
                ..
            } => {
                ensure!(
                    transitions.is_empty(),
                    "переходы на overlay video-дорожках ещё не поддерживаются"
                );
                let mut intervals = Vec::new();
                for clip in clips.iter().filter(|clip| clip.enabled) {
                    count_speed_ramp_segments(&clip.placement, &mut speed_ramp_segments)?;
                    let source = &composition.sources[&clip.source_id];
                    if clip.frame_interpolation == FrameInterpolation::OpticalFlow {
                        validate_optical_flow_work(
                            source.width,
                            source.height,
                            clip.placement.timeline_duration_ticks()?,
                            composition,
                            clip.id.as_str(),
                            &mut optical_flow_pixel_frames,
                        )?;
                    }
                    if clip.playback_mode == PlaybackMode::Reverse {
                        validate_reverse_work(
                            source.width,
                            source.height,
                            clip.placement.source_out_tick - clip.placement.source_in_tick,
                            composition,
                            clip.id.as_str(),
                            &mut reverse_pixel_frames,
                        )?;
                    }
                    if clip.stabilization.is_enabled() {
                        validate_stabilization_work(
                            source.width,
                            source.height,
                            clip.placement.source_out_tick - clip.placement.source_in_tick,
                            composition,
                            clip.id.as_str(),
                            &mut stabilization_pixel_frames,
                        )?;
                    }
                    validate_visual(
                        &clip.transform,
                        &clip.opacity,
                        &clip.effects,
                        clip.id.as_str(),
                        source.width,
                        source.height,
                        clip.placement.timeline_duration_ticks()?,
                        composition.time_base,
                        true,
                        &mut keyframe_points,
                    )?;
                    intervals.push((
                        clip.placement.timeline_start_tick,
                        clip.placement.timeline_end_tick()?,
                        clip.id.as_str(),
                    ));
                }
                validate_visual_intervals(&mut intervals, cursor)?;
            }
            CompositionTrack::Image {
                hidden: false,
                clips,
                ..
            } => {
                let mut intervals = Vec::new();
                for clip in clips.iter().filter(|clip| clip.enabled) {
                    validate_visual(
                        &clip.transform,
                        &clip.opacity,
                        &[],
                        clip.id.as_str(),
                        composition.sources[&clip.source_id].width,
                        composition.sources[&clip.source_id].height,
                        clip.duration_ticks,
                        composition.time_base,
                        true,
                        &mut keyframe_points,
                    )?;
                    let end = clip
                        .timeline_start_tick
                        .checked_add(clip.duration_ticks)
                        .ok_or_else(|| anyhow::anyhow!("image clip timeline overflow"))?;
                    intervals.push((clip.timeline_start_tick, end, clip.id.as_str()));
                }
                validate_visual_intervals(&mut intervals, cursor)?;
            }
            CompositionTrack::Text {
                hidden: false,
                clips,
                ..
            } => {
                let mut intervals = Vec::new();
                for clip in clips.iter().filter(|clip| clip.enabled) {
                    validate_visual(
                        &clip.transform,
                        &clip.opacity,
                        &[],
                        clip.id.as_str(),
                        composition.canvas.width,
                        composition.canvas.height,
                        clip.timeline_end_tick - clip.timeline_start_tick,
                        composition.time_base,
                        false,
                        &mut keyframe_points,
                    )?;
                    intervals.push((
                        clip.timeline_start_tick,
                        clip.timeline_end_tick,
                        clip.id.as_str(),
                    ));
                }
                validate_visual_intervals(&mut intervals, cursor)?;
            }
            _ => {}
        }
    }

    let has_solo = composition.tracks.iter().any(|track| {
        matches!(
            track,
            CompositionTrack::Audio {
                muted: false,
                solo: true,
                ..
            }
        )
    });
    for track in &composition.tracks {
        if let CompositionTrack::Audio {
            muted, solo, clips, ..
        } = track
        {
            if *muted || (has_solo && !*solo) {
                continue;
            }
            let mut end = 0_u64;
            for clip in clips.iter().filter(|clip| clip.enabled) {
                ensure!(
                    clip.placement.timeline_start_tick >= end,
                    "аудиоклипы внутри дорожки не должны перекрываться"
                );
                ensure!(
                    clip.placement.timeline_end_tick()? <= cursor,
                    "аудиоклип выходит за длительность основной видеодорожки"
                );
                let duration = clip.placement.timeline_duration_ticks()?;
                let ramp_audio_muted = clip
                    .placement
                    .speed_ramp
                    .as_ref()
                    .is_some_and(|ramp| ramp.audio_policy == SpeedRampAudioPolicy::Mute);
                if !ramp_audio_muted {
                    count_speed_ramp_segments(&clip.placement, &mut speed_ramp_segments)?;
                    validate_animatable(
                        &clip.gain,
                        "audio gain",
                        duration,
                        composition.time_base,
                        0.0,
                        MAX_AUDIO_GAIN,
                        true,
                        &mut audio_keyframe_points,
                    )?;
                    validate_animatable(
                        &clip.pan,
                        "audio pan",
                        duration,
                        composition.time_base,
                        -1.0,
                        1.0,
                        true,
                        &mut audio_keyframe_points,
                    )?;
                }
                end = clip.placement.timeline_end_tick()?;
            }
        }
    }
    ensure!(
        keyframe_points <= MAX_ACTIVE_KEYFRAME_POINTS,
        "composition содержит слишком много active visual keyframes"
    );
    ensure!(
        audio_keyframe_points <= MAX_ACTIVE_AUDIO_KEYFRAME_POINTS,
        "composition содержит слишком много active audio keyframes"
    );
    Ok(())
}

fn count_speed_ramp_segments(placement: &ClipPlacement, active_segments: &mut usize) -> Result<()> {
    let Some(segments) = placement.speed_ramp_segments()? else {
        return Ok(());
    };
    *active_segments = active_segments
        .checked_add(segments.len())
        .ok_or_else(|| anyhow::anyhow!("speed-ramp segment count overflow"))?;
    ensure!(
        *active_segments <= MAX_ACTIVE_SPEED_RAMP_SEGMENTS,
        "composition содержит слишком много active speed-ramp segments"
    );
    Ok(())
}

fn validate_optical_flow_work(
    width: u32,
    height: u32,
    duration_ticks: u64,
    composition: &Composition,
    clip_id: &str,
    total_pixel_frames: &mut u128,
) -> Result<()> {
    let pixels = u128::from(width) * u128::from(height);
    ensure!(
        width <= MAX_OPTICAL_FLOW_EDGE
            && height <= MAX_OPTICAL_FLOW_EDGE
            && pixels <= MAX_OPTICAL_FLOW_PIXELS,
        "optical flow clip {clip_id} превышает dimension budget"
    );
    ensure!(
        duration_ticks <= MAX_OPTICAL_FLOW_CLIP_TICKS,
        "optical flow clip {clip_id} превышает duration budget"
    );
    let output_frames = u128::from(
        frame_count_ceil(
            duration_ticks,
            composition.time_base,
            composition.canvas.fps_milli,
        )
        .ok_or_else(|| anyhow::anyhow!("optical-flow frame count overflow"))?,
    );
    let work = pixels
        .checked_mul(output_frames)
        .ok_or_else(|| anyhow::anyhow!("optical-flow work overflow"))?;
    *total_pixel_frames = total_pixel_frames
        .checked_add(work)
        .ok_or_else(|| anyhow::anyhow!("optical-flow work overflow"))?;
    ensure!(
        *total_pixel_frames <= MAX_OPTICAL_FLOW_PIXEL_FRAMES,
        "composition превышает optical-flow work budget"
    );
    Ok(())
}

fn validate_reverse_work(
    width: u32,
    height: u32,
    source_duration_ticks: u64,
    composition: &Composition,
    clip_id: &str,
    total_pixel_frames: &mut u128,
) -> Result<()> {
    let pixels = u128::from(width) * u128::from(height);
    ensure!(
        width <= MAX_REVERSE_EDGE && height <= MAX_REVERSE_EDGE && pixels <= MAX_REVERSE_PIXELS,
        "reverse clip {clip_id} превышает dimension budget"
    );
    ensure!(
        u128::from(source_duration_ticks) * u128::from(DEFAULT_TIME_BASE)
            <= u128::from(MAX_REVERSE_CLIP_TICKS) * u128::from(composition.time_base),
        "reverse clip {clip_id} превышает buffered duration budget"
    );
    let buffered_frames = u128::from(
        frame_count_ceil(
            source_duration_ticks,
            composition.time_base,
            composition.canvas.fps_milli,
        )
        .ok_or_else(|| anyhow::anyhow!("reverse frame count overflow"))?,
    );
    let work = pixels
        .checked_mul(buffered_frames)
        .ok_or_else(|| anyhow::anyhow!("reverse work overflow"))?;
    *total_pixel_frames = total_pixel_frames
        .checked_add(work)
        .ok_or_else(|| anyhow::anyhow!("reverse work overflow"))?;
    ensure!(
        *total_pixel_frames <= MAX_REVERSE_PIXEL_FRAMES,
        "composition превышает reverse buffering work budget"
    );
    Ok(())
}

fn scaled_source_duration_ticks(
    timeline_duration_ticks: u64,
    speed: f64,
    label: &str,
) -> Result<u64> {
    let duration = timeline_duration_ticks as f64 * speed;
    ensure!(
        duration.is_finite() && duration >= 1.0 && duration <= u64::MAX as f64,
        "{label} source duration недопустима"
    );
    Ok(duration.ceil() as u64)
}

fn validate_stabilization_work(
    width: u32,
    height: u32,
    source_duration_ticks: u64,
    composition: &Composition,
    clip_id: &str,
    total_pixel_frames: &mut u128,
) -> Result<()> {
    let pixels = u128::from(width) * u128::from(height);
    ensure!(
        width <= MAX_STABILIZATION_EDGE
            && height <= MAX_STABILIZATION_EDGE
            && pixels <= MAX_STABILIZATION_PIXELS,
        "stabilization clip {clip_id} превышает dimension budget"
    );
    ensure!(
        u128::from(source_duration_ticks) * u128::from(DEFAULT_TIME_BASE)
            <= u128::from(MAX_STABILIZATION_CLIP_TICKS) * u128::from(composition.time_base),
        "stabilization clip {clip_id} превышает duration budget"
    );
    let frames = u128::from(
        frame_count_ceil(
            source_duration_ticks,
            composition.time_base,
            composition.canvas.fps_milli,
        )
        .ok_or_else(|| anyhow::anyhow!("stabilization frame count overflow"))?,
    );
    let work = pixels
        .checked_mul(frames)
        .ok_or_else(|| anyhow::anyhow!("stabilization work overflow"))?;
    *total_pixel_frames = total_pixel_frames
        .checked_add(work)
        .ok_or_else(|| anyhow::anyhow!("stabilization work overflow"))?;
    ensure!(
        *total_pixel_frames <= MAX_STABILIZATION_PIXEL_FRAMES,
        "composition превышает stabilization work budget"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_visual(
    transform: &TransformSpec,
    opacity: &AnimatableValue,
    effects: &[VideoEffect],
    clip_id: &str,
    source_width: u32,
    source_height: u32,
    clip_duration_ticks: u64,
    composition_time_base: u32,
    media_anchor: bool,
    keyframe_points: &mut usize,
) -> Result<()> {
    validate_animatable(
        &transform.x,
        "transform x",
        clip_duration_ticks,
        composition_time_base,
        -MAX_VISUAL_POSITION,
        MAX_VISUAL_POSITION,
        true,
        keyframe_points,
    )?;
    validate_animatable(
        &transform.y,
        "transform y",
        clip_duration_ticks,
        composition_time_base,
        -MAX_VISUAL_POSITION,
        MAX_VISUAL_POSITION,
        true,
        keyframe_points,
    )?;
    let (_, scale_x) = validate_animatable(
        &transform.scale_x,
        "transform scaleX",
        clip_duration_ticks,
        composition_time_base,
        0.01,
        16.0,
        true,
        keyframe_points,
    )?;
    let (_, scale_y) = validate_animatable(
        &transform.scale_y,
        "transform scaleY",
        clip_duration_ticks,
        composition_time_base,
        0.01,
        16.0,
        true,
        keyframe_points,
    )?;
    let (rotation_min, rotation_max) = validate_animatable(
        &transform.rotation_degrees,
        "transform rotation",
        clip_duration_ticks,
        composition_time_base,
        -3_600.0,
        3_600.0,
        true,
        keyframe_points,
    )?;
    let width = source_width as f64 * scale_x;
    let height = source_height as f64 * scale_y;
    let rotates = rotation_min.abs() > f64::EPSILON || rotation_max.abs() > f64::EPSILON;
    let (rotated_width, rotated_height) = if rotates {
        let anchor_x = if media_anchor {
            transform.anchor_x
        } else {
            0.5
        };
        let anchor_y = if media_anchor {
            transform.anchor_y
        } else {
            0.5
        };
        let pad_width = (2.0 * (anchor_x * width).max((1.0 - anchor_x) * width))
            .max(width)
            .ceil();
        let pad_height = (2.0 * (anchor_y * height).max((1.0 - anchor_y) * height))
            .max(height)
            .ceil();
        let dimension = pad_width.hypot(pad_height).ceil();
        (dimension, dimension)
    } else {
        (width, height)
    };
    ensure!(
        width >= 1.0
            && height >= 1.0
            && rotated_width <= MAX_VISUAL_FILTER_DIMENSION
            && rotated_height <= MAX_VISUAL_FILTER_DIMENSION,
        "clip {clip_id} содержит небезопасные animated dimensions"
    );
    validate_animatable(
        opacity,
        "opacity",
        clip_duration_ticks,
        composition_time_base,
        0.0,
        1.0,
        true,
        keyframe_points,
    )?;
    let mut saw_mask = false;
    for effect in effects {
        match effect {
            VideoEffect::ChromaKey { .. } if saw_mask => {
                bail!("clip {clip_id} должен размещать chroma key effects перед shape masks")
            }
            VideoEffect::ChromaKey { similarity, .. } => ensure!(
                *similarity >= 0.000_01,
                "clip {clip_id} содержит chroma similarity ниже минимума FFmpeg"
            ),
            VideoEffect::Mask {
                shape: MaskShape::Linear,
                ..
            } => bail!("Linear mask semantics ещё не поддерживаются render v1"),
            VideoEffect::Mask {
                x,
                y,
                width,
                height,
                ..
            } => {
                saw_mask = true;
                for (value, label, minimum, maximum, inclusive_minimum) in [
                    (x, "mask x", 0.0, 1.0, true),
                    (y, "mask y", 0.0, 1.0, true),
                    (width, "mask width", 0.0, 2.0, false),
                    (height, "mask height", 0.0, 2.0, false),
                ] {
                    validate_animatable(
                        value,
                        label,
                        clip_duration_ticks,
                        composition_time_base,
                        minimum,
                        maximum,
                        inclusive_minimum,
                        keyframe_points,
                    )?;
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_animatable(
    value: &AnimatableValue,
    label: &str,
    clip_duration_ticks: u64,
    composition_time_base: u32,
    minimum: f64,
    maximum: f64,
    inclusive_minimum: bool,
    keyframe_points: &mut usize,
) -> Result<(f64, f64)> {
    let valid = |candidate: f64| {
        candidate.is_finite()
            && candidate <= maximum
            && if inclusive_minimum {
                candidate >= minimum
            } else {
                candidate > minimum
            }
    };
    match value {
        AnimatableValue::Constant { value } => {
            ensure!(valid(*value), "{label} вне поддерживаемого диапазона");
            Ok((*value, *value))
        }
        AnimatableValue::Keyframes { track } => {
            *keyframe_points = keyframe_points
                .checked_add(track.keyframes.len())
                .ok_or_else(|| anyhow::anyhow!("keyframe count overflow"))?;
            let last = track.keyframes.last().expect("domain validated keyframes");
            ensure!(
                u128::from(last.tick) * u128::from(composition_time_base)
                    <= u128::from(clip_duration_ticks) * u128::from(track.time_base),
                "{label} keyframes выходят за clip-local duration"
            );
            ensure!(
                track.keyframes.iter().all(|keyframe| valid(keyframe.value)),
                "{label} keyframes вне поддерживаемого диапазона"
            );
            let (minimum, maximum) = track.keyframes.iter().fold(
                (f64::INFINITY, f64::NEG_INFINITY),
                |(minimum, maximum), keyframe| {
                    (minimum.min(keyframe.value), maximum.max(keyframe.value))
                },
            );
            Ok((minimum, maximum))
        }
    }
}

fn validate_visual_intervals(
    intervals: &mut Vec<(u64, u64, &str)>,
    primary_duration: u64,
) -> Result<()> {
    intervals.sort_by_key(|(start, end, id)| (*start, *end, *id));
    let mut previous_end = 0;
    for (start, end, id) in intervals {
        ensure!(
            *start >= previous_end,
            "visual clips внутри дорожки не должны перекрываться перед {id}"
        );
        ensure!(
            *end <= primary_duration,
            "visual clip {id} выходит за основную видеодорожку"
        );
        previous_end = *end;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::composition::{
        AudioClip, CanvasSpec, ClipPlacement, ClipTransition, CompositionClipId, ImageClip, Rgba,
        SpeedRampInterpolation, SpeedRampPoint, SpeedRampSpec, StabilizationSpec, TextClip,
        TextStyle, TrackId, TransitionId, TransitionKind, VideoClip, VideoEffect,
    };
    use crate::domain::keyframes::{Interpolation, Keyframe, KeyframeTrack};

    fn request(second_start: u64) -> CompositionRenderRequest {
        let source_a = SourceId::parse("source-a").unwrap();
        let source_b = SourceId::parse("source-b").unwrap();
        let mut composition = Composition::new(CanvasSpec::default());
        for id in [&source_a, &source_b] {
            composition.sources.insert(
                id.clone(),
                CompositionSource {
                    id: id.clone(),
                    kind: SourceKind::Video,
                    duration_ticks: 4_000_000,
                    width: 1_920,
                    height: 1_080,
                    has_audio: true,
                },
            );
        }
        let clip = |id: &str, source_id: SourceId, start| VideoClip {
            id: CompositionClipId::parse(id).unwrap(),
            source_id,
            placement: ClipPlacement {
                timeline_start_tick: start,
                source_in_tick: 0,
                source_out_tick: 2_000_000,
                speed: 1.0,
                speed_ramp: None,
            },
            playback_mode: PlaybackMode::Forward,
            stabilization: StabilizationSpec::Disabled,
            frame_interpolation: FrameInterpolation::Duplicate,
            transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(1.0),
            blend_mode: BlendMode::Normal,
            effects: Vec::new(),
            source_audio_enabled: true,
            audio_gain: AnimatableValue::constant(1.0),
            audio_pan: AnimatableValue::constant(0.0),
            enabled: true,
        };
        composition.tracks.push(CompositionTrack::Video {
            id: TrackId::parse("video-main").unwrap(),
            name: "Main".into(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![
                clip("clip-a", source_a, 0),
                clip("clip-b", source_b, second_start),
            ],
            transitions: Vec::new(),
        });
        CompositionRenderRequest {
            schema_version: 1,
            composition,
            output: CompositionOutputRequest::default(),
        }
    }

    fn trusted(request: &CompositionRenderRequest) -> BTreeMap<SourceId, TrustedCompositionSource> {
        request
            .composition
            .sources
            .iter()
            .map(|(id, source)| {
                (
                    id.clone(),
                    TrustedCompositionSource {
                        source: source.clone(),
                        fingerprint: Fingerprint::digest(id.as_str().as_bytes()),
                    },
                )
            })
            .collect()
    }

    fn animated(
        interpolation: Interpolation,
        end_tick: u64,
        from: f64,
        to: f64,
    ) -> AnimatableValue {
        AnimatableValue::Keyframes {
            track: KeyframeTrack::new(
                1_000_000,
                interpolation,
                vec![
                    Keyframe {
                        tick: 0,
                        value: from,
                    },
                    Keyframe {
                        tick: end_tick,
                        value: to,
                    },
                ],
            )
            .unwrap(),
        }
    }

    #[test]
    fn delivery_output_wire_is_backward_compatible_and_canonical() {
        let legacy: CompositionOutputRequest = serde_json::from_value(serde_json::json!({
            "format": "mp4",
            "codec": "h264",
            "qualityTier": "high"
        }))
        .unwrap();
        let canonical: CompositionOutputRequest = serde_json::from_value(serde_json::json!({
            "profile": { "container": "mp4", "codec": "h264" },
            "qualityTier": "high"
        }))
        .unwrap();
        assert_eq!(legacy, canonical);
        assert_eq!(
            serde_json::to_value(legacy).unwrap(),
            serde_json::json!({
                "profile": { "container": "mp4", "codec": "h264" },
                "qualityTier": "high"
            })
        );

        let mut old_request = serde_json::to_value(request(2_000_000)).unwrap();
        old_request.as_object_mut().unwrap().remove("output");
        let old_request: CompositionRenderRequest = serde_json::from_value(old_request).unwrap();
        assert_eq!(old_request.output, CompositionOutputRequest::default());

        let mixed = serde_json::from_value::<CompositionOutputRequest>(serde_json::json!({
            "profile": { "container": "webm", "codec": "vp9" },
            "format": "mp4",
            "codec": "h264"
        }))
        .unwrap_err();
        assert!(mixed.to_string().contains("cannot mix"));
    }

    #[test]
    fn every_delivery_profile_round_trips_and_changes_plan_identity() {
        let profiles = [
            CompositionExportProfile::Mp4 {
                codec: CompositionMp4Codec::H264,
            },
            CompositionExportProfile::Mp4 {
                codec: CompositionMp4Codec::H265,
            },
            CompositionExportProfile::Webm {
                codec: CompositionWebmCodec::Vp9,
            },
            CompositionExportProfile::Webm {
                codec: CompositionWebmCodec::Av1,
            },
            CompositionExportProfile::Mov {
                profile: CompositionProResProfile::Proxy,
            },
            CompositionExportProfile::Mov {
                profile: CompositionProResProfile::Lt,
            },
            CompositionExportProfile::Mov {
                profile: CompositionProResProfile::Standard,
            },
            CompositionExportProfile::Mov {
                profile: CompositionProResProfile::Hq,
            },
        ];
        let mut fingerprints = BTreeSet::new();
        for profile in profiles {
            let output = CompositionOutputRequest {
                profile,
                quality_tier: CompositionQualityTier::Compact,
            };
            let round_trip: CompositionOutputRequest =
                serde_json::from_value(serde_json::to_value(output).unwrap()).unwrap();
            assert_eq!(round_trip, output);
            let mut render_request = request(2_000_000);
            render_request.output = output;
            let plan =
                CompositionPlan::compile(render_request.clone(), trusted(&render_request)).unwrap();
            assert!(fingerprints.insert(plan.fingerprint().to_string()));
        }
        assert_eq!(fingerprints.len(), profiles.len());
    }

    #[test]
    fn plan_uses_trusted_sources_and_has_stable_identity() {
        let request = request(2_000_000);
        let plan = CompositionPlan::compile(request.clone(), trusted(&request)).unwrap();
        let again = CompositionPlan::compile(request.clone(), trusted(&request)).unwrap();
        assert_eq!(plan.fingerprint(), again.fingerprint());
        assert_eq!(plan.output(), CompositionOutputRequest::default());
        assert_eq!(
            plan.output()
                .quality_tier
                .video_quality(plan.output().profile),
            23
        );

        let mut changed = trusted(&request);
        changed
            .get_mut(&SourceId::parse("source-b").unwrap())
            .unwrap()
            .fingerprint = Fingerprint::digest(b"changed");
        let changed = CompositionPlan::compile(request, changed).unwrap();
        assert_ne!(plan.fingerprint(), changed.fingerprint());
    }

    #[test]
    fn first_slice_rejects_video_gaps_instead_of_ignoring_them() {
        let request = request(2_500_000);
        let error = CompositionPlan::compile(request.clone(), trusted(&request)).unwrap_err();
        assert!(error.to_string().contains("gaps/overlaps"));
    }

    #[test]
    fn source_requirements_promote_audio_use_to_video() {
        let request = request(2_000_000);
        let requirements = source_requirements(&request.composition).unwrap();
        assert_eq!(
            requirements[&SourceId::parse("source-a").unwrap()],
            SourceRequirement::Video
        );
    }

    #[test]
    fn plan_accepts_exact_speed_ramp_timing_and_muted_audio_does_not_require_source() {
        let mut request = request(2_000_000);
        let CompositionTrack::Video { clips, .. } = &mut request.composition.tracks[0] else {
            unreachable!();
        };
        clips[0].placement.speed = 1.0;
        clips[0].placement.speed_ramp = Some(SpeedRampSpec {
            interpolation: SpeedRampInterpolation::Linear,
            points: vec![
                SpeedRampPoint {
                    source_progress_tick: 0,
                    speed: 1.0,
                },
                SpeedRampPoint {
                    source_progress_tick: 2_000_000,
                    speed: 2.0,
                },
            ],
            audio_policy: SpeedRampAudioPolicy::PreservePitch,
        });
        let first_duration = clips[0].placement.timeline_duration_ticks().unwrap();
        assert_eq!(first_duration, 1_386_294);
        clips[1].placement.timeline_start_tick = first_duration;

        let muted_source = SourceId::parse("muted-ramp-source").unwrap();
        request.composition.sources.insert(
            muted_source.clone(),
            CompositionSource {
                id: muted_source.clone(),
                kind: SourceKind::Audio,
                duration_ticks: 2_000_000,
                width: 0,
                height: 0,
                has_audio: true,
            },
        );
        request.composition.tracks.push(CompositionTrack::Audio {
            id: TrackId::parse("muted-ramp-track").unwrap(),
            name: "Muted ramp".into(),
            muted: false,
            solo: false,
            locked: false,
            clips: vec![AudioClip {
                id: CompositionClipId::parse("muted-ramp-clip").unwrap(),
                source_id: muted_source.clone(),
                placement: ClipPlacement {
                    timeline_start_tick: 0,
                    source_in_tick: 0,
                    source_out_tick: 1_000_000,
                    speed: 1.0,
                    speed_ramp: Some(SpeedRampSpec {
                        interpolation: SpeedRampInterpolation::Hold,
                        points: vec![
                            SpeedRampPoint {
                                source_progress_tick: 0,
                                speed: 1.0,
                            },
                            SpeedRampPoint {
                                source_progress_tick: 1_000_000,
                                speed: 1.0,
                            },
                        ],
                        audio_policy: SpeedRampAudioPolicy::Mute,
                    }),
                },
                gain: AnimatableValue::constant(1.0),
                pan: AnimatableValue::constant(0.0),
                fade_in_ticks: 0,
                fade_out_ticks: 0,
                enabled: true,
            }],
        });

        let requirements = source_requirements(&request.composition).unwrap();
        assert!(!requirements.contains_key(&muted_source));
        assert!(request.composition.requires_speed_ramp());
        assert!(request.composition.requires_speed_ramp_pitch_audio());
        let mut resolved = trusted(&request);
        resolved.retain(|id, _| requirements.contains_key(id));
        CompositionPlan::compile(request, resolved).unwrap();
    }

    #[test]
    fn plan_accepts_static_video_and_image_layers_above_primary() {
        let mut request = request(2_000_000);
        let overlay_id = SourceId::parse("overlay-source").unwrap();
        let image_id = SourceId::parse("image-source").unwrap();
        request.composition.sources.insert(
            overlay_id.clone(),
            CompositionSource {
                id: overlay_id.clone(),
                kind: SourceKind::Video,
                duration_ticks: 2_000_000,
                width: 640,
                height: 360,
                has_audio: false,
            },
        );
        request.composition.sources.insert(
            image_id.clone(),
            CompositionSource {
                id: image_id.clone(),
                kind: SourceKind::Image,
                duration_ticks: 0,
                width: 128,
                height: 128,
                has_audio: false,
            },
        );
        let mut overlay = VideoClip {
            id: CompositionClipId::parse("overlay-clip").unwrap(),
            source_id: overlay_id,
            placement: ClipPlacement {
                timeline_start_tick: 500_000,
                source_in_tick: 0,
                source_out_tick: 1_000_000,
                speed: 1.0,
                speed_ramp: None,
            },
            playback_mode: PlaybackMode::Forward,
            stabilization: StabilizationSpec::Disabled,
            frame_interpolation: FrameInterpolation::Duplicate,
            transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(0.7),
            blend_mode: BlendMode::Screen,
            effects: Vec::new(),
            source_audio_enabled: true,
            audio_gain: AnimatableValue::constant(1.0),
            audio_pan: AnimatableValue::constant(0.0),
            enabled: true,
        };
        overlay.transform.x = AnimatableValue::constant(24.0);
        overlay.effects.push(VideoEffect::ChromaKey {
            color: Rgba {
                red: 0.0,
                green: 1.0,
                blue: 0.0,
                alpha: 1.0,
            },
            similarity: 0.1,
            softness: 0.02,
            spill: 0.0,
        });
        request.composition.tracks.insert(
            0,
            CompositionTrack::Image {
                id: TrackId::parse("image-layer").unwrap(),
                name: "Image".into(),
                hidden: false,
                locked: false,
                clips: vec![ImageClip {
                    id: CompositionClipId::parse("image-clip").unwrap(),
                    source_id: image_id,
                    timeline_start_tick: 1_000_000,
                    duration_ticks: 1_000_000,
                    transform: TransformSpec::default(),
                    opacity: AnimatableValue::constant(0.5),
                    blend_mode: BlendMode::Normal,
                    enabled: true,
                }],
            },
        );
        request.composition.tracks.insert(
            1,
            CompositionTrack::Video {
                id: TrackId::parse("overlay-layer").unwrap(),
                name: "Overlay".into(),
                hidden: false,
                muted: false,
                locked: false,
                clips: vec![overlay],
                transitions: Vec::new(),
            },
        );

        CompositionPlan::compile(request.clone(), trusted(&request)).unwrap();
    }

    #[test]
    fn plan_accepts_visual_keyframes_and_shape_masks_fail_closed_on_linear() {
        let mut request = request(2_000_000);
        let overlay_id = SourceId::parse("animated-overlay-source").unwrap();
        request.composition.sources.insert(
            overlay_id.clone(),
            CompositionSource {
                id: overlay_id.clone(),
                kind: SourceKind::Video,
                duration_ticks: 2_000_000,
                width: 320,
                height: 180,
                has_audio: false,
            },
        );
        let transform = TransformSpec {
            x: animated(Interpolation::EaseIn, 1_000_000, -40.0, 40.0),
            y: animated(Interpolation::EaseOut, 1_000_000, 20.0, -20.0),
            scale_x: animated(Interpolation::Linear, 1_000_000, 0.5, 1.0),
            scale_y: animated(Interpolation::Hold, 1_000_000, 0.5, 1.0),
            rotation_degrees: animated(Interpolation::EaseInOut, 1_000_000, -30.0, 30.0),
            ..TransformSpec::default()
        };
        let overlay = VideoClip {
            id: CompositionClipId::parse("animated-overlay-clip").unwrap(),
            source_id: overlay_id,
            placement: ClipPlacement {
                timeline_start_tick: 250_000,
                source_in_tick: 0,
                source_out_tick: 1_000_000,
                speed: 1.0,
                speed_ramp: None,
            },
            playback_mode: PlaybackMode::Forward,
            stabilization: StabilizationSpec::Disabled,
            frame_interpolation: FrameInterpolation::Duplicate,
            transform,
            opacity: animated(Interpolation::EaseInOutCubic, 1_000_000, 0.2, 1.0),
            blend_mode: BlendMode::Normal,
            effects: vec![VideoEffect::Mask {
                shape: MaskShape::Ellipse,
                x: animated(Interpolation::Linear, 1_000_000, 0.25, 0.75),
                y: AnimatableValue::constant(0.5),
                width: animated(Interpolation::EaseIn, 1_000_000, 0.25, 0.75),
                height: AnimatableValue::constant(0.75),
                feather: 0.15,
                inverted: true,
            }],
            source_audio_enabled: true,
            audio_gain: AnimatableValue::constant(1.0),
            audio_pan: AnimatableValue::constant(0.0),
            enabled: true,
        };
        request.composition.tracks.insert(
            0,
            CompositionTrack::Video {
                id: TrackId::parse("animated-overlay-layer").unwrap(),
                name: "Animated".into(),
                hidden: false,
                muted: false,
                locked: false,
                clips: vec![overlay],
                transitions: Vec::new(),
            },
        );
        CompositionPlan::compile(request.clone(), trusted(&request)).unwrap();

        let CompositionTrack::Video { clips, .. } = &mut request.composition.tracks[0] else {
            unreachable!();
        };
        let VideoEffect::Mask { shape, .. } = &mut clips[0].effects[0] else {
            unreachable!();
        };
        *shape = MaskShape::Linear;
        let error = CompositionPlan::compile(request.clone(), trusted(&request)).unwrap_err();
        assert!(error.to_string().contains("Linear mask"), "{error:#}");

        let CompositionTrack::Video { clips, .. } = &mut request.composition.tracks[0] else {
            unreachable!();
        };
        let VideoEffect::Mask { shape, .. } = &mut clips[0].effects[0] else {
            unreachable!();
        };
        *shape = MaskShape::Rectangle;
        clips[0].effects.push(VideoEffect::ChromaKey {
            color: Rgba {
                red: 0.0,
                green: 1.0,
                blue: 0.0,
                alpha: 1.0,
            },
            similarity: 0.1,
            softness: 0.0,
            spill: 0.0,
        });
        let error = CompositionPlan::compile(request.clone(), trusted(&request)).unwrap_err();
        assert!(error.to_string().contains("перед shape masks"), "{error:#}");
    }

    #[test]
    fn plan_rejects_visual_keyframes_past_the_clip_local_duration() {
        let mut request = request(2_000_000);
        let image_id = SourceId::parse("animated-image-source").unwrap();
        request.composition.sources.insert(
            image_id.clone(),
            CompositionSource {
                id: image_id.clone(),
                kind: SourceKind::Image,
                duration_ticks: 0,
                width: 64,
                height: 64,
                has_audio: false,
            },
        );
        request.composition.tracks.insert(
            0,
            CompositionTrack::Image {
                id: TrackId::parse("animated-image-layer").unwrap(),
                name: "Animated image".into(),
                hidden: false,
                locked: false,
                clips: vec![ImageClip {
                    id: CompositionClipId::parse("animated-image-clip").unwrap(),
                    source_id: image_id,
                    timeline_start_tick: 0,
                    duration_ticks: 1_000_000,
                    transform: TransformSpec::default(),
                    opacity: animated(Interpolation::Linear, 1_000_001, 0.0, 1.0),
                    blend_mode: BlendMode::Normal,
                    enabled: true,
                }],
            },
        );
        let error = CompositionPlan::compile(request.clone(), trusted(&request)).unwrap_err();
        assert!(
            error.to_string().contains("clip-local duration"),
            "{error:#}"
        );
    }

    #[test]
    fn plan_validates_source_and_independent_audio_automation_only_when_active() {
        let mut request = request(2_000_000);
        let CompositionTrack::Video { clips, .. } = &mut request.composition.tracks[0] else {
            unreachable!();
        };
        clips[0].audio_gain = animated(Interpolation::EaseIn, 2_000_000, 0.25, 1.0);
        clips[0].audio_pan = animated(Interpolation::EaseOut, 2_000_000, -1.0, 1.0);
        request.composition.tracks.push(CompositionTrack::Audio {
            id: TrackId::parse("automated-audio").unwrap(),
            name: "Automated".into(),
            muted: false,
            solo: false,
            locked: false,
            clips: vec![AudioClip {
                id: CompositionClipId::parse("automated-audio-clip").unwrap(),
                source_id: SourceId::parse("source-a").unwrap(),
                placement: ClipPlacement {
                    timeline_start_tick: 500_000,
                    source_in_tick: 0,
                    source_out_tick: 1_000_000,
                    speed: 1.0,
                    speed_ramp: None,
                },
                gain: animated(Interpolation::EaseInOut, 1_000_000, 0.1, 0.9),
                pan: animated(Interpolation::Hold, 1_000_000, -0.5, 0.5),
                fade_in_ticks: 100_000,
                fade_out_ticks: 100_000,
                enabled: true,
            }],
        });
        CompositionPlan::compile(request.clone(), trusted(&request)).unwrap();

        let mut outside = request.clone();
        let CompositionTrack::Video { clips, .. } = &mut outside.composition.tracks[0] else {
            unreachable!();
        };
        clips[0].audio_gain = animated(Interpolation::Linear, 2_000_001, 0.0, 1.0);
        let error = CompositionPlan::compile(outside.clone(), trusted(&outside)).unwrap_err();
        assert!(
            error.to_string().contains("clip-local duration"),
            "{error:#}"
        );

        let mut out_of_range = request.clone();
        let CompositionTrack::Audio { clips, .. } = &mut out_of_range.composition.tracks[1] else {
            unreachable!();
        };
        clips[0].pan = animated(Interpolation::Linear, 1_000_000, -1.0, 1.01);
        let error =
            CompositionPlan::compile(out_of_range.clone(), trusted(&out_of_range)).unwrap_err();
        assert!(error.to_string().contains("диапазона"), "{error:#}");

        let CompositionTrack::Video { muted, .. } = &mut outside.composition.tracks[0] else {
            unreachable!();
        };
        *muted = true;
        CompositionPlan::compile(outside.clone(), trusted(&outside)).unwrap();
    }

    #[test]
    fn plan_accepts_bounded_optical_flow_and_rejects_unsafe_work() {
        let mut request = request(4_000_000);
        let CompositionTrack::Video { clips, .. } = &mut request.composition.tracks[0] else {
            unreachable!();
        };
        clips[0].placement.speed = 0.5;
        clips[0].frame_interpolation = FrameInterpolation::OpticalFlow;
        assert!(request.composition.requires_optical_flow());
        CompositionPlan::compile(request.clone(), trusted(&request)).unwrap();

        request.composition.canvas.width = 3_840;
        request.composition.canvas.height = 2_160;
        let error = CompositionPlan::compile(request.clone(), trusted(&request)).unwrap_err();
        assert!(error.to_string().contains("dimension budget"), "{error:#}");
    }

    #[test]
    fn plan_accepts_bounded_reverse_and_freeze_silences_source_audio() {
        let mut reverse = request(2_000_000);
        let CompositionTrack::Video { clips, .. } = &mut reverse.composition.tracks[0] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Reverse;
        CompositionPlan::compile(reverse.clone(), trusted(&reverse)).unwrap();

        let mut freeze = reverse.clone();
        let CompositionTrack::Video { clips, .. } = &mut freeze.composition.tracks[0] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Freeze { source_tick: 0 };
        clips[0].audio_gain = AnimatableValue::constant(MAX_AUDIO_GAIN + 1.0);
        CompositionPlan::compile(freeze.clone(), trusted(&freeze)).unwrap();

        let mut oversized = reverse;
        let source = oversized
            .composition
            .sources
            .get_mut(&SourceId::parse("source-a").unwrap())
            .unwrap();
        source.width = 3_840;
        source.height = 2_160;
        let error = CompositionPlan::compile(oversized.clone(), trusted(&oversized)).unwrap_err();
        assert!(error.to_string().contains("dimension budget"), "{error:#}");
    }

    #[test]
    fn plan_accepts_bounded_stabilization_with_reverse_optical_and_rejects_freeze() {
        let mut request = request(4_000_000);
        let CompositionTrack::Video { clips, .. } = &mut request.composition.tracks[0] else {
            unreachable!();
        };
        clips[0].placement.speed = 0.5;
        clips[0].playback_mode = PlaybackMode::Reverse;
        clips[0].frame_interpolation = FrameInterpolation::OpticalFlow;
        clips[0].stabilization = StabilizationSpec::Deshake {
            radius_x: 32,
            radius_y: 16,
        };
        CompositionPlan::compile(request.clone(), trusted(&request)).unwrap();

        let mut freeze = request.clone();
        let CompositionTrack::Video { clips, .. } = &mut freeze.composition.tracks[0] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Freeze { source_tick: 0 };
        clips[0].frame_interpolation = FrameInterpolation::Duplicate;
        let error = CompositionPlan::compile(freeze.clone(), trusted(&freeze)).unwrap_err();
        assert!(
            error.to_string().contains("InvalidStabilization"),
            "{error:#}"
        );

        let mut oversized = request;
        let CompositionTrack::Video { clips, .. } = &mut oversized.composition.tracks[0] else {
            unreachable!();
        };
        clips[0].placement.speed = 1.0;
        clips[0].playback_mode = PlaybackMode::Forward;
        clips[0].frame_interpolation = FrameInterpolation::Duplicate;
        let source = oversized
            .composition
            .sources
            .get_mut(&SourceId::parse("source-a").unwrap())
            .unwrap();
        source.width = 3_840;
        source.height = 2_160;
        let error = CompositionPlan::compile(oversized.clone(), trusted(&oversized)).unwrap_err();
        assert!(
            error.to_string().contains("stabilization clip")
                && error.to_string().contains("dimension budget"),
            "{error:#}"
        );
    }

    #[test]
    fn plan_accepts_static_unicode_text_layer() {
        let mut request = request(2_000_000);
        request.composition.tracks.insert(
            0,
            CompositionTrack::Text {
                id: TrackId::parse("text-layer").unwrap(),
                name: "Text".into(),
                hidden: false,
                locked: false,
                clips: vec![TextClip {
                    id: CompositionClipId::parse("text-clip").unwrap(),
                    timeline_start_tick: 250_000,
                    timeline_end_tick: 1_750_000,
                    text: "Привет, мир!\n字幕".into(),
                    style: TextStyle {
                        font_family: "Noto Sans".into(),
                        font_size: 64.0,
                        color: Rgba {
                            red: 1.0,
                            green: 1.0,
                            blue: 1.0,
                            alpha: 1.0,
                        },
                        background: Rgba::BLACK,
                        stroke: Rgba::BLACK,
                        stroke_width: 2.0,
                        shadow: Rgba::BLACK,
                        shadow_x: 3.0,
                        shadow_y: 3.0,
                    },
                    transform: TransformSpec::default(),
                    opacity: AnimatableValue::constant(0.8),
                    enabled: true,
                }],
            },
        );
        CompositionPlan::compile(request.clone(), trusted(&request)).unwrap();
    }

    #[test]
    fn plan_accepts_exact_handle_backed_primary_transition() {
        let mut request = request(2_000_000);
        let CompositionTrack::Video {
            clips, transitions, ..
        } = &mut request.composition.tracks[0]
        else {
            unreachable!()
        };
        for clip in clips.iter_mut() {
            clip.placement.source_in_tick = 500_000;
            clip.placement.source_out_tick = 2_500_000;
        }
        transitions.push(ClipTransition {
            id: TransitionId::parse("transition-a-b").unwrap(),
            from_clip_id: clips[0].id.clone(),
            to_clip_id: clips[1].id.clone(),
            duration_ticks: 500_000,
            kind: TransitionKind::WipeLeft,
        });
        CompositionPlan::compile(request.clone(), trusted(&request)).unwrap();
    }

    #[test]
    fn render_requirements_ignore_inactive_sources_and_match_audio_solo() {
        let mut request = request(2_000_000);
        let add_source = |composition: &mut Composition, name: &str| {
            let id = SourceId::parse(name).unwrap();
            composition.sources.insert(
                id.clone(),
                CompositionSource {
                    id: id.clone(),
                    kind: SourceKind::Video,
                    duration_ticks: 4_000_000,
                    width: 640,
                    height: 360,
                    has_audio: true,
                },
            );
            id
        };
        let hidden = add_source(&mut request.composition, "hidden-source");
        let muted = add_source(&mut request.composition, "muted-source");
        let non_solo = add_source(&mut request.composition, "non-solo-source");
        let solo = add_source(&mut request.composition, "solo-source");
        let video = |id: &str, source_id: SourceId| VideoClip {
            id: CompositionClipId::parse(id).unwrap(),
            source_id,
            placement: ClipPlacement {
                timeline_start_tick: 0,
                source_in_tick: 0,
                source_out_tick: 1_000_000,
                speed: 1.0,
                speed_ramp: None,
            },
            playback_mode: PlaybackMode::Forward,
            stabilization: StabilizationSpec::Disabled,
            frame_interpolation: FrameInterpolation::Duplicate,
            transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(1.0),
            blend_mode: BlendMode::Normal,
            effects: Vec::new(),
            source_audio_enabled: true,
            audio_gain: AnimatableValue::constant(1.0),
            audio_pan: AnimatableValue::constant(0.0),
            enabled: true,
        };
        request.composition.tracks.insert(
            0,
            CompositionTrack::Video {
                id: TrackId::parse("hidden-video").unwrap(),
                name: "Hidden".into(),
                hidden: true,
                muted: false,
                locked: false,
                clips: vec![video("hidden-clip", hidden.clone())],
                transitions: Vec::new(),
            },
        );
        let audio_track = |id: &str, source_id: SourceId, muted, solo| CompositionTrack::Audio {
            id: TrackId::parse(id).unwrap(),
            name: id.into(),
            muted,
            solo,
            locked: false,
            clips: vec![AudioClip {
                id: CompositionClipId::parse(format!("{id}-clip")).unwrap(),
                source_id,
                placement: ClipPlacement {
                    timeline_start_tick: 0,
                    source_in_tick: 0,
                    source_out_tick: 1_000_000,
                    speed: 1.0,
                    speed_ramp: None,
                },
                gain: AnimatableValue::constant(1.0),
                pan: AnimatableValue::constant(0.0),
                fade_in_ticks: 0,
                fade_out_ticks: 0,
                enabled: true,
            }],
        };
        request
            .composition
            .tracks
            .push(audio_track("muted-audio", muted.clone(), true, false));
        request.composition.tracks.push(audio_track(
            "non-solo-audio",
            non_solo.clone(),
            false,
            false,
        ));
        request
            .composition
            .tracks
            .push(audio_track("solo-audio", solo.clone(), false, true));

        let requirements = source_requirements(&request.composition).unwrap();
        assert!(!requirements.contains_key(&hidden));
        assert!(!requirements.contains_key(&muted));
        assert!(!requirements.contains_key(&non_solo));
        assert_eq!(requirements[&solo], SourceRequirement::Audio);

        let mut active_trusted = trusted(&request);
        active_trusted.retain(|id, _| requirements.contains_key(id));
        CompositionPlan::compile(request, active_trusted).unwrap();
    }
}
