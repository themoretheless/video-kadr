//! Original-source mask tracking. MIT attribution: docs/licenses/filmcraft-MIT.txt.
pub use super::filmcraft_source_masks::tracking_path;
use super::{
    filmcraft_parameter_edit::{self, Address, Command},
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_sequence_source::SourceTrackItem,
    group_surface::GroupSurface,
    motion_tracker::{self, Gray, Prepared, TrackMethod},
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Forward,
    Backward,
}
pub struct Plan {
    pub from: i64,
    pub to: Option<i64>,
    pub source: u64,
    pub generated: bool,
    pub sequence_rate: Option<filmcraft_time::FrameRate>,
    pub width: u32,
    pub height: u32,
    pub method: TrackMethod,
    pub more: bool,
}
impl Plan {
    pub fn generator_scale(&self) -> f32 {
        (960. / self.width.max(1) as f64).min(1.) as f32
    }
}
pub fn plan(
    document: &SourceProjectDocument,
    address: &Address,
    timeline: i64,
    cursor: Option<i64>,
    direction: Direction,
) -> Result<Plan> {
    ensure!(
        address.mask_index.is_some() && address.parameter == "path",
        "tracking needs a mask path address"
    );
    let (_, locked) = filmcraft_parameter_edit::parameter_path(document, address)?;
    ensure!(!locked, "source track is locked");
    let (path, _) = filmcraft_parameter_edit::location(
        document,
        address.sequence_id.parse()?,
        address.clip_id.parse()?,
    )?;
    let raw = document.envelope.pointer(&path).unwrap();
    let clip = SourceTrackItem::read(raw)?;
    let start = raw["start"].as_i64().context("invalid clip start")?;
    let duration = raw["duration"].as_i64().context("invalid clip duration")?;
    ensure!(duration > 0, "empty tracking clip");
    let end = start
        .checked_add(duration)
        .and_then(|v| v.checked_sub(1))
        .context("clip range overflow")?;
    let (a, b) = (clip.clocks(start)?.frame, clip.clocks(end)?.frame);
    let (lo, hi) = (a.min(b), a.max(b));
    let from = cursor.unwrap_or(clip.clocks(timeline.clamp(start, end))?.frame);
    ensure!(
        from >= lo && from <= hi,
        "tracking cursor outside clip source range"
    );
    let index = ProjectIndex::new(document)?;
    let item = index
        .item(clip.source_id)
        .context("tracking source missing")?;
    // Original Item::frame_rate: a Subclip uses the default 24000/1001 rate.
    let media = item.pointer("/kind/Media");
    let rate = media.and_then(|m| {
        m.pointer("/interpret/frame_rate")
            .filter(|v| v.is_object())
            .or_else(|| m.pointer("/info/video/frame_rate"))
    });
    let (num, den) = rate
        .map(|r| {
            (
                r["num"].as_i64().unwrap_or(24000),
                r["den"].as_i64().unwrap_or(1001),
            )
        })
        .unwrap_or((24000, 1001));
    let (num, den) = if num > 0 && den > 0 {
        (num, den)
    } else {
        (24000, 1001)
    };
    let fd = i64::try_from(254016000000i128 * den as i128 / num as i128)?.max(1);
    let delta = if matches!(direction, Direction::Backward) {
        -fd
    } else {
        fd
    };
    let next = from.checked_add(delta).filter(|v| *v >= lo && *v <= hi);
    let more = next
        .and_then(|v| v.checked_add(delta))
        .is_some_and(|v| v >= lo && v <= hi);
    let resolved = index
        .resolve_media(clip.source_id)
        .context("tracking source is not media")?;
    let media = resolved.media["media"]
        .as_object()
        .context("invalid tracking media reference")?;
    let generated = media.contains_key("Generator");
    ensure!(
        media.len() == 1 && (generated || media.contains_key("File")),
        "tracking requires file or generator media"
    );
    let (width, height) = index
        .source_size(clip.source_id)
        .context("tracking source dimensions missing")?;
    ensure!(
        width > 0 && height > 0 && width <= 3840 && height <= 2160,
        "tracking source dimensions exceed decode budget"
    );
    let mask = &raw["effects"][address.effect_index]["masks"][address.mask_index.unwrap()];
    let method = match mask["track_method"]
        .as_str()
        .unwrap_or("PositionScaleRotation")
    {
        "Position" => TrackMethod::Position,
        "PositionRotation" => TrackMethod::PositionRotation,
        "PositionScaleRotation" => TrackMethod::PositionScaleRotation,
        _ => anyhow::bail!("invalid tracking method"),
    };
    let sequence_rate = if !generated && resolved.media["info"]["kind"] == "ImageSequence" {
        let rate = resolved
            .media
            .pointer("/interpret/frame_rate")
            .filter(|v| !v.is_null())
            .or_else(|| resolved.media.pointer("/info/video/frame_rate"));
        Some(
            rate.map(|v| serde_json::from_value::<filmcraft_time::FrameRate>(v.clone()))
                .transpose()?
                .unwrap_or_default()
                .sane(),
        )
    } else {
        None
    };
    Ok(Plan {
        from,
        to: next,
        source: resolved.item_id,
        generated,
        sequence_rate,
        width,
        height,
        method,
        more,
    })
}
fn prepare(surface: &GroupSurface, resize: bool) -> Result<(Prepared, f64)> {
    ensure!(
        surface.width > 0
            && surface.height > 0
            && surface.width <= 3840
            && surface.height <= 2160
            && surface.rgba.len() == surface.width * surface.height * 4,
        "invalid tracking frame"
    );
    let scale = if resize {
        (960. / surface.width as f64).min(1.)
    } else {
        1.
    };
    let (w, h) = (
        (surface.width as f64 * scale).round().max(1.) as u32,
        (surface.height as f64 * scale).round().max(1.) as u32,
    );
    let frame = image::RgbaImage::from_raw(
        surface.width as u32,
        surface.height as u32,
        surface.rgba.clone(),
    )
    .context("invalid tracking pixels")?;
    let frame = if scale < 1. {
        image::imageops::resize(&frame, w, h, image::imageops::FilterType::Triangle)
    } else {
        frame
    };
    Ok((
        Prepared::new(Gray::from_rgba8(w as usize, h as usize, frame.as_raw())),
        w as f64 / surface.width as f64,
    ))
}
pub fn apply(
    document: &SourceProjectDocument,
    address: &Address,
    plan: &Plan,
    a: &GroupSurface,
    b: &GroupSurface,
) -> Result<Option<(SourceProjectDocument, usize)>> {
    let to = plan.to.context("tracking reached clip boundary")?;
    if a.width != b.width || a.height != b.height {
        if plan.sequence_rate.is_some() {
            return Ok(None);
        }
        anyhow::bail!("tracking frame geometry changed");
    }
    let (path, locked) = filmcraft_parameter_edit::parameter_path(document, address)?;
    ensure!(!locked, "source track is locked");
    let (parameter, region) = tracking_path(document.envelope.pointer(&path).unwrap(), plan.from)?;
    if region.len() < 3 {
        return Ok(None);
    }
    // Original ImageSequenceSource returns full decoded frames without scaling.
    let resize = !plan.generated && plan.sequence_rate.is_none();
    let (prev, k0) = prepare(a, resize)?;
    let (next, k1) = prepare(b, resize)?;
    let source_scale = a.width as f64 / plan.width as f64;
    if plan.generated {
        // Original GeneratorSource renders in quantized 1/64 scale buckets.
        // Rounded dimensions need not retain exactly equal x/y aspect ratios;
        // tracking uses its original width-derived scale for both coordinates.
        let scale = (plan.generator_scale().clamp(1. / 64., 1.) * 64.).round() / 64.;
        let expected = (
            (plan.width as f32 * scale).round().max(2.) as usize,
            (plan.height as f32 * scale).round().max(2.) as usize,
        );
        ensure!(
            (a.width, a.height) == expected,
            "unexpected generator analysis dimensions"
        );
    } else if plan.sequence_rate.is_none() {
        ensure!(
            (a.height as f64 / plan.height as f64 - source_scale).abs() < 1e-6,
            "tracking frame aspect changed"
        );
    }
    let region = region
        .into_iter()
        .map(|p| p * (source_scale * k0))
        .collect::<Vec<_>>();
    let Some(step) = motion_tracker::track_step(&prev, &next, &region, plan.method) else {
        return Ok(None);
    };
    let transformed = super::filmcraft_mask_track_batch::scaled_transform(
        &parameter,
        step.transform,
        source_scale * k0,
        source_scale * k1,
    )?;
    let seeded = filmcraft_parameter_edit::edit(
        document,
        address,
        Command::Upsert {
            time: plan.from.to_string(),
            value: parameter,
        },
        0,
    )?;
    let changed = filmcraft_parameter_edit::edit(
        &seeded,
        address,
        Command::Upsert {
            time: to.to_string(),
            value: transformed,
        },
        0,
    )?;
    Ok(Some((changed, step.inliers)))
}
