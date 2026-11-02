//! Source commands::time_p / export_tools::range_param (Filmcraft MIT,
//! 5231852443363f001c3f6b396dd9b1e6461ae2be; docs/licenses/filmcraft-MIT.txt).
use super::{
    filmcraft_export_range::SourceRange, filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
};
use anyhow::{ensure, Context, Result};
use filmcraft_time::{FrameRate, Tick};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandRange {
    pub source: Value,
    pub times: Value,
    pub context_sequence_id: Option<String>,
    pub playhead_time: Option<String>,
}
#[derive(Clone, Copy)]
pub struct Clock {
    pub rate: FrameRate,
    pub drop_frame: bool,
    pub playhead: Tick,
}
fn integer(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}
impl Clock {
    fn validate(self) -> Result<()> {
        super::filmcraft_export_preset_settings::ExportSettings {
            frame_rate: Some(self.rate),
            ..Default::default()
        }
        .validate()?;
        ensure!(
            (Tick::MIN.0..=Tick::MAX.0).contains(&self.playhead.0),
            "invalid source playhead"
        );
        Ok(())
    }
    fn frame_tick(self, frame: i64) -> Result<i64> {
        let ticks = i128::from(frame)
            .checked_mul(i128::from(filmcraft_time::TICKS_PER_SECOND))
            .and_then(|ticks| ticks.checked_mul(i128::from(self.rate.den)))
            .context("source frame clock overflow")?
            .div_euclid(i128::from(self.rate.num));
        let tick = i64::try_from(ticks).context("source frame tick overflow")?;
        ensure!(
            (Tick::MIN.0..=Tick::MAX.0).contains(&tick),
            "source frame exceeds tick bounds"
        );
        Ok(tick)
    }
    /// Source precedence: ticks, frame, seconds, timecode. Decimal strings are
    /// also accepted for exact ticks/frames over browser transports.
    pub fn time(self, params: &Value, prefix: &str) -> Result<Option<i64>> {
        self.validate()?;
        if let Some(tick) = integer(&params[format!("{prefix}Time")]) {
            return Ok(Some(tick));
        }
        if let Some(frame) = integer(&params[format!("{prefix}Frame")]) {
            return self.frame_tick(frame).map(Some);
        }
        if let Some(seconds) = params[format!("{prefix}Seconds")]
            .as_f64()
            .filter(|seconds| seconds.is_finite())
        {
            return Ok(Some(Tick::from_seconds_f64(seconds).0));
        }
        if let Some(text) = params[format!("{prefix}Timecode")].as_str() {
            if let Ok(frame) = filmcraft_time::parse_timecode(
                text,
                self.rate,
                self.drop_frame,
                self.rate.frame_at(self.playhead),
            ) {
                return self.frame_tick(frame).map(Some);
            }
        }
        Ok(None)
    }
}
pub fn resolve_with_clock(
    document: &SourceProjectDocument,
    sequence: u64,
    source: &Value,
    times: &Value,
    clock: Clock,
) -> Result<SourceRange> {
    if let Some(object) = source.as_object() {
        let mode = object
            .get("mode")
            .cloned()
            .unwrap_or_else(|| Value::String("custom".into()));
        return resolve_with_clock(document, sequence, &mode, source, clock);
    }
    let mode = if source.is_null() {
        if clock.time(times, "start")?.is_some() || clock.time(times, "end")?.is_some() {
            "custom".into()
        } else {
            "inout".into()
        }
    } else {
        source
            .as_str()
            .context("unexpected export range")?
            .to_ascii_lowercase()
            .replace(['-', '_', ' ', '/'], "")
    };
    if mode != "custom" {
        return super::filmcraft_quick_export_range::resolve(document, sequence, Some(&mode), None);
    }
    let start = clock
        .time(times, "start")?
        .context("custom range needs startTime/startFrame/startSeconds/startTimecode")?;
    let end = clock
        .time(times, "end")?
        .context("custom range needs endTime/endFrame/endSeconds/endTimecode")?;
    let range = SourceRange {
        start_ticks: start.to_string(),
        end_ticks: end.to_string(),
    };
    range.bounds()?;
    Ok(range)
}
impl CommandRange {
    pub fn resolve(&self, document: &SourceProjectDocument, sequence: u64) -> Result<SourceRange> {
        let context = self
            .context_sequence_id
            .as_deref()
            .map(str::parse::<u64>)
            .transpose()?
            .unwrap_or(sequence);
        let index = ProjectIndex::new(document)?;
        let raw = index
            .item(context)
            .and_then(|item| item.pointer("/kind/Sequence"))
            .context("invalid source time context sequence")?;
        let rate: FrameRate = serde_json::from_value(raw["settings"]["frame_rate"].clone())?;
        let playhead = self
            .playhead_time
            .as_deref()
            .map(str::parse::<i64>)
            .transpose()?
            .unwrap_or(0);
        let clock = Clock {
            rate,
            drop_frame: raw["settings"]["drop_frame"].as_bool().unwrap_or(false),
            playhead: Tick(playhead),
        };
        clock.validate()?;
        resolve_with_clock(document, sequence, &self.source, &self.times, clock)
    }
}
