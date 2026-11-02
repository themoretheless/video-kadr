//! Source export_tools::range_param and export::export_range defaults (MIT,
//! 5231852443363f001c3f6b396dd9b1e6461ae2be), using the exact native tick clock.
use super::{
    filmcraft_export_range::SourceRange, filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex, filmcraft_sequence_source::SourceSequence,
};
use anyhow::{Context, Result};
use filmcraft_time::{FrameRate, TimeRange};
use serde_json::Value;

fn optional_tick(value: &Value) -> Result<Option<i64>> {
    if value.is_null() {
        Ok(None)
    } else {
        Ok(Some(value.as_i64().context("invalid sequence mark")?))
    }
}

pub fn resolve(
    document: &SourceProjectDocument,
    sequence_id: u64,
    mode: Option<&str>,
    custom: Option<&SourceRange>,
) -> Result<SourceRange> {
    let index = ProjectIndex::new(document)?;
    let sequence = SourceSequence::read(&index, sequence_id)?;
    let rate: FrameRate = serde_json::from_value(sequence.raw["settings"]["frame_rate"].clone())?;
    super::filmcraft_export_preset_settings::ExportSettings {
        frame_rate: Some(rate),
        ..Default::default()
    }
    .validate()?;
    let fd = rate.frame_duration().0;
    let mode = mode
        .map(|mode| mode.to_ascii_lowercase().replace(['-', '_', ' ', '/'], ""))
        .unwrap_or_else(|| {
            if custom.is_some() {
                "custom".into()
            } else {
                "inout".into()
            }
        });
    let (start, end) = match mode.as_str() {
        "entire" | "entiresequence" | "sequence" | "all" => (0, sequence.duration()?.max(fd)),
        "inout" | "sequenceinout" => {
            let start = optional_tick(&sequence.raw["mark_in"])?.unwrap_or(0);
            let minimum = start.checked_add(fd).context("export In point overflow")?;
            let end = match optional_tick(&sequence.raw["mark_out"])? {
                Some(out) => out.checked_add(fd).context("export Out point overflow")?,
                None => sequence.duration()?,
            };
            (start, end.max(minimum))
        }
        "workarea" => {
            let range: TimeRange = serde_json::from_value(
                sequence
                    .raw
                    .get("work_area")
                    .filter(|value| !value.is_null())
                    .context("sequence has no work area")?
                    .clone(),
            )?;
            (
                range.start.0,
                range
                    .start
                    .0
                    .checked_add(range.duration.0)
                    .context("work area end overflow")?,
            )
        }
        "custom" => {
            let range = custom.context("custom range needs sourceRange")?;
            range.bounds()?
        }
        _ => anyhow::bail!("unknown quick export range"),
    };
    let range = SourceRange {
        start_ticks: start.to_string(),
        end_ticks: end.to_string(),
    };
    range.bounds()?;
    Ok(range)
}
