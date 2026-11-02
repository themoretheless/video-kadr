//! Source handles needed by the first covering independent transition window.
//! None means ordinary playback; Some(empty) preserves a transparent active window.
use super::composition::{ClipTransition, SourceId, VideoClip};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeSet;

/// Resolve the legacy centered cut without mutating the authored wire document.
/// Odd microsecond durations retain the existing floor-before/ceil-after rule.
pub fn resolved_span(clips: &[VideoClip], transition: &ClipTransition) -> Result<(i64, i64)> {
    if let Some(timeline) = transition.timeline {
        return timeline.source_span().map_err(|_| anyhow::anyhow!("invalid explicit transition span"));
    }
    let from = clips.iter().find(|clip| Some(&clip.id) == transition.from_clip_id.as_ref()).context("legacy transition outgoing clip missing")?;
    let to = clips.iter().find(|clip| Some(&clip.id) == transition.to_clip_id.as_ref()).context("legacy transition incoming clip missing")?;
    let cut = from.placement.timeline_end_tick()?;
    ensure!(cut == to.placement.timeline_start_tick, "legacy transition clips must touch");
    ensure!(transition.duration_ticks > 0, "legacy transition duration must be positive");
    let unit = super::caption_time::SOURCE_TICKS_PER_MICROSECOND;
    let start = i64::try_from((cut as i128 - (transition.duration_ticks / 2) as i128) * unit)?;
    let duration = i64::try_from(transition.duration_ticks as i128 * unit)?;
    start.checked_add(duration).context("legacy transition end overflow")?;
    Ok((start, duration))
}

pub fn active_sources(clips: &[VideoClip], transitions: &[ClipTransition], time_us: u64) -> Result<Option<BTreeSet<SourceId>>> {
    let exact = super::caption_time::CaptionTime::Microseconds(time_us).exact_ticks();
    let mut active = None;
    for transition in transitions {
        let (start, duration) = resolved_span(clips, transition)?;
        if exact >= start as i128 && exact < start as i128 + duration as i128 { active = Some(transition); break; }
    }
    let Some(transition) = active else { return Ok(None) };
    let mut sources = BTreeSet::new();
    {
        for id in [transition.from_clip_id.as_ref(), transition.to_clip_id.as_ref()].into_iter().flatten() {
            if let Some(clip) = clips.iter().find(|clip| &clip.id == id && clip.enabled) {
                if super::video_clock::transition_source_tick(&clip.placement, clip.playback_mode, exact)?.is_some() {
                    sources.insert(clip.source_id.clone());
                }
            }
        }
    }
    Ok(Some(sources))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_handles_include_both_sides_and_respect_first_window_and_disabled() {
        let track: super::super::composition::CompositionTrack = serde_json::from_value(serde_json::json!({
            "kind":"video","id":"video","name":"Video",
            "clips":[
                {"id":"a","sourceId":"red","transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","placement":{"timelineStartTick":0,"sourceInTick":20000000,"sourceOutTick":30000000,"speed":1.0}},
                {"id":"b","sourceId":"blue","transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","placement":{"timelineStartTick":10000000,"sourceInTick":20000000,"sourceOutTick":30000000,"speed":1.0}}
            ],
            "transitions":[
                {"id":"first","fromClipId":"a","toClipId":"b","kind":"dissolve","timeline":{"startTick":"2286144000000","durationTick":"508032000000"}},
                {"id":"second","toClipId":"b","kind":"dissolve","timeline":{"startTick":"2032128000000","durationTick":"1016064000000"}}
            ]
        })).unwrap();
        let super::super::composition::CompositionTrack::Video { mut clips, transitions, .. } = track else { unreachable!() };
        let ids = |values: &[&str]| values.iter().map(|id| SourceId::parse(*id).unwrap()).collect::<BTreeSet<_>>();
        // Both sides require handles across the ten-second edit, including outside clip placement.
        for time in [9_000_000, 9_500_000, 10_000_000, 10_500_000, 10_999_999] {
            assert_eq!(active_sources(&clips,&transitions,time).unwrap(),Some(ids(&["red","blue"])));
        }
        assert_eq!(active_sources(&clips,&transitions,11_000_000).unwrap(),Some(ids(&["blue"])));
        assert_eq!(active_sources(&clips,&transitions,12_000_000).unwrap(),None);
        clips[0].enabled=false;
        assert_eq!(active_sources(&clips,&transitions,9_500_000).unwrap(),Some(ids(&["blue"])));
        assert_eq!(active_sources(&clips,&transitions[1..],9_500_000).unwrap(),Some(ids(&["blue"])));
        clips[1].enabled=false;
        assert_eq!(active_sources(&clips,&transitions,9_500_000).unwrap(),Some(BTreeSet::new()));
    }
}

#[cfg(test)]
mod legacy_tests {
    use super::*;
    #[test]
    fn centered_legacy_window_preserves_odd_ticks_and_discovers_both_sources() {
        let track: super::super::composition::CompositionTrack = serde_json::from_value(serde_json::json!({
            "kind":"video","id":"video","name":"Video","clips":[
                {"id":"a","sourceId":"red","transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","placement":{"timelineStartTick":0,"sourceInTick":2000000,"sourceOutTick":3000000,"speed":1.0}},
                {"id":"b","sourceId":"blue","transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","placement":{"timelineStartTick":1000000,"sourceInTick":2000000,"sourceOutTick":3000000,"speed":1.0}}
            ],"transitions":[{"id":"cut","fromClipId":"a","toClipId":"b","durationTicks":1000001,"kind":"dissolve"}]
        })).unwrap();
        let super::super::composition::CompositionTrack::Video { mut clips, mut transitions, .. } = track else { unreachable!() };
        let original = transitions[0].clone();
        assert_eq!(resolved_span(&clips, &original).unwrap(), (500000 * 254016, 1000001 * 254016));
        let both = BTreeSet::from([SourceId::parse("red").unwrap(), SourceId::parse("blue").unwrap()]);
        for time in [500000, 999999, 1000000, 1500000] { assert_eq!(active_sources(&clips, &transitions, time).unwrap(), Some(both.clone())); }
        for time in [499999, 1500001] { assert_eq!(active_sources(&clips, &transitions, time).unwrap(), None); }
        assert_eq!(transitions[0], original);
        clips[0].placement.source_out_tick = clips[0].placement.source_in_tick + 1;
        clips[1].placement.timeline_start_tick = 1;
        transitions[0].duration_ticks = 5;
        assert_eq!(resolved_span(&clips, &transitions[0]).unwrap(), (-254016, 5 * 254016));
        clips[1].placement.timeline_start_tick = 2;
        assert!(resolved_span(&clips, &transitions[0]).is_err());
    }
}
