//! Source mask Program-monitor geometry. Original model: Filmcraft 52318524 (MIT).
use super::{
    filmcraft_parameter_edit::{self, Address, Command},
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_sequence_source::SourceTrackItem,
    transition_kernel_pack::geom::{Affine, Vec2},
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Part {
    Vertex,
    In,
    Out,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PenPoint {
    pub point: [f64; 2],
    pub handle: Option<[f64; 2]>,
}
#[derive(Default, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Operation {
    #[default]
    Read,
    BodyClick {
        point: [f64; 2],
        pixel_scale: [f64; 2],
    },
    Move {
        index: usize,
        part: Part,
        point: [f64; 2],
        #[serde(default)]
        break_handles: bool,
    },
    Falloff {
        parameter: FalloffParameter,
        delta: [f64; 2],
    },
    Translate {
        delta: [f64; 2],
        #[serde(default)]
        origin: Option<[f64; 2]>,
    },
    Append {
        point: [f64; 2],
    },
    PenDraft {
        points: Vec<PenPoint>,
        closed: bool,
    },
    InsertNear {
        point: [f64; 2],
        pixel_scale: [f64; 2],
    },
    Delete {
        index: usize,
    },
    Smooth {
        index: usize,
    },
    Close {
        closed: bool,
    },
    Box {
        shape: BoxShape,
        from: [f64; 2],
        to: [f64; 2],
    },
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FalloffParameter {
    Feather,
    Expansion,
}
impl FalloffParameter {
    fn name(self) -> &'static str {
        match self {
            Self::Feather => "feather",
            Self::Expansion => "expansion",
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BoxShape {
    Rectangle,
    Ellipse,
}
struct ContextData {
    path: String,
    clock: i64,
    matrix: Affine,
    canvas: (u32, u32),
    locked: bool,
    visible: bool,
}
fn context(
    document: &SourceProjectDocument,
    address: &Address,
    timeline: i64,
) -> Result<ContextData> {
    ensure!(
        address.mask_index.is_some() && address.parameter == "path",
        "monitor requires a mask path address"
    );
    let (path, locked) = filmcraft_parameter_edit::parameter_path(document, address)?;
    let sequence = address.sequence_id.parse()?;
    let clip = address.clip_id.parse()?;
    let (raw_path, _) = filmcraft_parameter_edit::location(document, sequence, clip)?;
    let raw = document.envelope.pointer(&raw_path).unwrap();
    let item = SourceTrackItem::read(raw)?;
    let start = raw["start"].as_i64().context("invalid clip start")?;
    let end = start
        .checked_add(raw["duration"].as_i64().context("invalid clip duration")?)
        .and_then(|v| v.checked_sub(1))
        .context("clip end overflow")?;
    ensure!(end >= start, "empty mask clip");
    let clock = item.clocks(timeline.clamp(start, end))?.frame;
    let appearance = super::filmcraft_video_appearance::sample(document, sequence, clip, timeline)?;
    let [a, b, c, d, e, f] = appearance.matrix;
    let seq = document.project()["items"]
        .get(sequence.to_string())
        .and_then(|v| v.pointer("/kind/Sequence/settings"))
        .context("missing canvas settings")?;
    let canvas = (
        u32::try_from(seq["width"].as_u64().context("invalid canvas width")?)?,
        u32::try_from(seq["height"].as_u64().context("invalid canvas height")?)?,
    );
    ensure!(canvas.0 > 0 && canvas.1 > 0, "empty monitor canvas");
    let visible =
        super::filmcraft_video_jobs::SourceFrameJobs::build(document, sequence, timeline)?
            .clips
            .contains_key(&clip);
    Ok(ContextData {
        path,
        clock,
        matrix: Affine { a, b, c, d, e, f },
        canvas,
        locked,
        visible,
    })
}
fn sampled(document: &SourceProjectDocument, ctx: &ContextData) -> Result<Value> {
    Ok(super::filmcraft_mask_track::tracking_path(
        document.envelope.pointer(&ctx.path).unwrap(),
        ctx.clock,
    )?
    .0)
}
fn point(value: &Value) -> Vec2 {
    Vec2::new(
        value["x"].as_f64().unwrap_or(0.),
        value["y"].as_f64().unwrap_or(0.),
    )
}
fn wire(p: Vec2) -> Value {
    json!({"x":p.x,"y":p.y})
}
fn scene_point(matrix: &Affine, p: Vec2) -> [f64; 2] {
    let p = matrix.apply(p);
    [p.x, p.y]
}
// Original ui-egui masks::offset: averaged neighbouring tangents and
// orientation from the signed y-down shoelace area (not a miter offset).
fn offset(points: &[Vec2], distance: f64) -> Vec<Vec2> {
    let n = points.len();
    if n < 3 || distance == 0. {
        return points.to_vec();
    }
    let area: f64 = (0..n)
        .map(|i| points[i].x * points[(i + 1) % n].y - points[(i + 1) % n].x * points[i].y)
        .sum();
    let sign = if area > 0. { 1. } else { -1. };
    (0..n)
        .map(|i| {
            let tangent = points[(i + 1) % n] - points[(i + n - 1) % n];
            let length = tangent.length().max(1e-9);
            points[i] + Vec2::new(tangent.y / length, -tangent.x / length) * (sign * distance)
        })
        .collect()
}
fn falloff_geometry(
    document: &SourceProjectDocument,
    ctx: &ContextData,
) -> Result<(Vec<Vec2>, f64, f64)> {
    let mask_path = ctx
        .path
        .rsplit_once('/')
        .context("invalid mask parameter path")?
        .0;
    let mask = document
        .envelope
        .pointer(mask_path)
        .context("mask missing")?;
    let sample = |name: &str, default: f64| -> Result<f64> {
        let Some(parameter) = mask.get(name) else {
            return Ok(default);
        };
        // Sampling consumes known fields only; authoring retains opaque source
        // extensions just like the source parameter inspector.
        let mut known = json!({"value":parameter["value"]});
        if let Some(keys) = parameter.get("keyframes") {
            let keys = keys.as_array().context("invalid falloff keys")?;
            known["keyframes"] = json!(keys
                .iter()
                .map(|key| {
                    let mut fields = serde_json::Map::new();
                    for name in ["time", "value", "interp", "in_influence", "out_influence"] {
                        if let Some(value) = key.get(name) {
                            fields.insert(name.into(), value.clone());
                        }
                    }
                    Value::Object(fields)
                })
                .collect::<Vec<_>>());
        }
        let curve = super::filmcraft_effect_scalar::convert(
            &known,
            super::filmcraft_effect_scalar::Evaluation::EffectParameter,
        )?;
        let value = super::filmcraft_effect_scalar::sample(&curve, ctx.clock);
        ensure!(value.is_finite(), "invalid mask falloff");
        Ok(value)
    };
    let (_, flat) = super::filmcraft_mask_track::tracking_path(
        document.envelope.pointer(&ctx.path).unwrap(),
        ctx.clock,
    )?;
    Ok((
        flat.into_iter().map(|p| Vec2::new(p.x, p.y)).collect(),
        sample("feather", 10.)?.max(0.),
        sample("expansion", 0.)?,
    ))
}
fn falloff_anchor(
    points: &[Vec2],
    matrix: &Affine,
    parameter: FalloffParameter,
) -> Option<(Vec2, Vec2)> {
    let indices = 0..points.len();
    let compare = |a: &usize, b: &usize| {
        matrix
            .apply(points[*a])
            .y
            .total_cmp(&matrix.apply(points[*b]).y)
    };
    let index = match parameter {
        FalloffParameter::Feather => indices.min_by(compare),
        FalloffParameter::Expansion => indices.max_by(compare),
    }?;
    let normal = offset(points, 1.)[index] - points[index];
    Some((points[index], normal * (1. / normal.length().max(1e-9))))
}
fn falloff_overlay(document: &SourceProjectDocument, ctx: &ContextData) -> Result<Value> {
    let (flat, feather, expansion) = falloff_geometry(document, ctx)?;
    let project = |distance| -> Result<Vec<[f64; 2]>> {
        let points = offset(&flat, distance)
            .into_iter()
            .map(|p| scene_point(&ctx.matrix, p))
            .collect::<Vec<_>>();
        ensure!(
            points.iter().flatten().all(|n| n.is_finite()),
            "mask falloff contour overflow"
        );
        Ok(points)
    };
    let mut handles = vec![];
    for parameter in [FalloffParameter::Feather, FalloffParameter::Expansion] {
        if let Some((anchor, normal)) = falloff_anchor(&flat, &ctx.matrix, parameter) {
            let distance = match parameter {
                FalloffParameter::Feather => expansion + feather / 2.,
                FalloffParameter::Expansion => expansion,
            };
            let direction = ctx.matrix.apply(anchor + normal) - ctx.matrix.apply(anchor);
            let point = scene_point(&ctx.matrix, anchor + normal * distance);
            ensure!(
                point
                    .iter()
                    .chain([direction.x, direction.y].iter())
                    .all(|n| n.is_finite()),
                "mask falloff handle overflow"
            );
            handles.push(json!({"parameter":parameter.name(),"point":point,"direction":[direction.x,direction.y]}));
        }
    }
    Ok(
        json!({"feather":feather,"expansion":expansion,"expansionContour":if expansion.abs()>1e-6 {project(expansion)?} else {vec![]},"featherContour":if feather>1e-6 {project(expansion+feather/2.)?} else {vec![]},"handles":handles}),
    )
}
fn overlay(document: &SourceProjectDocument, ctx: &ContextData) -> Result<Value> {
    let path = sampled(document, ctx)?;
    let vertices = path["Path"]["vertices"].as_array().unwrap();
    let closed = path["Path"]["closed"].as_bool().unwrap_or(true);
    let points=vertices.iter().enumerate().map(|(index,v)|{let p=point(&v["p"]);json!({"index":index,"point":scene_point(&ctx.matrix,p),"incoming":scene_point(&ctx.matrix,p+point(&v["t_in"])),"outgoing":scene_point(&ctx.matrix,p+point(&v["t_out"]))})}).collect::<Vec<_>>();
    let mut svg = String::new();
    if let Some(first) = points.first() {
        let p = &first["point"];
        svg = format!("M {} {}", p[0], p[1]);
        for i in 0..if closed && points.len() > 1 {
            points.len()
        } else {
            points.len().saturating_sub(1)
        } {
            let a = &points[i];
            let b = &points[(i + 1) % points.len()];
            svg.push_str(&format!(
                " C {} {}, {} {}, {} {}",
                a["outgoing"][0],
                a["outgoing"][1],
                b["incoming"][0],
                b["incoming"][1],
                b["point"][0],
                b["point"][1]
            ));
        }
        if closed {
            svg.push_str(" Z");
        }
    }
    ensure!(
        points.iter().all(
            |v| ["point", "incoming", "outgoing"].iter().all(|name| v[*name]
                .as_array()
                .unwrap()
                .iter()
                .all(|n| n.as_f64().is_some_and(f64::is_finite)))
        ),
        "monitor geometry overflow"
    );
    Ok(
        json!({"canvas":[ctx.canvas.0,ctx.canvas.1],"matrix":[ctx.matrix.a,ctx.matrix.b,ctx.matrix.c,ctx.matrix.d,ctx.matrix.e,ctx.matrix.f],"sampleTicks":ctx.clock.to_string(),"locked":ctx.locked,"visible":ctx.visible,"editable":!ctx.locked&&ctx.visible&&ctx.matrix.inverse().is_some(),"closed":closed,"vertices":points,"svgPath":svg,"falloff":falloff_overlay(document,ctx)?}),
    )
}
fn set_at(
    document: &SourceProjectDocument,
    address: &Address,
    ctx: &ContextData,
    value: Value,
) -> Result<SourceProjectDocument> {
    let (parameter_path, _) = filmcraft_parameter_edit::parameter_path(document, address)?;
    let parameter = document.envelope.pointer(&parameter_path).unwrap();
    let keys = parameter["keyframes"].as_array();
    if keys.is_none_or(|keys| keys.is_empty()) {
        return filmcraft_parameter_edit::edit(document, address, Command::SetBase { value }, 0);
    }
    let keys = keys.unwrap();
    let present = keys.iter().any(|k| k["time"].as_i64() == Some(ctx.clock));
    let interp = keys
        .iter()
        .rev()
        .find(|k| k["time"].as_i64().unwrap() < ctx.clock)
        .unwrap_or(&keys[0])["interp"]
        .clone();
    let mut changed = filmcraft_parameter_edit::edit(
        document,
        address,
        Command::Upsert {
            time: ctx.clock.to_string(),
            value,
        },
        0,
    )?;
    if !present {
        let key = changed.envelope.pointer_mut(&parameter_path).unwrap()["keyframes"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|k| k["time"].as_i64() == Some(ctx.clock))
            .unwrap();
        key["interp"] = interp;
    }
    Ok(changed)
}
/// Source masks.addVertex: insert a midpoint into every animated key, then
/// author the requested location at the sampled frame. Unequal nonempty key
/// topology is retained, with each key's insertion position clamped separately.
fn insert_after(
    document: &SourceProjectDocument,
    address: &Address,
    ctx: &ContextData,
    after: usize,
    at: Vec2,
) -> Result<(SourceProjectDocument, usize)> {
    let parameter = document.envelope.pointer(&ctx.path).unwrap();
    if parameter["keyframes"]
        .as_array()
        .is_some_and(|keys| keys.len() > 1)
    {
        let mut doc = document.clone();
        for key in doc.envelope.pointer_mut(&ctx.path).unwrap()["keyframes"]
            .as_array_mut()
            .unwrap()
        {
            let vertices = key["value"]["Path"]["vertices"]
                .as_array_mut()
                .context("invalid keyed mask path")?;
            ensure!(
                !vertices.is_empty() && vertices.len() < 4096,
                "keyed insertion needs nonempty paths within the vertex budget"
            );
            let index = after.saturating_add(1).min(vertices.len());
            let a = point(&vertices[after.min(vertices.len() - 1)]["p"]);
            let b = point(&vertices[index % vertices.len()]["p"]);
            vertices.insert(index, json!({"p":wire(a.lerp(b,0.5))}));
        }
        let mut path = sampled(&doc, ctx)?;
        let vertices = path["Path"]["vertices"].as_array_mut().unwrap();
        let index = after.saturating_add(1).min(vertices.len() - 1);
        vertices[index] = json!({"p":wire(at)});
        return Ok((set_at(&doc, address, ctx, path)?, index));
    }
    let mut path = sampled(document, ctx)?;
    let vertices = path["Path"]["vertices"].as_array_mut().unwrap();
    ensure!(vertices.len() < 4096, "mask vertex budget exceeded");
    let index = after.saturating_add(1).min(vertices.len());
    vertices.insert(index, json!({"p":wire(at)}));
    Ok((set_at(document, address, ctx, path)?, index))
}
/// Original monitor nonzero-winding fill hit test, including open paths.
fn inside(points: &[super::warp_stabilizer::Vec2], p: Vec2) -> bool {
    let n = points.len();
    let mut winding = 0;
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        if (a.y <= p.y) != (b.y <= p.y) && a.x + (b.x - a.x) * (p.y - a.y) / (b.y - a.y) < p.x {
            winding += if b.y > a.y { 1 } else { -1 };
        }
    }
    winding != 0
}
fn nearest_segment(
    path: &Value,
    matrix: &Affine,
    target: [f64; 2],
    scale: [f64; 2],
) -> Result<Option<usize>> {
    ensure!(
        target.iter().all(|n| n.is_finite()) && scale.iter().all(|n| n.is_finite() && *n > 0.),
        "invalid monitor hit-test coordinates"
    );
    let vertices = path["Path"]["vertices"].as_array().unwrap();
    if vertices.len() < 2 {
        return Ok(None);
    }
    let mut best = None;
    for i in 0..vertices.len() {
        // The original monitor uses a two-vertex open MaskPath's flattened
        // filled boundary at tolerance 0.5, including its closing cubic.
        let segment = json!({"value":{"Path":{"vertices":[vertices[i],vertices[(i+1)%vertices.len()]],"closed":false}}});
        let (_, flat) = super::filmcraft_mask_track::tracking_path(&segment, 0)?;
        for pair in flat.windows(2) {
            let screen = |p: super::warp_stabilizer::Vec2| {
                let p = matrix.apply(Vec2::new(p.x, p.y));
                Vec2::new((p.x - target[0]) * scale[0], (p.y - target[1]) * scale[1])
            };
            let (a, b) = (screen(pair[0]), screen(pair[1]));
            let d = b - a;
            let length = d.x * d.x + d.y * d.y;
            let t = if length > 0. {
                (-(a.x * d.x + a.y * d.y) / length).clamp(0., 1.)
            } else {
                0.
            };
            let distance = (a + d * t).length();
            if distance.is_finite() && best.is_none_or(|(_, old)| distance < old) {
                best = Some((i, distance));
            }
        }
    }
    Ok(best
        .filter(|(_, distance)| *distance < 8.)
        .map(|(index, _)| index))
}
pub fn evaluate(
    document: &SourceProjectDocument,
    address: &Address,
    timeline: i64,
    operation: Operation,
) -> Result<(Value, Option<SourceProjectDocument>)> {
    let ctx = context(document, address, timeline)?;
    if matches!(operation, Operation::Read) {
        return Ok((overlay(document, &ctx)?, None));
    }
    ensure!(
        !ctx.locked && ctx.visible,
        "mask is locked or not active in the monitor"
    );
    let inverse = ctx
        .matrix
        .inverse()
        .context("mask motion matrix is not invertible")?;
    let convert = |p: [f64; 2]| -> Result<Vec2> {
        ensure!(p.iter().all(|n| n.is_finite()), "invalid monitor point");
        let p = inverse.apply(Vec2::new(p[0], p[1]));
        ensure!(
            p.x.is_finite() && p.y.is_finite(),
            "monitor coordinate overflow"
        );
        Ok(p)
    };
    if let Operation::BodyClick { point, pixel_scale } = operation {
        ensure!(
            pixel_scale.iter().all(|n| n.is_finite() && *n > 0.),
            "invalid monitor pixel scale"
        );
        let local = convert(point)?;
        let (_, flat) = super::filmcraft_mask_track::tracking_path(
            document.envelope.pointer(&ctx.path).unwrap(),
            ctx.clock,
        )?;
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for p in &flat {
            let p = scene_point(&ctx.matrix, Vec2::new(p.x, p.y));
            ensure!(p.iter().all(|n| n.is_finite()), "mask body bounds overflow");
            for axis in 0..2 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        // Original body interaction is the flattened screen bounds expanded
        // by two CSS pixels and intersected with the monitor picture.
        let in_bounds = (0..2).all(|axis| {
            point[axis] >= (min[axis] - 2. / pixel_scale[axis]).max(0.)
                && point[axis]
                    <= (max[axis] + 2. / pixel_scale[axis]).min(if axis == 0 {
                        ctx.canvas.0
                    } else {
                        ctx.canvas.1
                    } as f64)
        });
        let mut view = overlay(document, &ctx)?;
        view["deselectMask"] = json!(in_bounds && !inside(&flat, local));
        return Ok((view, None));
    }
    if let Operation::Falloff { parameter, delta } = operation {
        ensure!(delta.iter().all(|n| n.is_finite()), "invalid falloff drag");
        let (flat, feather, expansion) = falloff_geometry(document, &ctx)?;
        let (_, normal) = falloff_anchor(&flat, &ctx.matrix, parameter)
            .context("empty mask has no falloff handle")?;
        let origin = inverse.apply(Vec2::new(0., 0.));
        let delta = convert(delta)? - origin;
        let along = delta.x * normal.x + delta.y * normal.y;
        let value = match parameter {
            FalloffParameter::Feather => (feather + 2. * along).max(0.),
            FalloffParameter::Expansion => expansion + along,
        };
        ensure!(value.is_finite(), "falloff drag overflow");
        let mut target = address.clone();
        target.parameter = parameter.name().into();
        let changed = set_at(document, &target, &ctx, json!({"Float":value}))?;
        return Ok((overlay(&changed, &ctx)?, Some(changed)));
    }
    let mut path = sampled(document, &ctx)?;
    let vertices = path["Path"]["vertices"].as_array_mut().unwrap();
    let mut changed = None;
    let mut selected_vertex = None;
    match operation {
        Operation::Falloff { .. } | Operation::BodyClick { .. } => unreachable!(),
        Operation::Read => unreachable!(),
        Operation::Move {
            index,
            part,
            point: target,
            break_handles,
        } => {
            let v = vertices.get_mut(index).context("mask vertex missing")?;
            let target = convert(target)?;
            match part {
                Part::Vertex => v["p"] = wire(target),
                Part::In | Part::Out => {
                    let tangent = target - point(&v["p"]);
                    let (name, other) = if matches!(part, Part::In) {
                        ("t_in", "t_out")
                    } else {
                        ("t_out", "t_in")
                    };
                    v[name] = wire(tangent);
                    if !break_handles {
                        v[other] = wire(tangent * -1.);
                    }
                }
            }
        }
        Operation::Translate { delta, origin } => {
            if let Some(origin) = origin {
                let on_canvas = origin[0] >= 0.
                    && origin[1] >= 0.
                    && origin[0] <= ctx.canvas.0 as f64
                    && origin[1] <= ctx.canvas.1 as f64;
                let origin = convert(origin)?;
                let (_, flat) = super::filmcraft_mask_track::tracking_path(
                    document.envelope.pointer(&ctx.path).unwrap(),
                    ctx.clock,
                )?;
                if !on_canvas || !inside(&flat, origin) {
                    return Ok((overlay(document, &ctx)?, None));
                }
            }
            let zero = convert([0., 0.])?;
            let d = convert(delta)? - zero;
            for v in vertices {
                v["p"] = wire(point(&v["p"]) + d);
            }
        }
        Operation::Smooth { index } => {
            ensure!(index < vertices.len(), "mask vertex missing");
            let v = &vertices[index];
            let tangent = if point(&v["t_in"]).x == 0.
                && point(&v["t_in"]).y == 0.
                && point(&v["t_out"]).x == 0.
                && point(&v["t_out"]).y == 0.
            {
                (point(&vertices[(index + 1) % vertices.len()]["p"])
                    - point(&vertices[(index + vertices.len() - 1) % vertices.len()]["p"]))
                    * (1. / 6.)
            } else {
                Vec2::ZERO
            };
            vertices[index]["t_out"] = wire(tangent);
            vertices[index]["t_in"] = wire(tangent * -1.);
        }
        Operation::Close { closed } => path["Path"]["closed"] = json!(closed),
        Operation::Box { shape, from, to } => {
            let a = convert(from)?;
            let b = convert(to)?;
            let (x0, y0, x1, y1) = (a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y));
            let shape = match shape {
                BoxShape::Rectangle => {
                    filmcraft_parameter_edit::MaskShape::Rectangle { x0, y0, x1, y1 }
                }
                BoxShape::Ellipse => {
                    filmcraft_parameter_edit::MaskShape::Ellipse { x0, y0, x1, y1 }
                }
            };
            path =
                filmcraft_parameter_edit::new_mask(String::new(), shape)?["path"]["value"].clone();
        }
        Operation::Append { point: at } => {
            let after = vertices.len().saturating_sub(1);
            let at = convert(at)?;
            let (doc, index) = insert_after(document, address, &ctx, after, at)?;
            changed = Some(doc);
            selected_vertex = Some(index);
        }
        Operation::InsertNear {
            point: at,
            pixel_scale,
        } => {
            let Some(after) = nearest_segment(&path, &ctx.matrix, at, pixel_scale)? else {
                return Ok((overlay(document, &ctx)?, None));
            };
            let (doc, index) = insert_after(document, address, &ctx, after, convert(at)?)?;
            changed = Some(doc);
            selected_vertex = Some(index);
        }
        Operation::PenDraft { points, closed } => {
            ensure!(points.len() <= 4096, "pen point budget exceeded");
            let vertices = points
                .into_iter()
                .map(|p| -> Result<Value> {
                    let anchor = convert(p.point)?;
                    let tangent = p
                        .handle
                        .map(convert)
                        .transpose()?
                        .map(|handle| handle - anchor)
                        .filter(|t| t.length() > 2.)
                        .unwrap_or(Vec2::ZERO);
                    Ok(json!({"p":wire(anchor),"t_in":wire(tangent * -1.),"t_out":wire(tangent)}))
                })
                .collect::<Result<Vec<_>>>()?;
            selected_vertex = vertices.len().checked_sub(1);
            path = json!({"Path":{"closed":closed,"vertices":vertices}});
        }
        Operation::Delete { index } => {
            ensure!(
                vertices.len() > 3 && index < vertices.len(),
                "a mask keeps at least three vertices"
            );
            let mut doc = document.clone();
            let parameter = doc.envelope.pointer_mut(&ctx.path).unwrap();
            let drop = |value: &mut Value| {
                if let Some(v) = value
                    .pointer_mut("/Path/vertices")
                    .and_then(Value::as_array_mut)
                {
                    if index < v.len() {
                        v.remove(index);
                    }
                }
            };
            drop(&mut parameter["value"]);
            if let Some(keys) = parameter["keyframes"].as_array_mut() {
                for key in keys {
                    drop(&mut key["value"]);
                }
            }
            changed = Some(doc);
        }
    }
    let changed = match changed {
        Some(changed) => changed,
        None => set_at(document, address, &ctx, path)?,
    };
    let mut view = overlay(&changed, &ctx)?;
    if let Some(index) = selected_vertex {
        view["selectedVertex"] = json!(index);
    }
    Ok((view, Some(changed)))
}

/// Stage a new mask in a private document only. Read returns no project and open
/// pen results remain client-local; closing the pen or a box publishes one
/// document containing both original Mask::new defaults and the complete path.
pub fn evaluate_new(
    document: &SourceProjectDocument,
    address: &Address,
    timeline: i64,
    operation: Operation,
) -> Result<(Value, Option<SourceProjectDocument>)> {
    ensure!(
        address.mask_index.is_none() && address.parameter == "path",
        "new-mask monitor must not address an existing mask"
    );
    ensure!(
        matches!(
            operation,
            Operation::Read | Operation::PenDraft { .. } | Operation::Box { .. }
        ),
        "new-mask monitor supports reading and drawing only"
    );
    if let Operation::PenDraft {
        ref points,
        closed: true,
    } = operation
    {
        ensure!(
            points.len() >= 3,
            "a closed new mask needs at least three vertices"
        );
    }
    ensure!(
        filmcraft_parameter_edit::maskable_effect(&address.effect_kind),
        "masks require an original video effect other than Motion"
    );
    let (clip, locked) = filmcraft_parameter_edit::location(
        document,
        address.sequence_id.parse()?,
        address.clip_id.parse()?,
    )?;
    ensure!(!locked, "source track is locked");
    let effect = document
        .envelope
        .pointer(&format!("{clip}/effects/{}", address.effect_index))
        .context("source effect missing")?;
    ensure!(
        effect["effect"].as_str() == Some(address.effect_kind.as_str()),
        "source effect identity changed"
    );
    let masks = effect
        .get("masks")
        .map(|v| v.as_array().context("invalid effect masks"))
        .transpose()?;
    let index = masks.map_or(0, Vec::len);
    let maximum = masks
        .into_iter()
        .flatten()
        .filter_map(|m| {
            m["name"]
                .as_str()?
                .strip_prefix("Mask (")?
                .strip_suffix(')')?
                .parse::<usize>()
                .ok()
        })
        .max()
        .unwrap_or(0);
    let ordinal = maximum
        .checked_add(1)
        .context("mask name ordinal overflow")?;
    let effect_address = filmcraft_parameter_edit::EffectAddress {
        sequence_id: address.sequence_id.clone(),
        clip_id: address.clip_id.clone(),
        effect_index: address.effect_index,
        effect_kind: address.effect_kind.clone(),
    };
    let staged = filmcraft_parameter_edit::edit_mask(
        document,
        &effect_address,
        filmcraft_parameter_edit::MaskCommand::Create {
            name: format!("Mask ({ordinal})"),
            shape: filmcraft_parameter_edit::MaskShape::FreeDraw,
        },
    )?;
    let mut selected = address.clone();
    selected.mask_index = Some(index);
    let (mut overlay, changed) = evaluate(&staged, &selected, timeline, operation)?;
    overlay["createdMaskIndex"] = json!(index);
    Ok((overlay, changed))
}
