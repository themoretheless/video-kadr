//! Source monitor inspection adapted from Filmcraft clip_ops.rs / commands.rs
//! (MIT), pinned 5231852443363f001c3f6b396dd9b1e6461ae2be.
use super::{
    filmcraft_project_file::SourceProjectDocument, filmcraft_project_index::ProjectIndex,
    filmcraft_sequence_source::SourceSequence,
};
use anyhow::{ensure, Context, Result};
use filmcraft_time::{FrameRate, Tick};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    document: Value,
    item: Option<Value>,
    #[serde(default)]
    context: SessionContext,
}
#[derive(Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct SessionContext {
    source_item: Option<Value>,
    source_playhead: Value,
    still_image_duration: f64,
    still_image_unit: String,
}
impl Default for SessionContext {
    fn default() -> Self {
        Self {
            source_item: None,
            source_playhead: json!(0),
            still_image_duration: 5.,
            still_image_unit: "seconds".into(),
        }
    }
}
fn tick(v: &Value) -> Result<i64> {
    if let Some(s) = v.as_str() {
        s.parse().context("invalid source monitor tick")
    } else {
        v.as_i64().context("invalid source monitor tick")
    }
}
fn rate(v: &Value) -> Result<FrameRate> {
    let rate: FrameRate = if v.is_null() {
        FrameRate::default()
    } else {
        serde_json::from_value(v.clone())?
    };
    ensure!(
        rate.num > 0 && rate.den > 0 && rate.num <= u32::MAX as i64 && rate.den <= u32::MAX as i64,
        "invalid source monitor rate"
    );
    Ok(rate)
}
fn media_rate(media: &Value) -> Result<FrameRate> {
    rate(if !media["interpret"]["frame_rate"].is_null() {
        &media["interpret"]["frame_rate"]
    } else {
        &media["info"]["video"]["frame_rate"]
    })
}
fn markers(raw: &Value, range: Option<(i64, i64)>) -> Result<Value> {
    let mut markers = Vec::new();
    for m in raw.as_array().context("missing source markers")? {
        let start = tick(&m["start"])?;
        if range.is_some_and(|(a, b)| start < a || start >= b) {
            continue;
        }
        markers.push(json!({"id":m["id"],"start":start,"name":m["name"],"duration":m["duration"]}));
    }
    Ok(Value::Array(markers))
}
fn still(media: &Value) -> Result<bool> {
    Ok(media["info"]["kind"] == "Still" || tick(&media["info"]["duration"])? <= 0)
}
fn still_duration(context: &SessionContext, rate: FrameRate) -> Result<i64> {
    let value = context.still_image_duration;
    ensure!(value.is_finite(), "invalid still duration");
    let duration = if context.still_image_unit == "frames" {
        let frames = value.max(1.).round();
        let ticks = frames * 254_016_000_000. * rate.den as f64 / rate.num as f64;
        ensure!(
            ticks <= (i64::MAX / 4) as f64,
            "still duration exceeds source clock"
        );
        rate.tick_of(frames as i64)
    } else {
        ensure!(
            context.still_image_unit == "seconds",
            "invalid still duration unit"
        );
        ensure!(
            value.max(0.) * 254_016_000_000. <= (i64::MAX / 4) as f64,
            "still duration exceeds source clock"
        );
        rate.snap_nearest(Tick::from_seconds_f64(value.max(0.)))
    };
    Ok(duration.max(rate.frame_duration()).0)
}
pub fn inspect(params: Value) -> Result<Value> {
    let request: Request = serde_json::from_value(params)?;
    let item = super::filmcraft_project_inspect::id(request.item)?.or(
        super::filmcraft_project_inspect::id(request.context.source_item.clone())?,
    );
    let Some(item) = item else {
        return Ok(json!({"item":null}));
    };
    let bytes = match request.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let document = SourceProjectDocument::read(&bytes)?;
    let index = ProjectIndex::new(&document)?;
    let pi = index
        .item(item)
        .context("source.inspect: the source item is gone")?;
    let kind = pi["kind"].as_object().context("invalid source kind")?;
    let mut result = if let Some(media) = kind.get("Media") {
        let rate = media_rate(media)?;
        let end = if still(media)? {
            still_duration(&request.context, rate)?
        } else {
            tick(&media["info"]["duration"])?
        };
        json!({"item":item,"media":item,"start":0,"end":end,"fps":rate.as_f64(),"markIn":media["mark_in"],"markOut":media["mark_out"],"markers":markers(&media["markers"],None)?,"subclip":null})
    } else if let Some(seq) = kind.get("Sequence") {
        let sequence = SourceSequence::read(&index, item)?;
        let rate = rate(&seq["settings"]["frame_rate"])?;
        json!({"item":item,"media":item,"start":0,"end":sequence.duration()?,"fps":rate.as_f64(),"markIn":seq["mark_in"],"markOut":seq["mark_out"],"markers":markers(&seq["markers"],None)?,"subclip":null})
    } else if let Some(sub) = kind.get("Subclip") {
        let resolved = index
            .resolve_media(item)
            .context("source subclip media is gone or cyclic")?;
        let media = resolved.media;
        let rate = media_rate(media)?;
        let start = tick(&sub["range"]["start"])?;
        let end = start
            .checked_add(tick(&sub["range"]["duration"])?)
            .context("subclip end overflow")?;
        let restrict = sub
            .get("restrict_trims")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let full = if still(media)? {
            end
        } else {
            tick(&media["info"]["duration"])?.max(end)
        };
        let (lo, hi, mi, mo) = if restrict {
            (start, end, None, None)
        } else {
            (
                0,
                full,
                Some(start),
                Some(
                    end.checked_sub(rate.frame_duration().0)
                        .context("subclip out overflow")?,
                ),
            )
        };
        json!({"item":item,"media":resolved.item_id,"start":lo,"end":hi,"fps":rate.as_f64(),"markIn":mi,"markOut":mo,"markers":markers(&media["markers"],Some((start,end)))?,"subclip":{"start":start,"end":end,"restrictTrims":restrict}})
    } else {
        let value = kind
            .get("Graphic")
            .or_else(|| kind.get("AdjustmentLayer"))
            .context("unsupported source kind")?;
        let rate = rate(&value["rate"])?;
        let end = if kind.contains_key("Graphic") {
            3600 * 254_016_000_000
        } else {
            tick(&value["duration"])?
        };
        json!({"item":item,"media":item,"start":0,"end":end,"fps":rate.as_f64(),"markIn":null,"markOut":null,"markers":[],"subclip":null})
    };
    result["playhead"] = json!(tick(&request.context.source_playhead)?);
    super::filmcraft_project_inspect::exact_numbers(&mut result);
    Ok(result)
}
