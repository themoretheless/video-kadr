//! Original masks.track planning and sequential tracking (MIT, pinned 52318524).
//! Pure Rust state; media loading, authorization and task ownership belong to the host.
use super::{
    filmcraft_mask_commands as masks, filmcraft_parameter_sample,
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_sequence_inspect::canonical_parameter,
    filmcraft_sequence_source::SourceTrackItem,
    group_surface::GroupSurface,
    motion_tracker::{self, Gray, Prepared, TrackMethod},
    warp_stabilizer::{Affine, Vec2},
};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
pub struct Plan {
    pub document: SourceProjectDocument,
    pub sequence: u64,
    pub clip: u64,
    pub effect: usize,
    pub mask: usize,
    pub parameter_path: String,
    pub source: u64,
    pub width: u32,
    pub height: u32,
    pub from: i64,
    pub delta: i64,
    pub steps: usize,
    pub method: TrackMethod,
    pub name: String,
}
impl Plan {
    pub fn read(p: &Value) -> Result<Self> {
        let id = super::filmcraft_project_inspect::id;
        let bytes = match p.get("document").context("missing document")? {
            Value::String(s) => s.as_bytes().to_vec(),
            v => serde_json::to_vec(v)?,
        };
        let document = SourceProjectDocument::read(&bytes)?;
        let index = ProjectIndex::new(&document)?;
        let sequence = id(p.get("sequence").cloned())?
            .or(id(p.pointer("/context/activeSequence").cloned())?)
            .context("need active sequence")?;
        let seq = index
            .item(sequence)
            .and_then(|i| i.pointer("/kind/Sequence"))
            .context("missing sequence")?;
        let clip = masks::target_clip(p, seq)?;
        let (path, _locked) = super::filmcraft_parameter_edit::location(&document, sequence, clip)?;
        let raw = document.envelope.pointer(&path).unwrap();
        let item = SourceTrackItem::read(raw)?;
        let sel = p.pointer("/context/selectedMask").filter(|s| s.is_object());
        let effect = masks::selector(
            raw["effects"].as_array().context("missing effects")?,
            p.get("effect"),
            sel,
            clip,
        )
        .context("no such effect")?;
        let mask = p["mask"]
            .as_u64()
            .or_else(|| {
                sel.filter(|s| {
                    id(s.get("clip").cloned()).ok().flatten() == Some(clip)
                        && s["effect"].as_u64() == Some(effect as u64)
                })
                .and_then(|s| s["mask"].as_u64())
            })
            .context("need mask")?;
        let mask = usize::try_from(mask)?;
        let target = &raw["effects"][effect]["masks"][mask];
        ensure!(target.is_object(), "no such mask");
        let method = p["method"]
            .as_str()
            .unwrap_or_else(|| {
                target["track_method"]
                    .as_str()
                    .unwrap_or("PositionScaleRotation")
            })
            .to_ascii_lowercase()
            .replace([' ', '_', '-', '&', ','], "");
        let method = match method.as_str() {
            "position" => TrackMethod::Position,
            "positionrotation" => TrackMethod::PositionRotation,
            "positionscalerotation" => TrackMethod::PositionScaleRotation,
            _ => anyhow::bail!("unknown tracking method"),
        };
        let backward = match p["direction"].as_str().unwrap_or("forward") {
            "forward" => false,
            "backward" => true,
            _ => anyhow::bail!("direction must be forward or backward"),
        };
        let kind = index
            .item(item.source_id)
            .and_then(|i| i.get("kind"))
            .context("missing tracking source")?;
        let rate = if let Some(m) = kind.get("Media") {
            m.pointer("/interpret/frame_rate")
                .filter(|v| v.is_object())
                .or_else(|| m.pointer("/info/video/frame_rate"))
        } else if let Some(q) = kind.get("Sequence") {
            q.pointer("/settings/frame_rate")
        } else {
            kind.get("Graphic")
                .or_else(|| kind.get("AdjustmentLayer"))
                .and_then(|g| g.get("rate"))
        };
        let rate = rate
            .map(|v| serde_json::from_value::<filmcraft_time::FrameRate>(v.clone()))
            .transpose()?
            .unwrap_or_default();
        let fd = rate.frame_duration().0.max(1);
        let from = masks::media(&item, masks::clock(p, seq)?)?;
        let start = masks::media(&item, item.timing.timeline_start)?;
        let end = masks::media(
            &item,
            item.end()
                .context("clip end overflow")?
                .checked_sub(1)
                .context("clip end underflow")?,
        )?;
        let (lo, hi) = (start.min(end), start.max(end));
        let room = if backward {
            from.checked_sub(lo)
        } else {
            hi.checked_sub(from)
        }
        .context("tracking room overflow")?
            / fd;
        let steps = p["frames"]
            .as_u64()
            .map(|n| i64::try_from(n).context("tracking frame count overflow"))
            .transpose()?
            .unwrap_or(room)
            .min(room)
            .max(0);
        ensure!(steps > 0, "already at tracking boundary");
        let steps = usize::try_from(steps)?;
        let (width, height) = index
            .source_size(item.source_id)
            .context("clip has no picture")?;
        ensure!(width > 0 && height > 0, "invalid source dimensions");
        let name = target["name"]
            .as_str()
            .context("missing mask name")?
            .to_owned();
        let source = item.source_id;
        Ok(Self {
            document,
            sequence,
            clip,
            effect,
            mask,
            parameter_path: format!("{path}/effects/{effect}/masks/{mask}/path"),
            source,
            width,
            height,
            from,
            delta: if backward { -fd } else { fd },
            steps,
            method,
            name,
        })
    }
    pub fn time(&self, step: usize) -> Result<i64> {
        ensure!(step <= self.steps, "tracking step outside plan");
        let offset = i128::from(self.delta) * i128::try_from(step)?;
        Ok(i64::try_from(i128::from(self.from) + offset)?)
    }
}
fn prepare(frame: &GroupSurface) -> Result<Prepared> {
    ensure!(
        frame.width > 0
            && frame.height > 0
            && frame
                .width
                .checked_mul(frame.height)
                .and_then(|v| v.checked_mul(4))
                == Some(frame.rgba.len()),
        "invalid tracking frame"
    );
    Ok(Prepared::new(Gray::from_rgba8(
        frame.width,
        frame.height,
        &frame.rgba,
    )))
}
fn force(parameter: &mut Value, t: i64, value: Value) -> Result<()> {
    let mut keys = parameter
        .get("keyframes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    ensure!(
        keys.iter().all(|k| k["time"].as_i64().is_some()),
        "invalid key time"
    );
    match keys.binary_search_by_key(&t,|k|k["time"].as_i64().unwrap()){Ok(i)=>keys[i]["value"]=value,Err(i)=>keys.insert(i,json!({"time":t,"value":value,"interp":"Linear","in_influence":1./3.,"out_influence":1./3.}))};
    parameter["keyframes"] = json!(keys);
    Ok(())
}
/// Original scale composition and separate linear tangent transform; operation order matters.
pub(super) fn transform(value: &Value, m: Affine) -> Result<Value> {
    let mut value = canonical_parameter(value.clone())?;
    let vertices = value
        .pointer_mut("/Path/vertices")
        .and_then(Value::as_array_mut)
        .context("tracking value is not a path")?;
    for vertex in vertices {
        for field in ["p", "t_in", "t_out"] {
            let p = &vertex[field];
            let q = Vec2::new(p["x"].as_f64().unwrap_or(0.), p["y"].as_f64().unwrap_or(0.));
            let mapped = if field == "p" {
                m.apply(q)
            } else {
                Affine { e: 0., f: 0., ..m }.apply(q)
            };
            vertex[field] = json!({"x":mapped.x,"y":mapped.y});
        }
    }
    canonical_parameter(value)
}
pub(super) fn scaled_transform(
    value: &Value,
    m: Affine,
    previous_scale: f64,
    next_scale: f64,
) -> Result<Value> {
    ensure!(
        previous_scale.is_finite()
            && next_scale.is_finite()
            && previous_scale > 0.
            && next_scale > 0.,
        "invalid analysis scale"
    );
    let m = Affine::scale(1. / next_scale, 1. / next_scale)
        .then_apply(&m)
        .then_apply(&Affine::scale(previous_scale, previous_scale));
    transform(value, m)
}
pub struct Runner {
    pub plan: Plan,
    pub document: SourceProjectDocument,
    pub done: usize,
    pub inliers: usize,
    current: Value,
    previous: Prepared,
    scale: f64,
}
impl Runner {
    /// First decoded frame commits the seed even if a later frame is lost or cancelled.
    pub fn start(plan: Plan, frame: &GroupSurface) -> Result<Self> {
        let previous = prepare(frame)?;
        let mut document = plan.document.clone();
        let parameter = document
            .envelope
            .pointer(&plan.parameter_path)
            .context("tracking parameter missing")?;
        let current = filmcraft_parameter_sample::sample(parameter, plan.from)?;
        let current = if current.get("Path").is_some() {
            current
        } else {
            json!({"Path":{"vertices":[],"closed":false}})
        };
        force(
            document.envelope.pointer_mut(&plan.parameter_path).unwrap(),
            plan.from,
            current.clone(),
        )?;
        let scale = frame.width as f64 / plan.width.max(1) as f64;
        Ok(Self {
            plan,
            document,
            done: 0,
            inliers: 0,
            current,
            previous,
            scale,
        })
    }
    pub fn step(&mut self, frame: &GroupSurface) -> Result<bool> {
        ensure!(self.done < self.plan.steps, "tracking completed");
        let next = prepare(frame)?;
        let next_scale = frame.width as f64 / self.plan.width.max(1) as f64;
        let (_, region) =
            super::filmcraft_source_masks::tracking_path(&json!({"value":self.current}), 0)?;
        let region = region
            .into_iter()
            .map(|p| p * self.scale)
            .collect::<Vec<_>>();
        let Some(step) =
            motion_tracker::track_step(&self.previous, &next, &region, self.plan.method)
        else {
            return Ok(false);
        };
        let transformed = scaled_transform(&self.current, step.transform, self.scale, next_scale)?;
        let time = self.plan.time(self.done + 1)?;
        force(
            self.document
                .envelope
                .pointer_mut(&self.plan.parameter_path)
                .context("tracking parameter missing")?,
            time,
            transformed.clone(),
        )?;
        self.current = transformed;
        self.previous = next;
        self.scale = next_scale;
        self.done += 1;
        self.inliers = step.inliers;
        Ok(true)
    }
}

pub struct Outcome {
    pub document: SourceProjectDocument,
    pub done: usize,
    pub total: usize,
    pub error: Option<String>,
    pub stopped: bool,
}
/// Synchronous driver for an owned frame provider. The async host can use Runner directly.
pub fn execute(
    plan: Plan,
    mut load: impl FnMut(i64) -> Result<GroupSurface>,
    mut cancelled: impl FnMut(usize) -> bool,
) -> Result<Outcome> {
    let first = match load(plan.from) {
        Ok(frame) => frame,
        Err(_) => {
            return Ok(Outcome {
                document: plan.document,
                done: 0,
                total: plan.steps,
                error: Some("can't decode the clip".into()),
                stopped: false,
            })
        }
    };
    let total = plan.steps;
    let mut runner = Runner::start(plan, &first)?;
    let mut error = None;
    let mut stopped = false;
    for step in 1..=total {
        if cancelled(runner.done) {
            stopped = true;
            break;
        }
        let t = runner.plan.time(step)?;
        let frame = match load(t) {
            Ok(frame) => frame,
            Err(_) => {
                error = Some(format!("can't decode frame {step}"));
                break;
            }
        };
        if !runner.step(&frame)? {
            error = Some(format!(
                "lost track after {} frame(s): not enough detail inside the mask",
                runner.done
            ));
            break;
        }
    }
    Ok(Outcome {
        document: runner.document,
        done: runner.done,
        total,
        error,
        stopped,
    })
}
