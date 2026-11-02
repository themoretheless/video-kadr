//! Filmcraft source-space animated Bezier effect and opacity masks.
//! Adapted from pinned Filmcraft 5231852443363f001c3f6b396dd9b1e6461ae2be.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt. No I/O or process ownership.
use super::{
    filmcraft_effect_scalar as scalar,
    transition_kernel_pack::{
        budget::Reservation, context::RenderContext, geom::Vec2, image::Image,
    },
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Vertex {
    p: Vec2,
    #[serde(default)]
    t_in: Vec2,
    #[serde(default)]
    t_out: Vec2,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Path {
    vertices: Vec<Vertex>,
    #[serde(default = "yes")]
    closed: bool,
}
fn yes() -> bool {
    true
}
#[derive(Clone, Copy, Deserialize, PartialEq)]
enum MaskMode {
    None,
    Add,
    Subtract,
    Intersect,
    Lighten,
    Darken,
    Difference,
}
impl MaskMode {
    fn start(self) -> f32 {
        if matches!(self, Self::Subtract | Self::Intersect | Self::Darken) {
            1.
        } else {
            0.
        }
    }
    fn combine(self, a: f32, m: f32) -> f32 {
        match self {
            Self::None => a,
            Self::Add => a + m - a * m,
            Self::Subtract => a * (1. - m),
            Self::Intersect => a * m,
            Self::Lighten => a.max(m),
            Self::Darken => a.min(m),
            Self::Difference => a + m - 2. * a * m,
        }
    }
}
struct FlatMask {
    pts: Vec<[f32; 2]>,
    feather: f32,
    expansion: f32,
    opacity: f32,
    inverted: bool,
    mode: MaskMode,
}
pub(crate) struct Coverage {
    pub values: Vec<f32>,
    _reservation: Reservation,
}

fn read_path(value: &Value) -> Result<Path> {
    let object = value.as_object().context("invalid mask path tag")?;
    ensure!(object.len() == 1, "ambiguous mask path tag");
    let path: Path =
        serde_json::from_value(object.get("Path").context("missing mask Path")?.clone())?;
    ensure!(
        path.vertices.iter().all(|v| [v.p, v.t_in, v.t_out]
            .into_iter()
            .all(|p| p.x.is_finite() && p.y.is_finite())),
        "nonfinite mask path"
    );
    Ok(path)
}
fn sample_path(
    param: &Value,
    tick: i64,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Path> {
    checkpoint()?;
    let baseline = read_path(param.get("value").context("missing mask path value")?)?;
    // Validate source key order, easing, influence and unknown fields through the
    // already-qualified scalar importer. A segment-local unit curve gives exactly
    // the source easing without an index subtraction or coordinate rounding.
    let mut normalized = param.clone();
    normalized["value"] = json!({"Float":0.});
    let keys = param
        .get("keyframes")
        .map(|v| v.as_array().context("invalid mask path keys"))
        .transpose()?
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut paths = Vec::with_capacity(keys.len());
    for (i, key) in keys.iter().enumerate() {
        checkpoint()?;
        paths.push(read_path(
            key.get("value").context("missing mask key path")?,
        )?);
        normalized["keyframes"][i]["value"] = json!({"Float":0.});
    }
    ensure!(keys.len() <= 4096, "source mask key budget exceeded");
    let mut previous = None;
    for key in keys {
        let time = key["time"].as_i64().context("invalid mask key time")?;
        ensure!(
            previous.is_none_or(|old| old < time),
            "mask keys must be strictly ordered"
        );
        previous = Some(time);
        ensure!(
            [
                "Linear",
                "Hold",
                "Bezier",
                "AutoBezier",
                "ContinuousBezier",
                "EaseIn",
                "EaseOut"
            ]
            .contains(
                &key["interp"]
                    .as_str()
                    .context("missing mask interpolation")?
            ),
            "invalid mask interpolation"
        );
        for name in ["in_influence", "out_influence"] {
            if let Some(value) = key.get(name) {
                ensure!(
                    value.as_f64().is_some_and(f64::is_finite),
                    "invalid mask influence"
                );
            }
        }
    }
    if keys.is_empty() {
        return Ok(baseline);
    }
    let time = |i: usize| {
        keys[i]["time"]
            .as_i64()
            .expect("validated source mask key time")
    };
    if tick <= time(0) {
        return Ok(paths[0].clone());
    }
    let last = keys.len() - 1;
    if tick >= time(last) {
        return Ok(paths[last].clone());
    }
    let i = match keys.binary_search_by_key(&tick, |k| k["time"].as_i64().unwrap()) {
        Ok(i) => return Ok(paths[i].clone()),
        Err(i) => i - 1,
    };
    let (a, b) = (&paths[i], &paths[i + 1]);
    if keys[i]["interp"] == "Hold" || a.vertices.len() != b.vertices.len() {
        return Ok(a.clone());
    }
    normalized["keyframes"] = json!([normalized["keyframes"][i], normalized["keyframes"][i + 1]]);
    normalized["keyframes"][1]["value"] = json!({"Float":1.});
    let easing = scalar::sample(&super::filmcraft_scalar_curve::convert(&normalized)?, tick);
    let vertices = a
        .vertices
        .iter()
        .zip(&b.vertices)
        .map(|(a, b)| Vertex {
            p: a.p.lerp(b.p, easing),
            t_in: a.t_in.lerp(b.t_in, easing),
            t_out: a.t_out.lerp(b.t_out, easing),
        })
        .collect();
    Ok(Path {
        vertices,
        closed: a.closed,
    })
}
fn flatten(
    path: &Path,
    scale: f32,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Vec<[f32; 2]>> {
    let n = path.vertices.len();
    if n == 0 {
        return Ok(vec![]);
    }
    let s = scale as f64;
    let tol = (0.05 / s.max(1e-6)).max(1e-4);
    let point = |p: Vec2| [(p.x * s) as f32, (p.y * s) as f32];
    let mut out = vec![point(path.vertices[0].p)];
    // Both closed and open source paths are filled with the closing cubic. A
    // single-vertex source path has no segment, including its authored tangents.
    if n < 2 {
        return Ok(out);
    }
    for i in 0..n {
        checkpoint()?;
        let (a, b) = (&path.vertices[i], &path.vertices[(i + 1) % n]);
        let (p0, c1, c2, p3) = (a.p, a.p + a.t_out, b.p + b.t_in, b.p);
        if (c1 - p0).length() >= 1e-12 || (c2 - p3).length() >= 1e-12 {
            let m = (p0 - c1 * 2. + c2)
                .length()
                .max((c1 - c2 * 2. + p3).length());
            let steps = ((0.75 * m / tol).sqrt().ceil() as usize).clamp(1, 256);
            for k in 1..steps {
                let t = k as f64 / steps as f64;
                let u = 1. - t;
                out.push(point(
                    p0 * (u * u * u)
                        + c1 * (3. * u * u * t)
                        + c2 * (3. * u * t * t)
                        + p3 * (t * t * t),
                ));
            }
        }
        if i + 1 != n {
            out.push(point(p3));
        }
    }
    ensure!(
        out.iter().flatten().all(|v| v.is_finite()),
        "nonfinite flattened mask"
    );
    Ok(out)
}
fn mode(raw: &Value) -> Result<MaskMode> {
    Ok(match raw.get("mode") {
        None => MaskMode::Add,
        Some(value) => match value.as_str() {
            Some("None") => MaskMode::None,
            Some("Add") => MaskMode::Add,
            Some("Subtract") => MaskMode::Subtract,
            Some("Intersect") => MaskMode::Intersect,
            Some("Lighten") => MaskMode::Lighten,
            Some("Darken") => MaskMode::Darken,
            Some("Difference") => MaskMode::Difference,
            _ => anyhow::bail!("invalid source mask mode"),
        },
    })
}
// Bound every copied JSON node and string before decoding/importing parameters.
// The walk itself borrows input, checks cancellation, and bounds recursion depth.
fn json_scratch(
    value: &Value,
    depth: usize,
    visits: &mut usize,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<usize> {
    ensure!(depth <= 128, "mask parameter nesting limit exceeded");
    *visits = visits
        .checked_add(1)
        .context("mask JSON node count overflow")?;
    if *visits % 256 == 0 {
        checkpoint()?;
    }
    let mut bytes = 512usize;
    match value {
        Value::String(value) => {
            bytes = bytes
                .checked_add(
                    value
                        .len()
                        .checked_mul(2)
                        .context("mask string scratch overflow")?,
                )
                .context("mask JSON scratch overflow")?
        }
        Value::Array(values) => {
            for value in values {
                bytes = bytes
                    .checked_add(json_scratch(value, depth + 1, visits, checkpoint)?)
                    .context("mask JSON scratch overflow")?;
            }
        }
        Value::Object(values) => {
            for (name, value) in values {
                bytes = bytes
                    .checked_add(
                        name.len()
                            .checked_mul(2)
                            .context("mask field scratch overflow")?,
                    )
                    .and_then(|n| n.checked_add(512))
                    .context("mask JSON scratch overflow")?;
                bytes = bytes
                    .checked_add(json_scratch(value, depth + 1, visits, checkpoint)?)
                    .context("mask JSON scratch overflow")?;
            }
        }
        _ => {}
    }
    Ok(bytes)
}
fn admission(
    raw: &[Value],
    w: usize,
    h: usize,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<usize> {
    let mut bytes = w
        .checked_mul(h)
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| n.checked_add(w.checked_mul(4)?))
        .context("mask image scratch overflow")?;
    for mask in raw {
        if mode(mask)? == MaskMode::None {
            continue;
        }
        let path = &mask["path"];
        let mut max_vertices = 0usize;
        let mut total = 0usize;
        let baseline = std::iter::once(&path["value"]);
        let keyed = path
            .get("keyframes")
            .map(|v| v.as_array().context("invalid mask keys"))
            .transpose()?;
        for value in baseline.chain(keyed.into_iter().flatten().map(|key| &key["value"])) {
            let count = value
                .pointer("/Path/vertices")
                .and_then(Value::as_array)
                .context("missing mask vertices")?
                .len();
            max_vertices = max_vertices.max(count);
            total = total
                .checked_add(count)
                .context("mask vertex count overflow")?;
        }
        let mut key_count = 0usize;
        let mut copy_bytes = 0usize;
        let mut visits = 0usize;
        for name in ["path", "feather", "opacity", "expansion"] {
            copy_bytes = copy_bytes
                .checked_add(json_scratch(&mask[name], 0, &mut visits, checkpoint)?)
                .context("mask JSON scratch overflow")?;
            let keys = mask[name]
                .get("keyframes")
                .map(|v| v.as_array().context("invalid mask parameter keys"))
                .transpose()?;
            key_count = key_count
                .checked_add(keys.map_or(0, Vec::len))
                .context("mask key count overflow")?;
        }
        // Bounds JSON import copies, sampled paths, flattened points and each row's
        // crossing/near-edge vectors, including vector capacity growth. Reserve before
        // any of these allocations. Coverage stays admitted until its final use.
        let geometry = max_vertices
            .checked_mul(256)
            .and_then(|n| n.checked_mul(128))
            .context("mask flatten scratch overflow")?;
        let import = total
            .checked_mul(4096)
            .and_then(|n| n.checked_add(key_count.checked_mul(4096)?))
            .and_then(|n| n.checked_add(4096))
            .and_then(|n| n.checked_add(copy_bytes.checked_mul(4)?))
            .context("mask import scratch overflow")?;
        bytes = bytes
            .checked_add(geometry)
            .and_then(|n| n.checked_add(import))
            .context("mask scratch overflow")?;
    }
    Ok(bytes)
}
pub(crate) fn coverage(
    context: &RenderContext,
    raw: &Value,
    tick: i64,
    scale: f32,
    w: usize,
    h: usize,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Option<Coverage>> {
    checkpoint()?;
    ensure!(scale.is_finite() && scale > 0., "invalid mask scale");
    let masks = match raw {
        Value::Null => return Ok(None),
        _ => raw.as_array().context("invalid source masks")?,
    };
    let mut active = false;
    for raw in masks {
        checkpoint()?;
        active |= mode(raw)? != MaskMode::None;
    }
    if !active {
        return Ok(None);
    }
    let reservation = context.reserve_scratch(admission(masks, w, h, checkpoint)?);
    let mut prepared = Vec::new();
    let s = scale as f64;
    for raw in masks {
        checkpoint()?;
        let mode = mode(raw)?;
        if mode == MaskMode::None {
            continue;
        }
        let scalar_at = |name: &str| -> Result<f64> {
            let curve = scalar::convert(
                raw.get(name)
                    .with_context(|| format!("missing mask {name}"))?,
                scalar::Evaluation::EffectParameter,
            )?;
            Ok(scalar::sample(&curve, tick))
        };
        let path = sample_path(
            raw.get("path").context("missing mask path")?,
            tick,
            checkpoint,
        )?;
        let feather = (scalar_at("feather")?.max(0.) * s) as f32;
        let expansion = (scalar_at("expansion")? * s) as f32;
        let opacity = (scalar_at("opacity")? / 100.).clamp(0., 1.) as f32;
        ensure!(
            feather.is_finite() && expansion.is_finite(),
            "nonfinite mask falloff"
        );
        prepared.push(FlatMask {
            pts: flatten(&path, scale, checkpoint)?,
            feather,
            expansion,
            opacity,
            inverted: raw
                .get("inverted")
                .map(|v| v.as_bool().context("invalid mask inversion"))
                .transpose()?
                .unwrap_or(false),
            mode,
        });
    }
    let mut values =
        vec![prepared[0].mode.start(); w.checked_mul(h).context("mask canvas overflow")?];
    let mut tmp = vec![0.; w];
    if w > 0 {
        for (y, row) in values.chunks_mut(w).enumerate() {
            checkpoint()?;
            for mask in &prepared {
                mask_row(mask, y, &mut tmp, checkpoint)?;
                for (a, v) in row.iter_mut().zip(&tmp) {
                    *a = mask.mode.combine(*a, *v).clamp(0., 1.);
                }
            }
        }
    }
    Ok(Some(Coverage {
        values,
        _reservation: reservation,
    }))
}
pub(crate) fn opacity(
    context: &RenderContext,
    effects: &Value,
    tick: i64,
    source_width: u32,
    image: Image,
) -> Result<Image> {
    ensure!(source_width > 0, "invalid opacity mask source width");
    let raw = effects
        .as_array()
        .context("missing opacity effects")?
        .iter()
        .find(|e| e["effect"] == "opacity")
        .filter(|e| e["enabled"] != false);
    let Some(raw) = raw else { return Ok(image) };
    if raw
        .get("masks")
        .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty))
    {
        return Ok(image);
    }
    let scale = image.w as f32 / source_width as f32;
    context
        .image_work(&[], || {
            let run = || -> Result<Image> {
                let mut checkpoint = || {
                    ensure!(!context.is_cancelled(), "opacity mask cancelled");
                    Ok(())
                };
                let cov = coverage(
                    context,
                    &raw["masks"],
                    tick,
                    scale,
                    image.w,
                    image.h,
                    &mut checkpoint,
                )?;
                let mut image = image;
                if let Some(cov) = cov {
                    for (row, cover) in image
                        .px
                        .chunks_mut(image.w * 4)
                        .zip(cov.values.chunks(image.w))
                    {
                        checkpoint()?;
                        for (p, c) in row.chunks_exact_mut(4).zip(cover) {
                            for v in p {
                                *v *= *c;
                            }
                        }
                    }
                }
                Ok(image)
            };
            run().map_err(|e| e.to_string())
        })
        .map_err(anyhow::Error::msg)
}
/// The falloff of a signed (expanded) distance for a feather width.
#[inline]
pub fn falloff(s: f32, feather: f32) -> f32 {
    let w = feather.max(1.0);
    let u = (s / w + 0.5).clamp(0.0, 1.0);
    let smooth = u * u * (3.0 - 2.0 * u);
    u + (smooth - u) * feather.min(1.0)
}

/// Distance band beyond which coverage is saturated (0 or 1).
fn band(m: &FlatMask) -> f32 {
    m.expansion.abs() + m.feather.max(1.0) * 0.5 + 1.0
}

#[inline]
fn seg_dist2(px: f32, py: f32, a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let (qx, qy) = (px - a[0], py - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        ((qx * dx + qy * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (ex, ey) = (qx - dx * t, qy - dy * t);
    ex * ex + ey * ey
}

/// Coverage of one mask for row `y` into `out` (length = width). Reference per-row algorithm:
/// winding from the row's sorted edge crossings, distance only against edges within the band.
fn mask_row(
    m: &FlatMask,
    y: usize,
    out: &mut [f32],
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    checkpoint()?;
    let n = m.pts.len();
    let py = y as f32 + 0.5;
    let b = band(m);
    let value = |sd: f32| {
        let c = falloff(sd + m.expansion, m.feather);
        m.opacity * if m.inverted { 1.0 - c } else { c }
    };
    if n < 3 {
        out.fill(value(-b));
        return Ok(());
    }
    // crossings of the scanline with the polygon edges (x, direction)
    let mut cross: Vec<(f32, i32)> = Vec::new();
    // edges within the band of this row: (a, b, min x, max x)
    let mut near: Vec<([f32; 2], [f32; 2], f32, f32)> = Vec::new();
    for i in 0..n {
        if i % 256 == 0 {
            checkpoint()?;
        }
        let a = m.pts[i];
        let c = m.pts[(i + 1) % n];
        if (a[1] <= py) != (c[1] <= py) {
            let t = (py - a[1]) / (c[1] - a[1]);
            cross.push((a[0] + (c[0] - a[0]) * t, if c[1] > a[1] { 1 } else { -1 }));
        }
        if py >= a[1].min(c[1]) - b && py <= a[1].max(c[1]) + b {
            near.push((a, c, a[0].min(c[0]) - b, a[0].max(c[0]) + b));
        }
    }
    cross.sort_by(|p, q| p.0.total_cmp(&q.0));
    let mut ci = 0;
    let mut wind = 0;
    let b2 = b * b;
    for (x, o) in out.iter_mut().enumerate() {
        if x % 256 == 0 {
            checkpoint()?;
        }
        let px = x as f32 + 0.5;
        while ci < cross.len() && cross[ci].0 < px {
            wind += cross[ci].1;
            ci += 1;
        }
        let inside = wind != 0;
        let mut d2 = b2;
        for (a, c, x0, x1) in &near {
            if px >= *x0 && px <= *x1 {
                d2 = d2.min(seg_dist2(px, py, *a, *c));
            }
        }
        let d = d2.sqrt();
        *o = value(if inside { d } else { -d });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    fn mask() -> Value {
        json!({"name":"budget","mode":"Add","path":{"value":{"Path":{"vertices":[{"p":{"x":1.25,"y":1.25}},{"p":{"x":7.25,"y":1.25}},{"p":{"x":7.25,"y":7.25}},{"p":{"x":1.25,"y":7.25}}],"closed":true}}},"feather":{"value":{"Float":0.75}},"opacity":{"value":{"Float":65.}},"expansion":{"value":{"Float":-0.25}}})
    }
    #[test]
    fn original_mask_path_easing_topology_and_flatten_match_every_bit() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-mask-path.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 1960);
        for (i, case) in cases.iter().enumerate() {
            let path = sample_path(
                &case["parameter"],
                case["at"].as_i64().unwrap(),
                &mut || Ok(()),
            )
            .unwrap();
            assert_eq!(json!(path.closed), case["closed"], "path closed {i}");
            let bits: Vec<_> = path
                .vertices
                .iter()
                .flat_map(|v| [v.p.x, v.p.y, v.t_in.x, v.t_in.y, v.t_out.x, v.t_out.y])
                .map(f64::to_bits)
                .collect();
            assert_eq!(json!(bits), case["bits"], "path components {i}");
            for flat in case["flat"].as_array().unwrap() {
                let points = flatten(
                    &path,
                    flat["scale"].as_f64().unwrap() as f32,
                    &mut || Ok(()),
                )
                .unwrap();
                let bits: Vec<_> = points.into_iter().map(|p| p.map(f32::to_bits)).collect();
                assert_eq!(json!(bits), flat["bits"], "path flatten {i}");
            }
        }
    }
    #[test]
    fn mask_budget_refusal_precancel_and_malformed_release() {
        for mode in 0..3 {
            let flag = Arc::new(AtomicBool::new(false));
            let context = RenderContext::new_with_budget(
                flag.clone(),
                1,
                128,
                if mode == 0 {
                    128 * 16 + 1
                } else {
                    32 * 1024 * 1024
                },
            )
            .unwrap();
            let image = context.image_work(&[], || Ok(Image::new(16, 8))).unwrap();
            let mut mask = mask();
            if mode == 1 {
                flag.store(true, Ordering::Relaxed);
            }
            if mode == 2 {
                mask["path"]["value"]["Path"]["vertices"][0]["p"]["x"] = Value::Null;
            }
            assert!(opacity(
                &context,
                &json!([{"effect":"opacity","enabled":true,"masks":[mask]}]),
                0,
                16,
                image
            )
            .is_err());
            assert_eq!(context.pixel_bytes_in_use(), 0);
        }
    }
    #[test]
    fn mask_geometry_and_raster_checkpoint_failures_release() {
        for stop in [1, 4, 12, 24] {
            let context = RenderContext::new_with_budget(
                Arc::new(AtomicBool::new(false)),
                1,
                128,
                32 * 1024 * 1024,
            )
            .unwrap();
            let result = context.image_work(&[], || {
                let image = Image::new(16, 8);
                let raw = json!([mask()]);
                let mut count = 0;
                let mut checkpoint = || {
                    count += 1;
                    ensure!(count < stop, "checkpoint cancelled");
                    Ok(())
                };
                coverage(&context, &raw, 0, 1., 16, 8, &mut checkpoint)
                    .map_err(|e| e.to_string())?;
                Ok(image)
            });
            assert!(result.is_err(), "stop {stop}");
            assert_eq!(context.pixel_bytes_in_use(), 0);
        }
    }
    #[test]
    fn all_mask_scalar_keys_are_included_in_admission() {
        let mut raw = mask();
        let base = admission(&[raw.clone()], 16, 8, &mut || Ok(())).unwrap();
        raw["feather"]["keyframes"] = json!((0..128)
            .map(|i| json!({"time":i,"interp":"Linear","value":{"Float":1.}}))
            .collect::<Vec<_>>());
        assert!(admission(&[raw], 16, 8, &mut || Ok(())).unwrap() - base >= 128 * 4096);
    }
    #[test]
    fn oversized_parameter_payload_is_refused_before_import_copy() {
        let context =
            RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)), 1, 128, 512 * 1024)
                .unwrap();
        let image = context.image_work(&[], || Ok(Image::new(16, 8))).unwrap();
        let mut raw = mask();
        raw["path"]["unsupported"] = json!("x".repeat(1024 * 1024));
        let failure = opacity(
            &context,
            &json!([{"effect":"opacity","masks":[raw]}]),
            0,
            16,
            image,
        )
        .unwrap_err();
        assert!(failure.to_string().contains("budget"), "{failure}");
        assert_eq!(context.pixel_bytes_in_use(), 0);
    }
    #[test]
    fn disabled_mask_modes_leave_working_pixels_unchanged() {
        let context =
            RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)), 1, 128, 128 * 16)
                .unwrap();
        let image = context
            .image_work(&[], || {
                let mut image = Image::new(16, 8);
                image.px.fill(0.25);
                Ok(image)
            })
            .unwrap();
        let output = opacity(
            &context,
            &json!([{"effect":"opacity","masks":[{"mode":"None"}]}]),
            0,
            16,
            image,
        )
        .unwrap();
        assert!(output.px.iter().all(|v| v.to_bits() == 0.25f32.to_bits()));
        drop(output);
        assert_eq!(context.pixel_bytes_in_use(), 0);
    }
}

/// Full f64 original tracking region; raster flattening rounds only for pixels.
pub fn tracking_path(
    parameter: &Value,
    tick: i64,
) -> Result<(Value, Vec<super::warp_stabilizer::Vec2>)> {
    let path = sample_path(parameter, tick, &mut || Ok(()))?;
    ensure!(
        path.vertices.len() <= 4096,
        "tracking path vertex budget exceeded"
    );
    let mut region = path.vertices.first().map(|v| vec![v.p]).unwrap_or_default();
    if path.vertices.len() < 2 {
        return Ok((
            json!({"Path":path}),
            region
                .into_iter()
                .map(|p| super::warp_stabilizer::Vec2::new(p.x, p.y))
                .collect(),
        ));
    }
    for i in 0..path.vertices.len() {
        let a = &path.vertices[i];
        let b = &path.vertices[(i + 1) % path.vertices.len()];
        let (p0, c1, c2, p3) = (a.p, a.p + a.t_out, b.p + b.t_in, b.p);
        if (c1 - p0).length() >= 1e-12 || (c2 - p3).length() >= 1e-12 {
            let m = (p0 - c1 * 2. + c2)
                .length()
                .max((c1 - c2 * 2. + p3).length());
            let steps = ((0.75 * m / 0.5).sqrt().ceil() as usize).clamp(1, 256);
            for k in 1..steps {
                let t = k as f64 / steps as f64;
                let u = 1. - t;
                region.push(
                    p0 * (u * u * u)
                        + c1 * (3. * u * u * t)
                        + c2 * (3. * u * t * t)
                        + p3 * (t * t * t),
                );
            }
        }
        if i + 1 != path.vertices.len() {
            region.push(p3);
        }
    }
    Ok((
        json!({"Path":path}),
        region
            .into_iter()
            .map(|p| super::warp_stabilizer::Vec2::new(p.x, p.y))
            .collect(),
    ))
}
