//! Exact source sequence/track/clip views for project conversion.
//! Opaque fields remain borrowed from the preserved authoring document. These
//! views do not make unsupported native rendering or editor admission implicit.
use super::{
    effect_frame_clock::{EffectClockInput, EffectFrameClock},
    filmcraft_project_index::ProjectIndex,
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;

pub struct SourceSequence<'a> {
    pub id: u64,
    pub raw: &'a Value,
    pub video_tracks: Vec<SourceTrack<'a>>,
    pub audio_tracks: Vec<SourceTrack<'a>>,
    pub submix_tracks: Vec<SourceTrack<'a>>,
    pub caption_tracks: Vec<SourceCaptionTrack<'a>>,
}
pub struct SourceTrack<'a> {
    pub id: u64,
    pub raw: &'a Value,
    pub items: Vec<SourceTrackItem<'a>>,
}
pub struct SourceTrackItem<'a> {
    pub id: u64,
    pub source_id: u64,
    pub enabled: bool,
    pub timing: EffectClockInput,
    pub raw: &'a Value,
}
pub struct SourceCaption<'a> {
    pub id: u64,
    pub start: i64,
    pub duration: i64,
    pub text: &'a str,
    pub raw: &'a Value,
}
pub struct SourceCaptionTrack<'a> {
    pub id: u64,
    pub raw: &'a Value,
    pub captions: Vec<SourceCaption<'a>>,
}
impl SourceCaption<'_> {
    pub fn end(&self) -> Result<i64> {
        self.start
            .checked_add(self.duration)
            .context("source caption end overflow")
    }
}
impl<'a> SourceCaptionTrack<'a> {
    fn read(raw: &'a Value) -> Result<Self> {
        let id = raw
            .get("id")
            .and_then(Value::as_u64)
            .context("invalid caption track ID")?;
        let captions = raw
            .get("captions")
            .and_then(Value::as_array)
            .context("missing source captions")?
            .iter()
            .map(|raw| {
                let caption = SourceCaption {
                    id: raw
                        .get("id")
                        .and_then(Value::as_u64)
                        .context("invalid caption ID")?,
                    start: raw
                        .get("start")
                        .and_then(Value::as_i64)
                        .context("invalid caption start")?,
                    duration: raw
                        .get("duration")
                        .and_then(Value::as_i64)
                        .context("invalid caption duration")?,
                    text: raw
                        .get("text")
                        .and_then(Value::as_str)
                        .context("invalid caption text")?,
                    raw,
                };
                caption.end()?;
                Ok(caption)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { id, raw, captions })
    }
    pub fn end(&self) -> Result<i64> {
        Ok(self
            .captions
            .iter()
            .map(SourceCaption::end)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .max()
            .unwrap_or(0))
    }
    pub fn caption_at(&self, timeline: i64) -> Option<&SourceCaption<'a>> {
        let index = self
            .captions
            .partition_point(|caption| caption.start <= timeline);
        index
            .checked_sub(1)
            .map(|i| &self.captions[i])
            .filter(|caption| caption.end().is_ok_and(|end| timeline < end))
    }
}
#[derive(Deserialize)]
struct Timing {
    id: u64,
    item: u64,
    start: i64,
    duration: i64,
    source_in: i64,
    speed: f64,
    enabled: bool,
    #[serde(default)]
    reverse: bool,
    #[serde(default)]
    frame_hold: Option<i64>,
    #[serde(default)]
    hold_filters: bool,
}
impl<'a> SourceTrackItem<'a> {
    pub fn read(raw: &'a Value) -> Result<Self> {
        let t: Timing = serde_json::from_value(raw.clone())?;
        let within = |v: i64| (i64::MIN / 4..=i64::MAX / 4).contains(&v);
        ensure!(
            within(t.start)
                && within(t.duration)
                && t.start.checked_add(t.duration).is_some_and(within),
            "source clip timeline bounds overflow"
        );
        ensure!(t.speed.is_finite(), "invalid source clip speed");
        Ok(Self {
            id: t.id,
            source_id: t.item,
            enabled: t.enabled,
            timing: EffectClockInput {
                timeline_start: t.start,
                source_in: t.source_in,
                duration: t.duration,
                speed: t.speed,
                reverse: t.reverse,
                frame_hold: t.frame_hold,
                hold_filters: t.hold_filters,
            },
            raw,
        })
    }
    pub fn end(&self) -> Option<i64> {
        self.timing.timeline_start.checked_add(self.timing.duration)
    }
    pub fn clocks(&self, timeline: i64) -> Result<EffectFrameClock> {
        self.timing
            .sample_source_item(timeline)
            .map_err(anyhow::Error::msg)
    }
}
impl<'a> SourceTrack<'a> {
    fn read(raw: &'a Value) -> Result<Self> {
        let id = raw
            .get("id")
            .and_then(Value::as_u64)
            .context("invalid source track ID")?;
        let items = raw
            .get("items")
            .and_then(Value::as_array)
            .context("invalid source track items")?
            .iter()
            .map(SourceTrackItem::read)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { id, raw, items })
    }
    /// Preserve original partition lookup, including legacy overlapping tracks.
    pub fn item_at(&self, timeline: i64) -> Option<&SourceTrackItem<'a>> {
        let index = self
            .items
            .partition_point(|item| item.timing.timeline_start <= timeline);
        index
            .checked_sub(1)
            .map(|i| &self.items[i])
            .filter(|item| item.end().is_some_and(|end| timeline < end))
    }
}
/// Filmcraft channel labels; adaptive interpretation remains with the audio engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Deserialize)]
pub enum SourceAudioChannels {
    Mono,
    Stereo,
    Surround51,
    Adaptive,
}

impl<'a> SourceSequence<'a> {
    pub fn strip_ids(&self) -> Vec<u64> {
        self.audio_tracks
            .iter()
            .chain(&self.submix_tracks)
            .map(|track| track.id)
            .chain(std::iter::once(u64::MAX))
            .collect()
    }
    pub fn mix_track(&self, id: u64) -> Option<&SourceTrack<'a>> {
        self.audio_tracks
            .iter()
            .chain(&self.submix_tracks)
            .find(|track| track.id == id)
    }
    pub fn output_channels(&self, id: u64) -> Result<SourceAudioChannels> {
        let master = || {
            serde_json::from_value(
                self.raw
                    .pointer("/settings/audio_master")
                    .context("missing master channel layout")?
                    .clone(),
            )
            .map_err(Into::into)
        };
        if id == u64::MAX {
            return master();
        }
        let output = self
            .mix_track(id)
            .and_then(|track| track.raw.pointer("/mixer/output"));
        let target = match output {
            None | Some(Value::Null) => None,
            Some(value) => Some(value.as_u64().context("invalid source mixer output ID")?),
        };
        match target.and_then(|id| self.submix_tracks.iter().find(|track| track.id == id)) {
            Some(track) => Ok(serde_json::from_value(
                track
                    .raw
                    .get("channels")
                    .context("missing submix channel layout")?
                    .clone(),
            )?),
            None => master(),
        }
    }
    pub fn pans_51(&self, id: u64) -> Result<bool> {
        Ok(id != u64::MAX && self.output_channels(id)? == SourceAudioChannels::Surround51)
    }

    pub fn duration(&self) -> Result<i64> {
        let media = self
            .video_tracks
            .iter()
            .chain(&self.audio_tracks)
            .map(|track| {
                track
                    .items
                    .iter()
                    .map(|item| item.end().context("source item end overflow"))
                    .collect::<Result<Vec<_>>>()
                    .map(|ends| ends.into_iter().max().unwrap_or(0))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .max()
            .unwrap_or(0);
        let captions = self
            .caption_tracks
            .iter()
            .map(SourceCaptionTrack::end)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .max()
            .unwrap_or(0);
        Ok(media.max(captions))
    }

    pub fn read(index: &ProjectIndex<'a>, id: u64) -> Result<Self> {
        let raw = index
            .item(id)
            .and_then(|item| item.pointer("/kind/Sequence"))
            .context("missing source sequence")?;
        let tracks = |key: &str| -> Result<Vec<SourceTrack<'a>>> {
            if key == "submix_tracks" && raw.get(key).is_none() {
                return Ok(Vec::new());
            }
            raw.get(key)
                .and_then(Value::as_array)
                .context("missing source sequence tracks")?
                .iter()
                .map(SourceTrack::read)
                .collect()
        };
        Ok(Self {
            id,
            raw,
            video_tracks: tracks("video_tracks")?,
            audio_tracks: tracks("audio_tracks")?,
            submix_tracks: tracks("submix_tracks")?,
            caption_tracks: match raw.get("caption_tracks") {
                None => &[][..],
                Some(value) => value
                    .as_array()
                    .context("invalid caption tracks")?
                    .as_slice(),
            }
            .iter()
            .map(SourceCaptionTrack::read)
            .collect::<Result<Vec<_>>>()?,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::super::filmcraft_project_file::SourceProjectDocument;
    use super::*;
    #[test]
    fn original_nonempty_submix_channels_and_strip_order_match() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-sequence-routing.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 96);
        for case in cases {
            let document =
                SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap())
                    .unwrap();
            let index = ProjectIndex::new(&document).unwrap();
            let sequence = SourceSequence::read(&index, case["id"].as_u64().unwrap()).unwrap();
            assert_eq!(sequence.submix_tracks.len(), 2);
            assert_eq!(
                sequence.strip_ids(),
                serde_json::from_value::<Vec<u64>>(case["strips"].clone()).unwrap()
            );
            for query in case["queries"].as_array().unwrap() {
                let id = query["id"].as_u64().unwrap();
                assert_eq!(
                    serde_json::to_value(sequence.output_channels(id).unwrap()).unwrap(),
                    query["channels"]
                );
                assert_eq!(
                    sequence.pans_51(id).unwrap(),
                    query["pans51"].as_bool().unwrap()
                );
            }
            assert_eq!(
                serde_json::from_slice::<Value>(&document.encode_preserved().unwrap()).unwrap(),
                case["document"]
            );
        }
    }

    #[test]
    fn original_caption_lookup_and_sequence_duration_preserve_source() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-sequence-caption.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 48);
        for case in cases {
            let document =
                SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap())
                    .unwrap();
            let index = ProjectIndex::new(&document).unwrap();
            let sequence = SourceSequence::read(&index, case["id"].as_u64().unwrap()).unwrap();
            assert_eq!(
                sequence.duration().unwrap(),
                case["duration"].as_i64().unwrap()
            );
            for (at, expected) in case["at"]
                .as_array()
                .unwrap()
                .iter()
                .zip(case["lookup"].as_array().unwrap())
            {
                assert_eq!(
                    sequence.caption_tracks[0]
                        .caption_at(at.as_i64().unwrap())
                        .map(|c| c.id),
                    expected.as_u64()
                );
            }
            assert_eq!(
                serde_json::from_slice::<Value>(&document.encode_preserved().unwrap()).unwrap(),
                case["document"]
            );
            let mut legacy = case["document"].clone();
            let raw = &mut legacy["project"]["items"][case["id"].as_u64().unwrap().to_string()]
                ["kind"]["Sequence"];
            raw.as_object_mut().unwrap().remove("caption_tracks");
            raw.as_object_mut().unwrap().remove("submix_tracks");
            let document =
                SourceProjectDocument::read(&serde_json::to_vec(&legacy).unwrap()).unwrap();
            let index = ProjectIndex::new(&document).unwrap();
            let sequence = SourceSequence::read(&index, case["id"].as_u64().unwrap()).unwrap();
            assert!(sequence.caption_tracks.is_empty() && sequence.submix_tracks.is_empty());
        }
    }

    #[test]
    fn original_held_clocks_do_not_evaluate_unused_moving_time() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-held-source-clock.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 36);
        for c in cases {
            let item = SourceTrackItem::read(&c["item"]).unwrap();
            let clocks = item.clocks(c["at"].as_i64().unwrap()).unwrap();
            assert_eq!(
                [clocks.frame, clocks.parameters, clocks.generator],
                serde_json::from_value::<[i64; 3]>(c["clocks"].clone()).unwrap()
            );
        }
    }

    #[test]
    fn all_3528_original_preserved_sequence_item_clocks_match_without_microsecond_rounding() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-sequence-source.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let document =
            SourceProjectDocument::read(&serde_json::to_vec(&fixture["document"]).unwrap())
                .unwrap();
        let index = ProjectIndex::new(&document).unwrap();
        let sequence = SourceSequence::read(&index, fixture["sequence"].as_u64().unwrap()).unwrap();
        assert_eq!(sequence.video_tracks[0].items.len(), 504);
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 3528);
        for c in cases {
            let id = c["id"].as_u64().unwrap();
            let item = sequence.video_tracks[0]
                .items
                .iter()
                .find(|item| item.id == id)
                .unwrap();
            let at = c["at"].as_i64().unwrap();
            let clocks = item.clocks(at).unwrap();
            assert_eq!(
                [clocks.frame, clocks.parameters, clocks.generator],
                serde_json::from_value::<[i64; 3]>(c["clocks"].clone()).unwrap(),
                "clip {id} at {at}"
            );
            assert_eq!(
                item.raw,
                &sequence.raw["video_tracks"][0]["items"][c["index"].as_u64().unwrap() as usize]
            );
            assert!(index.item(item.source_id).is_some());
        }
        for c in fixture["lookup"].as_array().unwrap() {
            assert_eq!(
                serde_json::to_value(
                    sequence.video_tracks[0]
                        .item_at(c["at"].as_i64().unwrap())
                        .map(|item| item.id)
                )
                .unwrap(),
                c["id"]
            );
        }
        assert_eq!(
            SourceProjectDocument::read(&document.encode_preserved().unwrap())
                .unwrap()
                .envelope,
            document.envelope
        );
    }
}
