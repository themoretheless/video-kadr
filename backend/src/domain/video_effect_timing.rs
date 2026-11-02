//! Persisted effect clock semantics, independent of the decoded frame clock.
use super::{
    composition::{PlaybackMode, VideoClip},
    effect_frame_clock::EffectFrameClock,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectTiming {
    FilmcraftSource {
        #[serde(default, rename = "holdFilters")]
        hold_filters: bool,
        /// Freeze playback alone does not retain the original moving direction.
        #[serde(default, rename = "reverseWhileFrozen")]
        reverse_while_frozen: bool,
    },
}

pub fn sample(clip: &VideoClip, timeline: i128) -> Result<Option<EffectFrameClock>> {
    let timeline = i64::try_from(timeline)? as i128;
    let Some(frame) = super::video_clock::transition_source_sample_tick(
        &clip.placement,
        clip.playback_mode,
        timeline,
    )?
    else {
        return Ok(None);
    };
    let origin=if let Some(clock)=&clip.placement.concat_retime {clock.timeline_window_ticks(clip.placement.timeline_start_tick,clip.placement.source_in_tick)?.0}else{clip.placement.timeline_start_tick as i128*254_016};
    let generator = i64::try_from(timeline-origin)?;
    let parameters = match clip.effect_timing {
        None => generator,
        Some(EffectTiming::FilmcraftSource {
            hold_filters: true, ..
        }) if matches!(clip.playback_mode, PlaybackMode::Freeze { .. }) => frame,
        Some(EffectTiming::FilmcraftSource {
            reverse_while_frozen,
            ..
        }) => {
            let moving = match clip.playback_mode {
                PlaybackMode::Freeze { .. } => {
                    if reverse_while_frozen {
                        PlaybackMode::Reverse
                    } else {
                        PlaybackMode::Forward
                    }
                }
                mode => mode,
            };
            let Some(parameters) = super::video_clock::transition_source_sample_tick(
                &clip.placement,
                moving,
                timeline,
            )?
            else {
                return Ok(None);
            };
            parameters
        }
    };
    Ok(Some(EffectFrameClock {
        frame,
        parameters,
        generator,
    }))
}

/// One neighboring picture request retains independent frame, parameter and local clocks.
#[derive(Debug, Clone, Copy)]
pub struct TemporalSample {
    pub timeline_tick: i64,
    pub clocks: EffectFrameClock,
    pub frame_time: super::video_clock::FrameTime,
    pub local_seconds: f64,
    pub progress: f64,
    pub duration_us: u64,
}
pub fn temporal_sample(clip: &VideoClip, origin: i64, delta_seconds: f64) -> Result<Option<TemporalSample>> {
    use anyhow::ensure;
    let delta = (delta_seconds * 254_016_000_000.0).round();
    ensure!(delta.is_finite() && delta >= i64::MIN as f64 && delta < -(i64::MIN as f64), "effect temporal offset overflow");
    let timeline_tick = origin.checked_add(delta as i64).ok_or_else(||anyhow::anyhow!("effect temporal timeline overflow"))?;
    let Some(clocks) = sample(clip, i128::from(timeline_tick))? else { return Ok(None) };
    let (frame_time, duration) = if let Some(retained) = &clip.placement.concat_retime {
        (super::video_clock::FrameTime::SourceTicks(clocks.frame), retained.duration_source_ticks()?)
    } else {
        (super::video_clock::FrameTime::Microseconds(u64::try_from(clocks.frame / 254_016)?), i128::from(clip.placement.timeline_duration_ticks()?) * 254_016)
    };
    ensure!(duration > 0, "invalid temporal picture duration");
    let duration_us = if clip.placement.concat_retime.is_some() {(duration as f64 / 254_016.).round().max(1.) as u64} else {clip.placement.timeline_duration_ticks()?};
    let local_seconds=clocks.generator as f64 / 254_016_000_000.;
    let progress=if clip.placement.concat_retime.is_some(){clocks.generator as f64/duration as f64}else{local_seconds*1_000_000./duration_us.max(1) as f64};
    Ok(Some(TemporalSample { timeline_tick, clocks, frame_time, local_seconds, progress:progress.clamp(0.,1.), duration_us }))
}

pub fn parameter_time(
    clip: &VideoClip,
    clocks: EffectFrameClock,
) -> super::animation_sample_time::AnimationSampleTime {
    use super::animation_sample_time::AnimationSampleTime;
    if clip.effect_timing.is_some() {
        AnimationSampleTime::FilmcraftSourceTicks(clocks.parameters)
    } else {
        AnimationSampleTime::SourceTicks(clocks.parameters)
    }
}

#[cfg(test)]
#[path = "video_effect_timing_original_tests.rs"]
mod original_tests;
