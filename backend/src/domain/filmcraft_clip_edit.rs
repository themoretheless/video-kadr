//! Timeline clip operations from Filmcraft commands.rs / clip_ops.rs (MIT), pinned
//! 5231852443363f001c3f6b396dd9b1e6461ae2be. Atomic source-document snapshots.
use super::{
    filmcraft_labels::context_ids,
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_inspect::id,
    filmcraft_source_edit::Alloc,
    filmcraft_trim::{clip_location, follow_transition_cuts},
};
use anyhow::{ensure, Context, Result};
use filmcraft_time::{FrameRate, Tick};
use serde_json::{json, Value};
pub const COMMANDS: [&str; 8] = [
    "clip.speedDuration",
    "clip.enable",
    "clip.link",
    "clip.group",
    "clip.ungroup",
    "clip.scaleToFrameSize",
    "clip.frameHold",
    "clip.insertFrameHoldSegment",
];
fn scalar(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_f64().map(|f| f as u64))
        .or_else(|| v.as_str()?.parse().ok())
}
fn integer(v: &Value) -> Result<i64> {
    v.as_i64()
        .or_else(|| v.as_str()?.parse().ok())
        .context("invalid clip edit clock")
}
fn add(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b).context("clip edit clock overflow")
}
fn sub(a: i64, b: i64) -> Result<i64> {
    a.checked_sub(b).context("clip edit clock overflow")
}
fn rounded(v: f64) -> Result<i64> {
    let v = v.round();
    ensure!(
        v.is_finite() && v >= i64::MIN as f64 && v < 9223372036854775808.,
        "clip source clock overflow"
    );
    Ok(v as i64)
}
fn frame_tick(r: FrameRate, f: i64) -> Result<i64> {
    let r = r.sane();
    let n = (f as i128)
        .checked_mul(254016000000)
        .and_then(|n| n.checked_mul(r.den as i128))
        .context("clip frame clock overflow")?;
    Ok(n.div_euclid(r.num as i128) as i64)
}
pub(super) fn source_at(clip: &Value, t: i64) -> Result<i64> {
    let speed = clip["speed"].as_f64().context("invalid clip speed")?.abs();
    let delta = rounded(sub(t, integer(&clip["start"])?)? as f64 * speed)?;
    let source = integer(&clip["source_in"])?;
    if clip["reverse"].as_bool().unwrap_or(false) {
        sub(
            sub(
                add(source, rounded(integer(&clip["duration"])? as f64 * speed)?)?,
                delta,
            )?,
            1,
        )
    } else {
        add(source, delta)
    }
}
fn strings(ids: &[u64]) -> Vec<String> {
    ids.iter().map(u64::to_string).collect()
}
fn all(q: &Value) -> impl Iterator<Item = &Value> {
    ["video_tracks", "audio_tracks"]
        .into_iter()
        .flat_map(|key| q[key].as_array().into_iter().flatten())
        .flat_map(|t| t["items"].as_array().into_iter().flatten())
}
fn item(q: &Value, id: u64) -> Option<&Value> {
    let (k, t, c) = clip_location(q, id).ok()?;
    Some(&q[k][t]["items"][c])
}
fn item_mut(q: &mut Value, id: u64) -> Option<&mut Value> {
    let (k, t, c) = clip_location(q, id).ok()?;
    Some(&mut q[k][t]["items"][c])
}
fn with_links(q: &Value, mut ids: Vec<u64>, linked: bool) -> Vec<u64> {
    if linked {
        let links: Vec<_> = ids
            .iter()
            .filter_map(|id| item(q, *id)?["link"].as_u64())
            .collect();
        for it in all(q) {
            if it["link"].as_u64().is_some_and(|l| links.contains(&l)) {
                if let Some(id) = it["id"].as_u64() {
                    if !ids.contains(&id) {
                        ids.push(id)
                    }
                }
            }
        }
    }
    ids
}
fn speed_group(
    q: &mut Value,
    ids: &[u64],
    speed: f64,
    reverse: bool,
    ripple: bool,
    min: i64,
) -> Result<()> {
    ensure!(speed.is_finite() && speed > 0., "speed must be positive");
    let before = q.clone();
    let mut origins = vec![];
    for c in ids {
        let (k, t, i) = clip_location(q, *c)?;
        let track = &q[k][t];
        let tid = track["id"].as_u64().context("invalid track ID")?;
        ensure!(
            !origins.iter().any(|(id, _, _)| *id == tid),
            "clips on one track change speed one at a time"
        );
        let it = &track["items"][i];
        let start = integer(&it["start"])?;
        let duration = integer(&it["duration"])?;
        let consumed =
            rounded(duration as f64 * it["speed"].as_f64().context("invalid old speed")?.abs())?;
        add(integer(&it["source_in"])?, consumed)?;
        let room = Tick::MAX.0.saturating_sub(start.max(0));
        let mut next = ((consumed as f64 / speed).round() as i64)
            .min(room)
            .max(min);
        let old_end = add(start, duration)?;
        if !ripple {
            let next_start = track["items"]
                .as_array()
                .unwrap()
                .get(i + 1)
                .map(|it| integer(&it["start"]))
                .transpose()?
                .unwrap_or(Tick::MAX.0);
            next = next.min(sub(next_start, start)?)
        }
        let delta = sub(next, duration)?;
        let it = &mut q[k][t]["items"][i];
        it["speed"] = json!(speed);
        it["reverse"] = json!(reverse);
        it["duration"] = json!(next);
        origins.push((tid, old_end, delta));
    }
    let from = origins.first().map(|o| o.1).unwrap_or(0);
    let shift = origins.iter().map(|o| o.2).max().unwrap_or(0);
    if ripple && shift != 0 {
        for key in ["video_tracks", "audio_tracks"] {
            for tr in q[key].as_array_mut().into_iter().flatten() {
                if tr["locked"].as_bool().unwrap_or(false) {
                    continue;
                }
                let tid = tr["id"].as_u64().context("invalid track ID")?;
                let member = origins.iter().find(|o| o.0 == tid).map(|o| o.1);
                if member.is_none() && !tr["sync_lock"].as_bool().unwrap_or(false) {
                    continue;
                }
                let from = member.unwrap_or(from);
                for it in tr["items"].as_array_mut().context("invalid speed track")? {
                    if !ids.contains(&it["id"].as_u64().context("invalid clip ID")?)
                        && integer(&it["start"])? >= from
                    {
                        let next = add(integer(&it["start"])?, shift)?;
                        add(next, integer(&it["duration"])?)?;
                        it["start"] = json!(next);
                    }
                }
                tr["items"]
                    .as_array_mut()
                    .unwrap()
                    .sort_by_key(|i| i["start"].as_i64().unwrap_or(0));
                if let Some(transitions) = tr["transitions"].as_array_mut() {
                    transitions.sort_by_key(|i| i["start"].as_i64().unwrap_or(0));
                }
            }
        }
    }
    if ripple {
        follow_transition_cuts(&before, q)?
    }
    super::filmcraft_marks::check_sequence(q)?;
    Ok(())
}
pub fn edit(command: &str, mut params: Value) -> Result<Value> {
    ensure!(COMMANDS.contains(&command), "unknown clip edit command");
    let fields = params
        .as_object_mut()
        .context("invalid clip edit parameters")?;
    let raw = fields.remove("document").context("missing document")?;
    let ctx = fields.remove("context").unwrap_or_else(|| json!({}));
    let active = id(fields.remove("sequence"))?
        .or(id(ctx.get("activeSequence").cloned())?)
        .context("no active sequence")?;
    let bytes = match raw {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let mut doc = SourceProjectDocument::read(&bytes)?;
    let qpath = format!("/project/items/{active}/kind/Sequence");
    let mut q = doc
        .envelope
        .pointer(&qpath)
        .context("no active sequence")?
        .clone();
    let rate: FrameRate = serde_json::from_value(q["settings"]["frame_rate"].clone())?;
    let selection = context_ids(ctx["selection"].as_array().cloned().unwrap_or_default())?;
    let named = params["clips"].as_array().map(|a| {
        a.iter()
            .filter_map(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
            .collect::<Vec<_>>()
    });
    let plain = matches!(
        command,
        "clip.group" | "clip.ungroup" | "clip.scaleToFrameSize"
    );
    let hold = matches!(command, "clip.frameHold" | "clip.insertFrameHoldSegment");
    ensure!(
        hold || !selection.is_empty()
            || !plain
                && named
                    .as_ref()
                    .is_some_and(|ids| ids.iter().any(|id| item(&q, *id).is_some())),
        "no selected clips"
    );
    let mut targets = if plain {
        selection.clone()
    } else {
        named.unwrap_or_else(|| {
            scalar(&params["clip"])
                .map(|id| vec![id])
                .unwrap_or_else(|| selection.clone())
        })
    };
    ensure!(targets.len() <= 4096, "too many clip edit targets");
    if matches!(command, "clip.speedDuration" | "clip.enable") {
        targets = with_links(
            &q,
            targets,
            ctx["linkedSelection"].as_bool().unwrap_or(true),
        )
    }
    let mut alloc = Alloc::new(&doc)?;
    let mut after_selection = selection.clone();
    let result = if hold {
        let ph = if let Some(v) = ctx.get("playhead") {
            integer(v)?
        } else {
            frame_tick(
                rate,
                ctx.get("playheadFrames")
                    .map(integer)
                    .transpose()?
                    .unwrap_or(0),
            )?
        };
        let clock = super::filmcraft_export_command_range::Clock {
            rate,
            drop_frame: q["settings"]["drop_frame"].as_bool().unwrap_or(false),
            playhead: Tick(ph),
        };
        let t=clock.time(&json!({"Time":params["time"],"Frame":params["frame"],"Seconds":params["seconds"],"Timecode":params["timecode"]}),"")?.unwrap_or(ph);
        let targeted = ctx["targetedTrackIds"]
            .as_array()
            .map(|a| context_ids(a.clone()))
            .transpose()?;
        let hit = |id: u64| {
            clip_location(&q, id)
                .ok()
                .filter(|(key, _, _)| *key == "video_tracks")
                .filter(|(k, ti, ci)| {
                    let it = &q[*k][*ti]["items"][*ci];
                    integer(&it["start"])
                        .ok()
                        .zip(integer(&it["duration"]).ok())
                        .is_some_and(|(s, d)| s <= t && s.checked_add(d).is_some_and(|e| t < e))
                })
        };
        let fallback = || {
            q["video_tracks"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|tr| {
                    !tr["locked"].as_bool().unwrap_or(false)
                        && targeted.as_ref().map_or(true, |ids| {
                            tr["id"].as_u64().is_some_and(|id| ids.contains(&id))
                        })
                })
                .find_map(|tr| {
                    let items = tr["items"].as_array()?;
                    let at =
                        items.partition_point(|it| it["start"].as_i64().unwrap_or(i64::MAX) <= t);
                    let it = at.checked_sub(1).and_then(|i| items.get(i))?;
                    let id = it["id"].as_u64()?;
                    hit(id).map(|_| id)
                })
        };
        if command == "clip.frameHold" {
            let mut clips: Vec<_> = targets
                .into_iter()
                .filter(|id| hit(*id).is_some())
                .collect();
            if clips.is_empty() && params.get("clips").is_none() && params.get("clip").is_none() {
                if let Some(id) = fallback() {
                    clips.push(id)
                }
            }
            ensure!(
                !clips.is_empty(),
                "place the playhead over a selected video clip"
            );
            let mut out = vec![];
            for c in clips {
                let current = doc.envelope.pointer(&qpath).unwrap();
                let Some(it) = item(current, c) else { continue };
                let held = source_at(it, t)?;
                let start = integer(&it["start"])?;
                let target = if start < t {
                    super::filmcraft_trim::razor(&mut doc, active, &[], Some(&[c]), t)?
                        .first()
                        .copied()
                } else {
                    Some(c)
                };
                if let Some(id) = target {
                    if let Some(it) = item_mut(doc.envelope.pointer_mut(&qpath).unwrap(), id) {
                        it["frame_hold"] = json!(held);
                        out.push(id);
                    }
                }
            }
            after_selection = out.clone();
            json!({"clips":strings(&out)})
        } else {
            let explicit = scalar(&params["clip"]);
            let chosen = explicit
                .filter(|id| hit(*id).is_some())
                .or_else(|| selection.iter().copied().find(|id| hit(*id).is_some()))
                .or_else(|| if explicit.is_none() { fallback() } else { None })
                .context("place the playhead over a video clip")?;
            let (k, ti, ci) = clip_location(&q, chosen)?;
            let track = q[k][ti]["id"].as_u64().context("invalid insertion track")?;
            let mut segment = q[k][ti]["items"][ci].clone();
            let held = source_at(&segment, t)?;
            let seconds = params["seconds"].as_f64().unwrap_or(2.);
            let ticks = rounded(seconds * 254016000000.)?;
            let r = rate.sane();
            let a = frame_tick(r, r.frame_at(Tick(ticks)))?;
            let b = frame_tick(
                r,
                r.frame_at(Tick(ticks))
                    .checked_add(1)
                    .context("hold segment duration overflow")?,
            )?;
            let duration = if sub(ticks, a)? <= sub(b, ticks)? {
                a
            } else {
                b
            };
            let duration = duration.max(frame_tick(rate, 1)?);
            let id = alloc.get()?;
            doc.envelope["project"]["next_id"] = json!(alloc.next);
            segment["id"] = json!(id);
            segment["start"] = json!(t);
            segment["duration"] = json!(duration);
            segment["source_in"] = json!(held);
            segment["frame_hold"] = json!(held);
            segment["link"] = Value::Null;
            segment["markers"] = json!([]);
            after_selection =
                super::filmcraft_trim::place_clips(&mut doc, active, &[(track, segment)], true)?;
            json!({"clip":id.to_string(),"duration":duration.to_string()})
        }
    } else {
        let result = match command {
            "clip.speedDuration" => {
                let speed = params["speed"].as_f64().unwrap_or(100.) / 100.;
                let reverse = params["reverse"].as_bool().unwrap_or(false);
                let ripple = params["ripple"].as_bool().unwrap_or(false);
                let interpolation = if let Some(m) = params["interpolation"].as_str() {
                    Some(
                        ["frameSampling", "frameBlending", "opticalFlow"]
                            .into_iter()
                            .zip(["Frame Sampling", "Frame Blending", "Optical Flow"])
                            .find(|(a, b)| m.eq_ignore_ascii_case(a) || m.eq_ignore_ascii_case(b))
                            .map(|(m, _)| m)
                            .context("unknown interpolation")?,
                    )
                } else {
                    None
                };
                let place = |id: u64| {
                    clip_location(&q, id).ok().and_then(|(k, t, c)| {
                        Some((
                            q[k][t]["id"].as_u64()?,
                            q[k][t]["items"][c]["link"].as_u64(),
                        ))
                    })
                };
                let mut groups: Vec<Vec<u64>> = vec![];
                for c in &targets {
                    let Some((track, link)) = place(*c) else {
                        groups.push(vec![*c]);
                        continue;
                    };
                    if let Some(group) = groups.iter_mut().find(|group| {
                        link.is_some()
                            && group
                                .iter()
                                .all(|id| place(*id).is_some_and(|(t, l)| l == link && t != track))
                    }) {
                        group.push(*c)
                    } else {
                        groups.push(vec![*c])
                    }
                }
                for group in groups {
                    speed_group(&mut q, &group, speed, reverse, ripple, frame_tick(rate, 1)?)?
                }
                if let Some(mode) = interpolation {
                    for c in &targets {
                        if let Some(it) = item_mut(&mut q, *c) {
                            if mode == "frameSampling" {
                                it.as_object_mut().unwrap().remove("time_interpolation");
                            } else {
                                it["time_interpolation"] = json!(mode);
                            }
                        }
                    }
                }
                Value::Null
            }
            "clip.enable" => {
                let enabled = !targets
                    .iter()
                    .all(|id| item(&q, *id).is_some_and(|it| it["enabled"] == true));
                for c in &targets {
                    if let Some(it) = item_mut(&mut q, *c) {
                        it["enabled"] = json!(enabled)
                    }
                }
                Value::Null
            }
            "clip.link" => {
                let linked = targets
                    .iter()
                    .all(|id| item(&q, *id).is_some_and(|it| it["link"].as_u64().is_some()));
                let link = if linked { None } else { Some(alloc.get()?) };
                for c in &targets {
                    if let Some(it) = item_mut(&mut q, *c) {
                        it["link"] = json!(link)
                    }
                }
                json!({"linked":!linked})
            }
            "clip.group" => {
                let group = alloc.get()?;
                for c in &targets {
                    if let Some(it) = item_mut(&mut q, *c) {
                        it["group"] = json!(group)
                    }
                }
                Value::Null
            }
            "clip.ungroup" => {
                for c in &targets {
                    if let Some(it) = item_mut(&mut q, *c) {
                        it["group"] = Value::Null;
                    }
                }
                Value::Null
            }
            _ => {
                for c in &targets {
                    if let Some(it) = item_mut(&mut q, *c) {
                        it["scale_to_frame"] =
                            json!(!it["scale_to_frame"].as_bool().unwrap_or(false));
                    }
                }
                Value::Null
            }
        };
        *doc.envelope.pointer_mut(&qpath).unwrap() = q;
        doc.envelope["project"]["next_id"] = json!(alloc.next);
        result
    };
    let next = doc.envelope["project"]["next_id"]
        .as_u64()
        .context("invalid next ID")?;
    while alloc.next < next {
        alloc.get()?;
    }
    super::filmcraft_marks::check_sequence(doc.envelope.pointer(&qpath).unwrap())?;
    let text = String::from_utf8(doc.encode_preserved()?)?;
    Ok(
        json!({"result":result,"clipEdit":true,"context":{"selection":strings(&after_selection)},"document":doc.envelope,"documentText":text}),
    )
}
