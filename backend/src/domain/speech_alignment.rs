//! Align source speech intervals with the authoritative native clip clock.
use super::{
    caption_timeline::CaptionTrack,
    composition::{ClipPlacement, CompositionTrack, PlaybackMode, SourceId},
    nested_composition::NestedCompositionProject,
    video_clock::source_tick,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpeechSegmentation {
    #[default]
    Phrase,
    Word,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpeechSegment {
    pub start_us: u64,
    pub end_us: u64,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Transcript {
    pub language: String,
    pub model_hash: String,
    pub source_start_us: u64,
    pub duration_us: u64,
    pub segments: Vec<SpeechSegment>,
    #[serde(default)]
    pub segmentation: SpeechSegmentation,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechClipPlan {
    pub source_id: SourceId,
    pub source_start_us: u64,
    pub source_end_us: u64,
    pub timeline_start_us: u64,
    pub timeline_end_us: u64,
}
fn selected_with_freeze(
    project: &NestedCompositionProject,
    composition_id: &str,
    clip_id: &str,
    allow_freeze: bool,
) -> Result<(SourceId, ClipPlacement, PlaybackMode, bool)> {
    project.dependency_order()?;
    let node = project
        .compositions
        .get(composition_id)
        .context("Таймлайн не найден")?;
    for track in &node.content.tracks {
        match track {
            CompositionTrack::Video { clips, .. } => {
                if let Some(clip) = clips.iter().find(|clip| clip.id.as_str() == clip_id) {
                    ensure!(
                        allow_freeze || !matches!(clip.playback_mode, PlaybackMode::Freeze { .. }),
                        "Для стоп-кадра выберите звуковой клип"
                    );
                    return Ok((
                        clip.source_id.clone(),
                        clip.placement.clone(),
                        clip.playback_mode,
                        clip.effect_timing.is_some(),
                    ));
                }
            }
            CompositionTrack::Audio { clips, .. } => {
                if let Some(clip) = clips.iter().find(|clip| clip.id.as_str() == clip_id) {
                    return Ok((
                        clip.source_id.clone(),
                        clip.placement.clone(),
                        if clip.reversed {
                            PlaybackMode::Reverse
                        } else {
                            PlaybackMode::Forward
                        },
                        false,
                    ));
                }
            }
            _ => {}
        }
    }
    anyhow::bail!("Выберите видео или звуковой клип")
}
fn selected(project:&NestedCompositionProject,composition_id:&str,clip_id:&str)->Result<(SourceId,ClipPlacement,PlaybackMode,bool)>{selected_with_freeze(project,composition_id,clip_id,false)}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct VoiceReferencePlan {pub source_id:SourceId,pub source_start_us:u64}
pub fn reference_plan(project:&NestedCompositionProject,composition_id:&str,clip_id:&str)->Result<VoiceReferencePlan>{
 let(source_id,placement,mode,precise)=selected_with_freeze(project,composition_id,clip_id,true)?;
 let source=project.compositions[composition_id].content.sources.get(&source_id).context("Источник не найден")?;ensure!(source.has_audio,"В источнике нет звука");
 let start=placement.timeline_start_tick;let end=placement.timeline_end_tick()?;ensure!(end>start,"Пустой клип");
 let first=sample(&placement,mode,precise,start)?.context("Нет исходного времени клипа")?;let last=sample(&placement,mode,precise,end-1)?.context("Нет исходного времени клипа")?;
 Ok(VoiceReferencePlan{source_id,source_start_us:first.min(last)})
}
pub fn plan(
    project: &NestedCompositionProject,
    composition_id: &str,
    clip_id: &str,
) -> Result<SpeechClipPlan> {
    let (source_id, placement, mode, precise) = selected(project, composition_id, clip_id)?;
    let source = project.compositions[composition_id]
        .content
        .sources
        .get(&source_id)
        .context("Источник не найден")?;
    ensure!(source.has_audio, "В источнике нет звука");
    let start = placement.timeline_start_tick;
    let end = placement.timeline_end_tick()?;
    let first = sample(&placement, mode, precise, start)?.context("Нет исходного времени клипа")?;
    let last =
        sample(&placement, mode, precise, end - 1)?.context("Нет исходного времени клипа")?;
    Ok(SpeechClipPlan {
        source_id,
        source_start_us: first.min(last),
        source_end_us: first
            .max(last)
            .checked_add(1)
            .context("Переполнение исходного времени")?,
        timeline_start_us: start,
        timeline_end_us: end,
    })
}
fn sample(
    placement: &ClipPlacement,
    mode: PlaybackMode,
    precise: bool,
    time: u64,
) -> Result<Option<u64>> {
    if precise {
        super::video_clock::transition_source_sample_tick(
            placement,
            mode,
            i128::from(time) * 254016,
        )?
        .map(|tick| u64::try_from(i128::from(tick) / 254016).map_err(Into::into))
        .transpose()
    } else {
        source_tick(placement, mode, time)
    }
}
fn boundary(
    placement: &ClipPlacement,
    mode: PlaybackMode,
    precise: bool,
    value: u64,
) -> Result<u64> {
    let mut lo = placement.timeline_start_tick;
    let mut hi = placement.timeline_end_tick()?;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let source =
            sample(placement, mode, precise, mid)?.context("Нет исходного времени клипа")?;
        let after = if mode == PlaybackMode::Reverse {
            source < value
        } else {
            source >= value
        };
        if after {
            hi = mid
        } else {
            lo = mid + 1
        }
    }
    Ok(lo)
}
pub fn align(
    project: &NestedCompositionProject,
    composition_id: &str,
    clip_id: &str,
    source_id: &str,
    transcript: &Transcript,
) -> Result<CaptionTrack> {
    let (source, placement, mode, precise) = selected(project, composition_id, clip_id)?;
    ensure!(
        source.as_str() == source_id,
        "Источник клипа изменился после распознавания"
    );
    ensure!(
        transcript.duration_us > 0
            && transcript.duration_us <= 600_000_000
            && transcript.segments.len() <= 512,
        "Некорректный диапазон речи"
    );
    ensure!(
        transcript.model_hash.len() == 64
            && transcript.model_hash.bytes().all(|b| b.is_ascii_hexdigit())
            && (2..=16).contains(&transcript.language.len())
            && transcript
                .language
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'-'),
        "Некорректные сведения о распознавании"
    );
    let limit = transcript
        .source_start_us
        .checked_add(transcript.duration_us)
        .context("Переполнение времени речи")?;
    ensure!(
        limit <= project.compositions[composition_id].content.sources[&source].duration_ticks,
        "Речь выходит за пределы источника"
    );
    let mut previous = 0;
    let mut cues = Vec::new();
    for (index, segment) in transcript.segments.iter().enumerate() {
        ensure!(
            segment.start_us >= previous
                && segment.start_us < segment.end_us
                && segment.end_us <= transcript.duration_us
                && !segment.text.trim().is_empty()
                && segment.text.len() <= 4096,
            "Некорректный фрагмент речи"
        );
        previous = segment.end_us;
        let a = transcript.source_start_us + segment.start_us;
        let b = transcript.source_start_us + segment.end_us;
        let (start, end) = if mode == PlaybackMode::Reverse {
            (
                boundary(&placement, mode, precise, b)?,
                boundary(&placement, mode, precise, a)?,
            )
        } else {
            (
                boundary(&placement, mode, precise, a)?,
                boundary(&placement, mode, precise, b)?,
            )
        };
        if start < end {
            cues.push(serde_json::json!({"id":format!("speech-aligned-{index}"),"timelineStartTick":start,"timelineEndTick":end,"text":segment.text}));
        }
    }
    cues.sort_by_key(|cue| cue["timelineStartTick"].as_u64().unwrap());
    let track: CaptionTrack = serde_json::from_value(
        serde_json::json!({"id":"speech-aligned","name":format!("Распознанная речь ({})",transcript.language),"format":"Subtitle","language":transcript.language,"syncLock":true,"hidden":false,"locked":false,"clips":cues}),
    )?;
    track.validate()?;
    Ok(track)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn transcript(start: u64, end: u64) -> Transcript {
        Transcript {
            segmentation: SpeechSegmentation::Phrase,
            language: "en".into(),
            model_hash: "a".repeat(64),
            source_start_us: 0,
            duration_us: 10_000_000,
            segments: vec![SpeechSegment {
                start_us: start,
                end_us: end,
                text: "spoken".into(),
            }],
        }
    }
    #[test]
    fn follows_native_source_clock_for_constant_ramp_reverse_trim_phase_and_retained_head() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/concat-merge-preview.json"
        ))
        .unwrap();
        let mut tested = 0;
        for family in ["nativeTrims", "nativeExtendedHeads"] {
            for case in fixture[family].as_array().unwrap() {
                let project: NestedCompositionProject =
                    serde_json::from_value(case["after"].clone()).unwrap();
                if project.compositions["new-timeline"]
                    .content
                    .tracks
                    .iter()
                    .any(|track| match track {
                        CompositionTrack::Video { clips, .. } => clips.iter().any(|clip| {
                            clip.id.as_str() == "auto-first"
                                && matches!(clip.playback_mode, PlaybackMode::Freeze { .. })
                        }),
                        _ => false,
                    })
                {
                    assert!(plan(&project, "new-timeline", "auto-first").is_err());
                    continue;
                }
                let (source, placement, mode, precise) =
                    selected(&project, "new-timeline", "auto-first").unwrap();
                let p = plan(&project, "new-timeline", "auto-first").unwrap();
                let span = p.source_end_us - p.source_start_us;
                if span < 6 {
                    continue;
                }
                let a = p.source_start_us + span / 3;
                let b = p.source_start_us + span * 2 / 3;
                let track = align(
                    &project,
                    "new-timeline",
                    "auto-first",
                    source.as_str(),
                    &transcript(a, b),
                )
                .unwrap();
                let mut times = vec![
                    placement.timeline_start_tick,
                    placement.timeline_end_tick().unwrap() - 1,
                ];
                for cue in &track.clips {
                    let a = cue.timeline_start_tick.exact_microseconds().unwrap();
                    let b = cue.timeline_end_tick.exact_microseconds().unwrap();
                    times.extend([a.saturating_sub(1), a, b - 1, b]);
                }
                for time in times {
                    if time < placement.timeline_start_tick
                        || time >= placement.timeline_end_tick().unwrap()
                    {
                        continue;
                    }
                    let physical = sample(&placement, mode, precise, time).unwrap().unwrap();
                    let active = track.clips.iter().any(|cue| {
                        cue.timeline_start_tick.exact_microseconds().unwrap() <= time
                            && time < cue.timeline_end_tick.exact_microseconds().unwrap()
                    });
                    assert_eq!(
                        active,
                        a <= physical && physical < b,
                        "{family}/{tested}/{time}"
                    );
                }
                tested += 1;
            }
        }
        assert!(tested >= 20);
    }
    #[test]
    fn constant_and_reverse_boundaries_match_hand_calculated_intervals_with_source_effect_clock() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/concat-merge-preview.json"
        ))
        .unwrap();
        for reverse in [false, true] {
            let mut value = fixture["nativeTrims"][0]["before"].clone();
            for track in value["compositions"]["new-timeline"]["content"]["tracks"]
                .as_array_mut()
                .unwrap()
            {
                if track["kind"] == "video" {
                    for clip in track["clips"].as_array_mut().unwrap() {
                        if clip["id"] == "auto-first" {
                            clip["placement"] = serde_json::json!({"timelineStartTick":2000000,"sourceInTick":1000000,"sourceOutTick":4000000,"speed":2});
                            clip["playbackMode"] =
                                serde_json::json!({"mode":if reverse{"reverse"}else{"forward"}});
                            clip["effectTiming"] = serde_json::json!({"mode":"filmcraft_source","holdFilters":false,"reverseWhileFrozen":false});
                        }
                    }
                }
            }
            let project: NestedCompositionProject = serde_json::from_value(value).unwrap();
            let t = Transcript {
                segmentation: SpeechSegmentation::Phrase,
                language: "en".into(),
                model_hash: "a".repeat(64),
                source_start_us: 1000000,
                duration_us: 2000000,
                segments: vec![
                    SpeechSegment {
                        start_us: 0,
                        end_us: 1000000,
                        text: "early".into(),
                    },
                    SpeechSegment {
                        start_us: 1000000,
                        end_us: 2000000,
                        text: "late".into(),
                    },
                ],
            };
            let track = align(&project, "new-timeline", "auto-first", "source", &t).unwrap();
            assert_eq!(track.clips.len(), 2);
            let expected = if reverse {
                vec![(2500000, 3000000, "late"), (3000000, 3500000, "early")]
            } else {
                vec![(2000000, 2500000, "early"), (2500000, 3000000, "late")]
            };
            for (cue, (start, end, text)) in track.clips.iter().zip(expected) {
                assert_eq!(cue.timeline_start_tick.exact_microseconds().unwrap(), start);
                assert_eq!(cue.timeline_end_tick.exact_microseconds().unwrap(), end);
                assert_eq!(cue.text, text);
            }
        }
    }
}
