//! Functional caption edit algorithms adapted from Filmcraft edit/captions.rs.
//! MIT notice: docs/licenses/filmcraft-MIT.txt. Clock units are supplied by the caller.
use super::caption_time::{CaptionDelta,CaptionTime,SOURCE_TICKS_PER_MICROSECOND as UNIT};
use super::{
    caption_timeline::{CaptionCue, CaptionTrack},
    composition::{CompositionClipId, TrackId},
};
use anyhow::{bail, ensure, Result};
use std::collections::BTreeSet;
#[derive(Debug, Clone)]
pub enum CaptionEdit {
    Add {
        track: TrackId,
        cue: CaptionCue,
    },
    Split {
        id: CompositionClipId,
        time: CaptionTime,
        new_id: CompositionClipId,
    },
    Merge {
        ids: Vec<CompositionClipId>,
    },
    Text {
        id: CompositionClipId,
        text: Option<String>,
        speaker: Option<Option<String>>,
    },
    Times {
        id: CompositionClipId,
        start: CaptionTime,
        end: CaptionTime,
    },
    Trim {
        id: CompositionClipId,
        in_edge: bool,
        delta: CaptionDelta,
    },
    Move {
        ids: Vec<CompositionClipId>,
        delta: CaptionDelta,
    },
    Delete {
        ids: Vec<CompositionClipId>,
        ripple: bool,
    },
}
#[derive(Debug, Clone)]
pub struct CaptionEditResult {
    pub tracks: Vec<CaptionTrack>,
    pub applied: Option<CaptionDelta>,
    pub deleted: usize,
}
fn validate(tracks: &[CaptionTrack]) -> Result<()> {
    let mut tids = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for track in tracks {
        track.validate()?;
        ensure!(tids.insert(&track.id), "duplicate caption track");
        for cue in &track.clips {
            ensure!(ids.insert(&cue.id), "duplicate caption cue");
        }
    }
    Ok(())
}
fn locate(tracks: &[CaptionTrack], id: &CompositionClipId) -> Result<(usize, usize)> {
    for (ti, track) in tracks.iter().enumerate() {
        if let Some(ci) = track.clips.iter().position(|cue| &cue.id == id) {
            ensure!(!track.locked, "caption track is locked");
            return Ok((ti, ci));
        }
    }
    bail!("caption not found")
}
pub fn edit(
    tracks: &[CaptionTrack],
    command: CaptionEdit,
    min_duration: u64,
) -> Result<CaptionEditResult> {
    edit_exact(tracks, command, min_duration as i128 * UNIT)
}
/// Exact source-clock minimum, preserving fractional standard frame durations.
pub fn edit_exact(
    tracks: &[CaptionTrack],
    command: CaptionEdit,
    min_duration: i128,
) -> Result<CaptionEditResult> {
    validate(tracks)?;
    ensure!(min_duration > 0, "caption frame duration must be positive");
    let mut result = CaptionEditResult {
        tracks: tracks.to_vec(),
        applied: None,
        deleted: 0,
    };
    let tracks = &mut result.tracks;
    match command {
        CaptionEdit::Add {
            track,
            mut cue,
        } => {
            let track = tracks
                .iter_mut()
                .find(|candidate| candidate.id == track)
                .ok_or_else(|| anyhow::anyhow!("caption track not found"))?;
            ensure!(!track.locked, "caption track is locked");
            cue.validate()?;
            ensure!(
                !track
                    .clips
                    .iter()
                    .any(
                        |existing| existing.timeline_start_tick <= cue.timeline_start_tick
                            && cue.timeline_start_tick < existing.timeline_end_tick
                    ),
                "a caption already covers this time"
            );
            if let Some(next) = track
                .clips
                .iter()
                .find(|existing| existing.timeline_start_tick > cue.timeline_start_tick)
            {
                cue.timeline_end_tick = cue.timeline_end_tick.min(next.timeline_start_tick);
            }
            ensure!(
                cue.timeline_end_tick.exact_ticks() - cue.timeline_start_tick.exact_ticks() >= min_duration,
                "caption is shorter than a frame"
            );
            track.clips.push(cue);
            track.clips.sort_by_key(|cue| cue.timeline_start_tick);
        }
        CaptionEdit::Split { id, time, new_id } => {
            time.validate_timeline_point().map_err(anyhow::Error::msg)?;
            let (ti, ci) = locate(tracks, &id)?;
            let cue = &mut tracks[ti].clips[ci];
            ensure!(
                time >= cue.timeline_start_tick
                    && time <= cue.timeline_end_tick
                    && time.exact_ticks() - cue.timeline_start_tick.exact_ticks() >= min_duration
                    && cue.timeline_end_tick.exact_ticks() - time.exact_ticks() >= min_duration,
                "caption split is shorter than a frame"
            );
            let mut right = cue.clone();
            right.id = new_id.clone();
            right.timeline_start_tick = time;
            right.cue_id = None;
            cue.timeline_end_tick = time;
            tracks[ti].clips.insert(ci + 1, right);
        }
        CaptionEdit::Merge { ids } => {
            ensure!(ids.len() >= 2, "select at least two captions");
            let (ti, _) = locate(tracks, &ids[0])?;
            let mut indices = vec![];
            for id in &ids {
                let (owner, ci) = locate(tracks, id)?;
                ensure!(owner == ti, "captions must be on one track");
                indices.push(ci)
            }
            indices.sort_unstable();
            indices.dedup();
            ensure!(
                indices.len() >= 2 && indices.windows(2).all(|pair| pair[1] == pair[0] + 1),
                "captions must be consecutive"
            );
            let first = indices[0];
            let last = *indices.last().unwrap();
            let end = tracks[ti].clips[last].timeline_end_tick;
            let text = indices
                .iter()
                .map(|&i| tracks[ti].clips[i].text.trim())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            let cue = &mut tracks[ti].clips[first];
            cue.timeline_end_tick = end;
            cue.text = text;
            tracks[ti].clips.drain(first + 1..=last);
        }
        CaptionEdit::Text { id, text, speaker } => {
            ensure!(text.is_some() || speaker.is_some(), "need text or speaker");
            let (ti, ci) = locate(tracks, &id)?;
            let cue = &mut tracks[ti].clips[ci];
            if let Some(text) = text {
                cue.text = text.replace("\r\n", "\n")
            }
            if let Some(speaker) = speaker {
                cue.speaker = speaker
                    .map(|speaker| speaker.trim().to_string())
                    .filter(|speaker| !speaker.is_empty())
            }
        }
        CaptionEdit::Times { id, start, end } => {
            start.validate_timeline_point().map_err(anyhow::Error::msg)?;
            end.validate_timeline_point().map_err(anyhow::Error::msg)?;
            let (ti, ci) = locate(tracks, &id)?;
            ensure!(
                end > start && end.exact_ticks() - start.exact_ticks() >= min_duration,
                "caption is shorter than a frame"
            );
            ensure!(
                !tracks[ti].clips.iter().any(|cue| cue.id != id
                    && cue.timeline_start_tick < end
                    && start < cue.timeline_end_tick),
                "captions would overlap"
            );
            let cue = &mut tracks[ti].clips[ci];
            cue.timeline_start_tick = start;
            cue.timeline_end_tick = end;
            tracks[ti].clips.sort_by_key(|cue| cue.timeline_start_tick);
        }
        CaptionEdit::Trim { id, in_edge, delta } => {
            delta.validate_wire().map_err(anyhow::Error::msg)?;
            let (ti, ci) = locate(tracks, &id)?;
            let track = &mut tracks[ti];
            let prev = ci
                .checked_sub(1)
                .map(|i| track.clips[i].timeline_end_tick)
                .unwrap_or(CaptionTime::ZERO).exact_ticks();
            let next = track
                .clips
                .get(ci + 1)
                .map(|cue| cue.timeline_start_tick)
                .unwrap_or(CaptionTime::Microseconds(9_007_199_254_740_991)).exact_ticks();
            let cue = &mut track.clips[ci];
            let start = cue.timeline_start_tick.exact_ticks();
            let end = cue.timeline_end_tick.exact_ticks();
            let duration = end - start;
            ensure!(
                duration >= min_duration as i128,
                "caption is shorter than a frame"
            );
            let applied = if in_edge {
                delta.exact_ticks().clamp(prev - start, duration - min_duration as i128)
            } else {
                delta.exact_ticks().clamp(min_duration as i128 - duration, next - end)
            };
            ensure!(applied != 0, "nothing to trim");
            if in_edge {
                cue.timeline_start_tick = cue.timeline_start_tick.with_exact_ticks(start + applied).map_err(anyhow::Error::msg)?
            } else {
                cue.timeline_end_tick = cue.timeline_end_tick.with_exact_ticks(end + applied).map_err(anyhow::Error::msg)?
            };
            result.applied = Some(CaptionDelta::from_exact_ticks(applied).map_err(anyhow::Error::msg)?);
        }
        CaptionEdit::Move { ids, delta } => {
            delta.validate_wire().map_err(anyhow::Error::msg)?;
            ensure!(!ids.is_empty(), "select captions to move");
            let selected: BTreeSet<_> = ids.iter().collect();
            let mut lo = i64::MIN as i128*UNIT;
            let mut hi = i64::MAX as i128*UNIT;
            for id in &ids {
                let (ti, ci) = locate(tracks, id)?;
                let cue = &tracks[ti].clips[ci];
                let start = cue.timeline_start_tick.exact_ticks();
                let end = cue.timeline_end_tick.exact_ticks();
                lo = lo.max(-start);
                for other in tracks[ti]
                    .clips
                    .iter()
                    .filter(|cue| !selected.contains(&cue.id))
                {
                    if other.timeline_end_tick <= cue.timeline_start_tick {
                        lo = lo.max(other.timeline_end_tick.exact_ticks() - start)
                    } else if other.timeline_start_tick >= cue.timeline_end_tick {
                        hi = hi.min(other.timeline_start_tick.exact_ticks() - end)
                    } else {
                        bail!("captions overlap")
                    }
                }
            }
            let applied = delta.exact_ticks().clamp(lo.min(0), hi.max(0));
            ensure!(applied != 0, "nothing to move");
            for track in tracks.iter_mut() {
                for cue in &mut track.clips {
                    if selected.contains(&cue.id) {
                        cue.timeline_start_tick =
                            cue.timeline_start_tick.with_exact_ticks(cue.timeline_start_tick.exact_ticks() + applied).map_err(anyhow::Error::msg)?;
                        cue.timeline_end_tick = cue.timeline_end_tick.with_exact_ticks(cue.timeline_end_tick.exact_ticks() + applied).map_err(anyhow::Error::msg)?;
                    }
                }
                track.clips.sort_by_key(|cue| cue.timeline_start_tick);
            }
            result.applied = Some(CaptionDelta::from_exact_ticks(applied).map_err(anyhow::Error::msg)?);
        }
        CaptionEdit::Delete { ids, ripple } => {
            for id in &ids {
                locate(tracks, id)?;
            }
            let selected: BTreeSet<_> = ids.iter().collect();
            for track in tracks.iter_mut() {
                let mut removed = track
                    .clips
                    .iter()
                    .filter(|cue| selected.contains(&cue.id))
                    .map(|cue| (cue.timeline_start_tick, cue.timeline_end_tick))
                    .collect::<Vec<_>>();
                result.deleted += removed.len();
                track.clips.retain(|cue| !selected.contains(&cue.id));
                if ripple {
                    removed.sort_by_key(|range| std::cmp::Reverse(range.0));
                    for (start, end) in removed {
                        for cue in track
                            .clips
                            .iter_mut()
                            .filter(|cue| cue.timeline_start_tick >= end)
                        {
                            cue.timeline_start_tick = cue.timeline_start_tick.with_exact_ticks(cue.timeline_start_tick.exact_ticks()-(end.exact_ticks()-start.exact_ticks())).map_err(anyhow::Error::msg)?;
                            cue.timeline_end_tick = cue.timeline_end_tick.with_exact_ticks(cue.timeline_end_tick.exact_ticks()-(end.exact_ticks()-start.exact_ticks())).map_err(anyhow::Error::msg)?;
                        }
                    }
                }
            }
        }
    }
    validate(tracks)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use filmcraft_time::{FrameRate, Tick, TICKS_PER_SECOND};
    use serde_json::Value;
    fn id(value: &Value) -> CompositionClipId {
        CompositionClipId::parse(format!("caption-{}", value.as_u64().unwrap())).unwrap()
    }
    fn tracks(value: &Value) -> Vec<CaptionTrack> {
        value["tracks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|track| CaptionTrack {format:None,language:None,sync_lock:None,vtt_blocks:vec![], 
                id: TrackId::parse(format!("track-{}", track["id"].as_u64().unwrap())).unwrap(),
                name: track["name"].as_str().unwrap().into(),
                hidden: !track["enabled"].as_bool().unwrap(),
                locked: track["locked"].as_bool().unwrap(),
                style: serde_json::from_value(track["style"].clone()).unwrap(),
                clips: track["captions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|cue| CaptionCue {cue_id:None, 
                        id: id(&cue["id"]),
                        timeline_start_tick: (cue["start"].as_u64().unwrap()).into(),
                        timeline_end_tick: (cue["end"].as_u64().unwrap()).into(),
                        text: cue["text"].as_str().unwrap().into(),
                        speaker: cue["speaker"].as_str().map(str::to_string),
                        settings: cue["settings"].as_str().unwrap().into(),
                    })
                    .collect(),
            })
            .collect()
    }
    #[test]
    fn original_caption_edit_algorithms_preserve_source_tick_results_atomically() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-caption-edit.json"
        ))
        .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 12);
        for case in cases {
            let before = tracks(&case["before"]);
            let preserved = before.clone();
            let expected = tracks(&case["after"]);
            let params = &case["params"];
            let rate: FrameRate = serde_json::from_value(case["frameRate"].clone()).unwrap();
            let min = rate.frame_duration().0 as u64;
            let ids = || {
                params["captions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(id)
                    .collect()
            };
            let seconds = |key: &str| Tick::from_seconds_f64(params[key].as_f64().unwrap());
            let command = match case["name"].as_str().unwrap() {
                "split" => CaptionEdit::Split {
                    id: id(&params["caption"]),
                    time: params["time"].as_u64().unwrap().into(),
                    new_id: CompositionClipId::parse("caption-52").unwrap(),
                },
                "merge" => CaptionEdit::Merge { ids: ids() },
                "text" => CaptionEdit::Text {
                    id: id(&params["caption"]),
                    text: params["text"].as_str().map(str::to_string),
                    speaker: Some(params["speaker"].as_str().map(str::to_string)),
                },
                "times" => CaptionEdit::Times {
                    id: id(&params["caption"]),
                    start: (rate.snap_nearest(seconds("startSeconds")).0 as u64).into(),
                    end: (rate.snap_nearest(seconds("endSeconds")).0 as u64).into(),
                },
                "trim-out" | "trim-in" => CaptionEdit::Trim {
                    id: id(&params["caption"]),
                    in_edge: params["edge"] == "in",
                    delta: CaptionDelta::Microseconds(params["deltaFrames"].as_i64().unwrap() * min as i64),
                },
                "move-right" | "move-left" => CaptionEdit::Move {
                    ids: ids(),
                    delta: CaptionDelta::Microseconds(params["deltaFrames"].as_i64().unwrap() * min as i64),
                },
                "delete" | "ripple-delete" => CaptionEdit::Delete {
                    ids: ids(),
                    ripple: params["ripple"].as_bool().unwrap_or(false),
                },
                _ => {
                    let start = rate.snap(seconds("seconds")).0 as u64;
                    let duration = rate.snap_nearest(Tick(3 * TICKS_PER_SECOND)).0 as u64;
                    CaptionEdit::Add {
                        track: before[0].id.clone(),
                        cue: CaptionCue {cue_id:None, 
                            id: CompositionClipId::parse("caption-52").unwrap(),
                            timeline_start_tick: (start).into(),
                            timeline_end_tick: (start + duration).into(),
                            text: params["text"].as_str().unwrap().into(),
                            speaker: None,
                            settings: String::new(),
                        },
                    }
                }
            };
            let result = edit(&before, command, min);
            assert_eq!(before, preserved, "immutable input{}", case["name"]);
            if case["result"].get("error").is_some() {
                assert!(result.is_err(), "refusal{}", case["name"]);
                continue;
            }
            let result = result.unwrap();
            assert_eq!(result.tracks, expected, "source ticks{}", case["name"]);
            if let Some(applied) = case["result"]["ok"].get("applied") {
                assert_eq!(result.applied, Some(CaptionDelta::Microseconds(applied.as_i64().unwrap())));
            }
            if let Some(deleted) = case["result"]["ok"].get("deleted") {
                assert_eq!(result.deleted, deleted.as_u64().unwrap() as usize);
            }
            let mut locked = before.clone();
            locked[0].locked = true;
            assert!(edit(
                &locked,
                CaptionEdit::Delete {
                    ids: vec![locked[0].clips[0].id.clone()],
                    ripple: false
                },
                min
            )
            .is_err());
            assert_eq!(locked[0].clips, before[0].clips);
        }
    }
    #[test]
    fn caption_exact_clock_persistence_edit_clamp_and_ripple_do_not_round() {
        let track:CaptionTrack=serde_json::from_value(serde_json::json!({"id":"track","name":"Exact","clips":[
            {"id":"first","cueId":"file","timelineStartTick":"1","timelineEndTick":"254016000001","text":"First"},
            {"id":"next","timelineStartTick":"254016000002","timelineEndTick":"508032000000","text":"Next"}
        ]})).unwrap();
        let first=track.clips[0].id.clone();
        let changed=edit(&[track.clone()],CaptionEdit::Text{id:first.clone(),text:Some("Changed".into()),speaker:None},1).unwrap();
        assert_eq!(serde_json::to_value(&changed.tracks[0]).unwrap()["clips"][0]["timelineStartTick"],"1");
        let moved=edit(&[track.clone()],CaptionEdit::Move{ids:vec![first.clone()],delta:CaptionDelta::Microseconds(1)},1).unwrap();
        assert_eq!(moved.applied,Some(CaptionDelta::SourceTicks(1)));
        assert_eq!(moved.tracks[0].clips[0].timeline_start_tick,CaptionTime::SourceTicks(2));
        assert_eq!(moved.tracks[0].clips[0].timeline_end_tick,track.clips[1].timeline_start_tick);
        let deleted=edit(&[track.clone()],CaptionEdit::Delete{ids:vec![first.clone()],ripple:true},1).unwrap();
        assert_eq!(deleted.tracks[0].clips[0].timeline_start_tick,CaptionTime::SourceTicks(2));
        let mut locked=track.clone();locked.locked=true;
        assert!(edit(&[locked],CaptionEdit::Move{ids:vec![first],delta:CaptionDelta::Microseconds(1)},1).is_err());
        assert_eq!(serde_json::to_value(track).unwrap()["clips"][0]["timelineStartTick"],"1");
    }

    #[test]
    fn caption_exact_frame_minimum_admits_one_frame_and_refuses_one_tick_less() {
        for rate in [FrameRate::FPS_24, FrameRate::FPS_23_976, FrameRate::FPS_29_97] {
            let minimum=rate.frame_duration().0;
            let track:CaptionTrack=serde_json::from_value(serde_json::json!({"id":"track","name":"Exact","clips":[]})).unwrap();
            let cue:CaptionCue=serde_json::from_value(serde_json::json!({"id":"cue","timelineStartTick":"0","timelineEndTick":minimum.to_string(),"text":"One frame"})).unwrap();
            let command=CaptionEdit::Add{track:track.id.clone(),cue:cue.clone()};
            let admitted=edit_exact(&[track.clone()],command,minimum as i128).unwrap();
            assert_eq!(admitted.tracks[0].clips[0].timeline_end_tick.exact_ticks(),minimum as i128);
            let mut shorter=cue;shorter.timeline_end_tick=CaptionTime::SourceTicks(minimum-1);
            assert!(edit_exact(&[track.clone()],CaptionEdit::Add{track:track.id.clone(),cue:shorter},minimum as i128).is_err());
            assert!(track.clips.is_empty());
        }
    }

    #[test]
    fn caption_exact_commands_keep_one_tick_positions_and_deltas() {
        let track:CaptionTrack=serde_json::from_value(serde_json::json!({"id":"track","name":"Exact","clips":[{"id":"cue","timelineStartTick":"1","timelineEndTick":"100001","text":"Exact"}]})).unwrap();
        let id=track.clips[0].id.clone();
        let moved=edit_exact(&[track.clone()],CaptionEdit::Move{ids:vec![id.clone()],delta:CaptionDelta::SourceTicks(1)},1).unwrap();
        assert_eq!(moved.tracks[0].clips[0].timeline_start_tick.exact_ticks(),2);
        assert_eq!(moved.applied,Some(CaptionDelta::SourceTicks(1)));
        let split=edit_exact(&[track.clone()],CaptionEdit::Split{id:id.clone(),time:CaptionTime::SourceTicks(50001),new_id:CompositionClipId::parse("right").unwrap()},1).unwrap();
        assert_eq!(split.tracks[0].clips[0].timeline_end_tick.exact_ticks(),50001);
        assert_eq!(split.tracks[0].clips[1].timeline_start_tick.exact_ticks(),50001);
        let times=edit_exact(&[track.clone()],CaptionEdit::Times{id:id.clone(),start:CaptionTime::SourceTicks(3),end:CaptionTime::SourceTicks(100003)},1).unwrap();
        assert_eq!(times.tracks[0].clips[0].timeline_start_tick.exact_ticks(),3);
        let trimmed=edit_exact(&[track.clone()],CaptionEdit::Trim{id,in_edge:true,delta:CaptionDelta::SourceTicks(1)},1).unwrap();
        assert_eq!(trimmed.tracks[0].clips[0].timeline_start_tick.exact_ticks(),2);
        assert_eq!(track.clips[0].timeline_start_tick.exact_ticks(),1);
    }

    #[test]
    fn caption_frame_minimum_matches_independently_executed_original_transactions() {
        let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-caption-frame-minimum.json")).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let track:CaptionTrack=serde_json::from_value(serde_json::json!({"id":"track","name":"Exact","clips":[]})).unwrap();
            let cue:CaptionCue=serde_json::from_value(serde_json::json!({"id":"cue","timelineStartTick":"0","timelineEndTick":case["duration"],"text":"Frame"})).unwrap();
            let minimum=case["minimum"].as_str().unwrap().parse::<i128>().unwrap();
            let result=edit_exact(&[track.clone()],CaptionEdit::Add{track:track.id.clone(),cue},minimum);
            assert_eq!(result.is_ok(),case["success"].as_bool().unwrap(),"{case}");
            if let Ok(result)=result {
                assert_eq!(result.tracks[0].clips[0].timeline_end_tick.exact_ticks(),minimum);
            }
            assert!(track.clips.is_empty());
        }
    }

}
