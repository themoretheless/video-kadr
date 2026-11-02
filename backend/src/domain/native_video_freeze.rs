//! Native keyed-pose freeze: preserves native picture/audio ownership without source interchange.
use super::{composition::*, concat_retime_edit::Clock};
use anyhow::{ensure, Context, Result};
fn shift(value: &mut AnimatableValue, offset_us: u64, preserve_grid: bool) -> Result<()> {
    if let AnimatableValue::Keyframes { track } = value {
        let ticks = u128::from(offset_us) * u128::from(track.time_base);
        if preserve_grid || track.sample_clock.is_some() || ticks % 1_000_000 != 0 {
            let clock = track.sample_clock.unwrap_or_else(super::native_keyframe_clock::Clock::identity)
                .shifted(i64::try_from(offset_us).context("animation clock shift overflow")?)?;
            clock.prepare(track)?;
            track.sample_clock = Some(clock);
            return Ok(());
        }
        let delta = i64::try_from(ticks / 1_000_000).context("animation shift overflow")?;
        for key in &mut track.keyframes {
            key.tick = key
                .tick
                .checked_sub(delta)
                .context("animation key shift overflow")?;
            ensure!(
                key.tick.unsigned_abs() <= 9_007_199_254_740_991,
                "animation tick exceeds wire range"
            );
        }
    }
    Ok(())
}
fn retime(value: &mut AnimatableValue, duration_us: u64) -> Result<()> {
    if let AnimatableValue::Keyframes { track } = value {
        let base = u128::from(duration_us) * u128::from(track.time_base);
        if track.sample_clock.is_some() || base % 1_000_000 != 0 {
            let clock = track.sample_clock.unwrap_or_else(super::native_keyframe_clock::Clock::identity)
                .retimed(duration_us, 1_000_000)?;
            clock.prepare(track)?;
            track.sample_clock = Some(clock);
            return Ok(());
        }
        let base =
            u64::try_from(base / 1_000_000).context("freeze animation time base overflow")?;
        ensure!(
            base > 0 && base <= 9_007_199_254_740_991,
            "freeze animation time base exceeds wire range"
        );
        track.time_base = base;
    }
    Ok(())
}
fn scale(value: &mut AnimatableValue, numerator: f64, denominator: f64) {
    match value {
        AnimatableValue::Constant { value } => *value = *value * numerator / denominator,
        AnimatableValue::Keyframes { track } => {
            for key in &mut track.keyframes {
                key.value = key.value * numerator / denominator
            }
        }
    }
}
pub fn eligible(composition: &Composition, target: &str) -> bool {
    composition.tracks.iter().any(|track| match track {
        CompositionTrack::Video {
            clips, transitions, ..
        } if transitions.is_empty() => clips.iter().any(|clip| {
            clip.id.as_str() == target
                && clip.placement.concat_retime.is_none()
                && clip.placement.speed_ramp.is_none()
                && clip.placement.source_timeline_offset_tick.is_none()
                && clip.placement.source_head_timeline_tick.is_none()
                && clip.transform.concat_preparation.is_none()
                && clip.transform.concat_animation.is_none()
                && clip.transform.position.is_none()
                && clip.effects.is_empty()
                && clip.dsp_effects.is_empty()
                && clip.concat_gain_keys.is_empty()
                && clip.concat_cutout.is_none()
                && clip.filmcraft_motion.is_none()
                && clip.effect_timing.is_none()
                && clip.playback_mode == PlaybackMode::Forward
                && clip.stabilization == StabilizationSpec::Disabled
                && clip.frame_interpolation == FrameInterpolation::Duplicate
        }),
        _ => false,
    })
}
/// Caller has validated the composition, fresh media/clip IDs, group structure and lock state.
pub fn splice(
    composition: &Composition,
    target: &str,
    time: f64,
    still: &CompositionSource,
    tail_id: &str,
    still_id: &str,
    image_track_id: &str,
) -> Result<Option<Composition>> {
    ensure!(
        eligible(composition, target),
        "native freeze profile requires qualified animation/effect handling"
    );
    let index=composition.tracks.iter().position(|track|matches!(track,CompositionTrack::Video{clips,..} if clips.iter().any(|clip|clip.id.as_str()==target))).context("native freeze video lane missing")?;
    let CompositionTrack::Video {
        id: owner,
        clips,
        hidden,
        locked,
        ..
    } = &composition.tracks[index]
    else {
        unreachable!()
    };
    ensure!(!locked, "freeze lane is locked");
    let picture = clips
        .iter()
        .find(|clip| clip.id.as_str() == target)
        .unwrap();
    let placement = &picture.placement;
    let duration = placement.timeline_duration_ticks()?;
    let clock = Clock {
        start: placement.timeline_start_tick as f64 / 1e6,
        duration: duration as f64 / 1e6,
        source_start: placement.source_in_tick as f64 / 1e6,
        speed: placement.speed,
        speed_curve: None,
    };
    let Some(plan) = clock.freeze(time, Some(1.))? else {
        return Ok(None);
    };
    ensure!(
        (time * 1e6).round() / 1e6 == time,
        "ordinary freeze needs native tick precision"
    );
    ensure!(
        duration
            == ((placement.source_out_tick - placement.source_in_tick) as f64 / placement.speed)
                .round() as u64,
        "native authored duration needs a separate freeze mapping"
    );
    let at = (time * 1e6).round() as u64;
    let offset = at
        .checked_sub(placement.timeline_start_tick)
        .context("freeze precedes clip")?;
    let boundary = (plan.tail.source_start * 1e6).round() as u64;
    ensure!(
        placement.source_in_tick < boundary && boundary < placement.source_out_tick,
        "native freeze source boundary is outside clip"
    );
    let mut head = picture.clone();
    head.placement.source_out_tick = boundary;
    head.placement.timeline_duration_tick = Some(offset);
    head.audio_fade_out_ticks = 0;
    let mut tail = picture.clone();
    tail.id = CompositionClipId::parse(tail_id.to_owned())?;
    tail.placement.source_in_tick = boundary;
    tail.placement.timeline_start_tick =
        at.checked_add(1_000_000).context("freeze tail overflow")?;
    tail.placement.timeline_duration_tick = Some(duration - offset);
    tail.audio_fade_in_ticks = 0;
    for value in [
        &mut tail.transform.x,
        &mut tail.transform.y,
        &mut tail.transform.scale_x,
        &mut tail.transform.scale_y,
        &mut tail.transform.rotation_degrees,
        &mut tail.opacity,
        &mut tail.audio_gain,
    ] {
        shift(value, offset, false)?;
    }
    shift(&mut tail.audio_pan, offset, true)?;
    ensure!(
        still.width <= 3840 && still.height <= 2160,
        "freeze JPEG exceeds native image decoder bounds"
    );
    let source = composition
        .sources
        .get(&picture.source_id)
        .context("native freeze source missing")?;
    ensure!(
        source.width <= 3840 && source.height <= 2160,
        "source raster exceeds native decoder bounds"
    );
    let mut transform = picture.transform.clone();
    for value in [
        &mut transform.x,
        &mut transform.y,
        &mut transform.scale_x,
        &mut transform.scale_y,
        &mut transform.rotation_degrees,
    ] {
        retime(value, duration)?;
    }
    scale(
        &mut transform.scale_x,
        source.width as f64,
        still.width as f64,
    );
    scale(
        &mut transform.scale_y,
        source.height as f64,
        still.height as f64,
    );
    let mut opacity = picture.opacity.clone();
    retime(&mut opacity, duration)?;
    let frozen = ImageClip {
        concat_source_timing: None,
id: CompositionClipId::parse(still_id.to_owned())?,
        source_id: still.id.clone(),
        timeline_start_tick: at,
        duration_ticks: 1_000_000,
        transform,
        opacity,
        blend_mode: picture.blend_mode,
        effects: vec![],
        enabled: picture.enabled,
    };
    let mut result = composition.clone();
    let CompositionTrack::Video { clips, .. } = &mut result.tracks[index] else {
        unreachable!()
    };
    let mut replacements = vec![];
    for clip in clips.iter() {
        if clip.id.as_str() == target {
            replacements.push(head.clone());
            replacements.push(tail.clone());
        } else {
            let mut clip = clip.clone();
            if clip.placement.timeline_start_tick >= at {
                clip.placement.timeline_start_tick = clip
                    .placement
                    .timeline_start_tick
                    .checked_add(1_000_000)
                    .context("native freeze ripple overflow")?;
            }
            replacements.push(clip);
        }
    }
    *clips = replacements;
    let sibling = super::concat_lane_groups::groups(composition)?
        .into_iter()
        .find(|(video, _)| *video == index)
        .map(|(_, image)| image);
    let image_id = if let Some(image) = sibling {
        let CompositionTrack::Image { id, clips, .. } = &mut result.tracks[image] else {
            unreachable!()
        };
        for clip in clips.iter_mut() {
            if clip.timeline_start_tick >= at {
                clip.timeline_start_tick = clip
                    .timeline_start_tick
                    .checked_add(1_000_000)
                    .context("native image ripple overflow")?;
            }
        }
        clips.push(frozen);
        id.clone()
    } else {
        let id = TrackId::parse(image_track_id.to_owned())?;
        result.tracks.insert(
            index,
            CompositionTrack::Image {
                id: id.clone(),
                name: "Стоп-кадр".into(),
                hidden: *hidden,
                locked: false,
                clips: vec![frozen],
            },
        );
        id
    };
    result.sources.insert(still.id.clone(), still.clone());
    let name = composition
        .metadata
        .as_ref()
        .and_then(|meta| meta.clip_names.get(&picture.id))
        .cloned()
        .unwrap_or_else(|| target.into());
    let metadata = result.metadata.get_or_insert_with(Default::default);
    metadata
        .concat_lane_groups
        .insert(owner.clone(), vec![owner.clone(), image_id]);
    for id in [tail_id, still_id] {
        metadata
            .clip_names
            .insert(CompositionClipId::parse(id.to_owned())?, name.clone());
    }
    if let Some(version) = composition
        .metadata
        .as_ref()
        .and_then(|meta| meta.media_versions.get(&picture.id))
    {
        metadata.media_versions.insert(
            CompositionClipId::parse(tail_id.to_owned())?,
            version.clone(),
        );
    }
    metadata
        .source_names
        .insert(still.id.clone(), "Стоп-кадр".into());
    result.validate_native_surface()?;
    Ok(Some(result))
}
