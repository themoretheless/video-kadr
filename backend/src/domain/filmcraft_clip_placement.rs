//! Exact conversion of representable Filmcraft timing into native video placement.
//! Retain the SourceTrackItem alongside this projection: this does not convert
//! its effects, audio, links or other authoring data. Nonrepresentable timing is
//! reported, never rounded onto the native microsecond grid.
use super::{
    composition::{ClipPlacement, PlaybackMode, VideoClip},
    filmcraft_sequence_source::SourceTrackItem,
    video_effect_timing::EffectTiming,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportedVideoPlacement {
    pub placement: ClipPlacement,
    pub playback_mode: PlaybackMode,
    pub effect_timing: EffectTiming,
}
impl ImportedVideoPlacement {
    pub fn from_source(item: &SourceTrackItem<'_>) -> Result<Self> {
        let source = item.timing;
        ensure!(
            source.duration > 0,
            "source duration requires native legacy-item support"
        );
        let speed = source.speed.abs();
        ensure!(
            (0.05..=16.).contains(&speed),
            "source speed requires expanded native placement support"
        );
        let microseconds = |tick: i64| -> Result<u64> {
            ensure!(
                tick >= 0 && tick % 254016 == 0,
                "source timing requires the exact Filmcraft tick grid"
            );
            Ok(u64::try_from(tick / 254016)?)
        };
        let end = source
            .timeline_start
            .checked_add(source.duration)
            .context("source timeline overflow")?;
        let source_out = super::effect_frame_clock::EffectClockInput {
            reverse: false,
            frame_hold: None,
            hold_filters: false,
            ..source
        }
        .sample_source_item(end)
        .map_err(anyhow::Error::msg)?
        .frame;
        let placement = ClipPlacement {
            concat_source_eof: false,
timeline_start_tick: microseconds(source.timeline_start)?,
            source_in_tick: microseconds(source.source_in)?,
            source_out_tick: microseconds(source_out)?,
            speed,
            source_timeline_offset_tick: None, source_head_timeline_tick: None, concat_retime: None,
            timeline_duration_tick: Some(microseconds(source.duration)?),
            speed_ramp: None,
        };
        placement.timeline_end_tick()?;
        let playback_mode = if let Some(held) = source.frame_hold {
            let held = microseconds(held)?;
            ensure!(
                held >= placement.source_in_tick && held < placement.source_out_tick,
                "held frame requires native out-of-window hold support"
            );
            PlaybackMode::Freeze { source_tick: held }
        } else if source.reverse {
            PlaybackMode::Reverse
        } else {
            PlaybackMode::Forward
        };
        Ok(Self {
            placement,
            playback_mode,
            effect_timing: EffectTiming::FilmcraftSource {
                hold_filters: source.hold_filters,
                reverse_while_frozen: source.reverse,
            },
        })
    }
    /// Bind only timing, leaving separately converted native clip content intact.
    pub fn apply_to(&self, clip: &mut VideoClip) {
        clip.placement = self.placement.clone();
        clip.playback_mode = self.playback_mode;
        clip.effect_timing = Some(self.effect_timing);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    #[test]
    fn all_1176_original_clocks_survive_native_clip_placement_and_json() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-clip-placement.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 1176);
        for (i, c) in cases.iter().enumerate() {
            let source = SourceTrackItem::read(&c["item"]).unwrap();
            let converted = ImportedVideoPlacement::from_source(&source).unwrap();
            let restored: ImportedVideoPlacement =
                serde_json::from_value(serde_json::to_value(&converted).unwrap()).unwrap();
            assert_eq!(converted, restored);
            let mut clip:VideoClip=serde_json::from_value(json!({"id":"clip","sourceId":"media","placement":restored.placement,"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[]})).unwrap();
            restored.apply_to(&mut clip);
            let clip: VideoClip =
                serde_json::from_value(serde_json::to_value(clip).unwrap()).unwrap();
            let clocks =
                super::super::video_effect_timing::sample(&clip, c["at"].as_i64().unwrap() as i128)
                    .unwrap()
                    .unwrap();
            assert_eq!(
                [clocks.frame, clocks.parameters, clocks.generator],
                serde_json::from_value::<[i64; 3]>(c["clocks"].clone()).unwrap(),
                "native placement case {i}"
            );
        }
    }
    #[test]
    fn unrepresentable_timing_is_preserved_in_source_and_refused_without_rounding() {
        let baseline = json!({"id":1,"item":2,"start":0,"duration":254016000000i64,"source_in":0,"speed":1.,"enabled":true});
        for (field, value) in [
            ("start", json!(17)),
            ("source_in", json!(1)),
            ("duration", json!(0)),
            ("speed", json!(0.)),
            ("start", json!(-254016)),
        ] {
            let mut raw = baseline.clone();
            raw[field] = value;
            let item = SourceTrackItem::read(&raw).unwrap();
            assert!(ImportedVideoPlacement::from_source(&item).is_err());
            assert_eq!(item.raw, &raw);
        }
    }
}
