//! Clip video options from Filmcraft clip_ops.rs (MIT), pinned
//! 5231852443363f001c3f6b396dd9b1e6461ae2be. Snapshot authoring, no decoder admission.
use super::{
    filmcraft_labels::{clip_paths, context_ids},
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::id,
};
use anyhow::{ensure, Context, Result};
use filmcraft_time::{FrameRate, Tick};
use serde_json::{json, Value};
pub const COMMANDS: [&str; 8] = [
    "clip.frameHoldOptions",
    "clip.fieldOptions",
    "clip.timeInterpolation.frameSampling",
    "clip.timeInterpolation.frameBlending",
    "clip.timeInterpolation.opticalFlow",
    "clip.setTimeInterpolation",
    "clip.fitToFrame",
    "clip.fillFrame",
];
fn integer(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str()?.parse().ok())
}
fn add(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b).context("video option clock overflow")
}
fn sub(a: i64, b: i64) -> Result<i64> {
    a.checked_sub(b).context("video option clock overflow")
}
fn rounded(n: f64) -> Result<i64> {
    let n = n.round();
    ensure!(
        n.is_finite() && n >= i64::MIN as f64 && n < 9223372036854775808.,
        "video option clock overflow"
    );
    Ok(n as i64)
}
fn frame_tick(rate: FrameRate, frame: i64) -> Result<i64> {
    let r = rate.sane();
    let n = (frame as i128)
        .checked_mul(254016000000)
        .and_then(|n| n.checked_mul(r.den as i128))
        .context("video frame overflow")?;
    Ok(n.div_euclid(r.num as i128) as i64)
}
fn moving(clip: &Value, time: i64) -> Result<i64> {
    let start = integer(&clip["start"]).context("invalid clip start")?;
    let duration = integer(&clip["duration"]).context("invalid clip duration")?;
    let last = sub(add(start, duration)?, 1)?;
    ensure!(last >= start, "invalid clip duration");
    let time = time.clamp(start, last);
    let speed = clip["speed"].as_f64().context("invalid clip speed")?.abs();
    let rel = rounded(sub(time, start)? as f64 * speed)?;
    let source = integer(&clip["source_in"]).context("invalid source In")?;
    if clip["reverse"].as_bool().unwrap_or(false) {
        sub(
            sub(add(source, rounded(duration as f64 * speed)?)?, rel)?,
            1,
        )
    } else {
        add(source, rel)
    }
}
pub fn edit(command: &str, mut params: Value) -> Result<Value> {
    ensure!(COMMANDS.contains(&command), "unknown video options command");
    let fields = params
        .as_object_mut()
        .context("invalid video option parameters")?;
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
    let q = doc.envelope.pointer(&qpath).context("no active sequence")?;
    let rate: FrameRate = serde_json::from_value(q["settings"]["frame_rate"].clone())?;
    let paths = clip_paths(&doc, Some(active));
    let selection = context_ids(ctx["selection"].as_array().cloned().unwrap_or_default())?;
    let named = params["clips"].as_array().map(|ids| {
        ids.iter()
            .filter_map(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
            .collect::<Vec<_>>()
    });
    ensure!(
        !selection.is_empty()
            || named
                .as_ref()
                .is_some_and(|ids| ids.iter().any(|id| paths.iter().any(|(c, _)| c == id))),
        "no selected clips"
    );
    let scalar = params["clip"]
        .as_u64()
        .or_else(|| params["clip"].as_f64().map(|n| n as u64))
        .or_else(|| params["clip"].as_str()?.parse().ok());
    let targets = named.unwrap_or_else(|| scalar.map(|c| vec![c]).unwrap_or(selection));
    ensure!(targets.len() <= 4096, "too many video option targets");
    let targets: Vec<_> = targets
        .into_iter()
        .filter_map(|id| {
            paths
                .iter()
                .find(|(c, path)| *c == id && path.contains("/video_tracks/"))
                .map(|(_, path)| (id, path.clone()))
        })
        .collect();
    let index = ProjectIndex::new(&doc)?;
    let sizes: std::collections::BTreeMap<_, _> = targets
        .iter()
        .filter_map(|(id, path)| {
            index
                .source_size(doc.envelope.pointer(path)?["item"].as_u64()?)
                .map(|size| (*id, size))
        })
        .collect();
    let rates = targets
        .iter()
        .filter(|_| command == "clip.frameHoldOptions")
        .map(|(id, path)| {
            let item = doc.envelope.pointer(path).unwrap()["item"]
                .as_u64()
                .context("invalid clip item")?;
            let rate = super::filmcraft_subclip::item_rate(&index, item)?;
            let tc = index
                .resolve_media(item)
                .and_then(|root| {
                    root.media
                        .pointer("/info/start_timecode")
                        .and_then(Value::as_i64)
                })
                .unwrap_or(0);
            Ok((*id, (rate, tc)))
        })
        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
    let ph = if let Some(v) = ctx.get("playhead") {
        integer(v).context("invalid program clock")?
    } else {
        frame_tick(
            rate,
            ctx.get("playheadFrames").and_then(integer).unwrap_or(0),
        )?
    };
    let fit = matches!(command, "clip.fitToFrame" | "clip.fillFrame");
    ensure!(
        if fit {
            !sizes.is_empty()
        } else {
            !targets.is_empty()
        },
        "select a video clip"
    );
    let mode = if command == "clip.setTimeInterpolation" {
        let raw = params["mode"].as_str().context("need interpolation mode")?;
        ["frameSampling", "frameBlending", "opticalFlow"]
            .into_iter()
            .zip(["Frame Sampling", "Frame Blending", "Optical Flow"])
            .find(|(name, label)| raw.eq_ignore_ascii_case(name) || raw.eq_ignore_ascii_case(label))
            .map(|(name, _)| name)
            .context("invalid interpolation mode")?
    } else {
        command
            .strip_prefix("clip.timeInterpolation.")
            .unwrap_or("")
    };
    let processing = params["processing"].as_str().unwrap_or("none");
    if command == "clip.fieldOptions" {
        ensure!(
            ["none", "alwaysDeinterlace", "flickerRemoval"].contains(&processing),
            "unknown field processing"
        )
    }
    let (fw, fh) = (
        q["settings"]["width"]
            .as_u64()
            .context("invalid frame width")? as f64,
        q["settings"]["height"]
            .as_u64()
            .context("invalid frame height")? as f64,
    );
    let enabled = params["enabled"].as_bool().unwrap_or(true);
    let hold_filters = params["holdFilters"].as_bool().unwrap_or(false);
    let hold_mode = params["holdOn"].as_str().unwrap_or("in");
    let clock = super::filmcraft_export_command_range::Clock {
        rate,
        drop_frame: q["settings"]["drop_frame"].as_bool().unwrap_or(false),
        playhead: Tick(ph),
    };
    let sequence_time = if command == "clip.frameHoldOptions" {
        clock.time(&json!({"Time":params["time"],"Frame":params["frame"],"Seconds":params["seconds"],"Timecode":params["timecode"]}),"")?
    } else {
        None
    };
    let mut out = vec![];
    for (id, path) in &targets {
        let clip = doc.envelope.pointer_mut(path).unwrap();
        if fit {
            let Some((w, h)) = sizes.get(id) else {
                continue;
            };
            let (sx, sy) = (fw / (*w).max(1) as f64, fh / (*h).max(1) as f64);
            let pct = if command == "clip.fillFrame" {
                sx.max(sy)
            } else {
                sx.min(sy)
            } * 100.;
            clip["scale_to_frame"] = json!(false);
            let Some(motion) = clip["effects"]
                .as_array_mut()
                .and_then(|effects| effects.iter_mut().find(|e| e["effect"] == "motion"))
            else {
                continue;
            };
            for (key, value) in [
                ("scale", json!({"Float":pct})),
                ("uniform_scale", json!({"Bool":true})),
                ("position", json!({"Vec2":{"x":fw/2.,"y":fh/2.}})),
                (
                    "anchor",
                    json!({"Vec2":{"x":*w as f64/2.,"y":*h as f64/2.}}),
                ),
            ] {
                if let Some(param) = motion["params"].get_mut(key) {
                    param["value"] = value;
                    param
                        .as_object_mut()
                        .context("invalid effect parameter")?
                        .remove("keyframes");
                } else {
                    motion["params"][key] = json!({"value":value});
                }
            }
            out.push(json!({"clip":id.to_string(),"scale":pct}));
        } else if command == "clip.fieldOptions" {
            let reverse = params["reverseFieldDominance"].as_bool().unwrap_or(false);
            if processing == "none" && !reverse {
                clip.as_object_mut().unwrap().remove("field_options");
            } else {
                clip["field_options"] =
                    json!({"reverseFieldDominance":reverse,"processing":processing});
            }
        } else if !mode.is_empty() {
            if mode == "frameSampling" {
                clip.as_object_mut().unwrap().remove("time_interpolation");
            } else {
                clip["time_interpolation"] = json!(mode);
            }
        } else {
            if !enabled {
                clip["frame_hold"] = Value::Null;
                clip.as_object_mut().unwrap().remove("hold_filters");
                continue;
            }
            let start = integer(&clip["start"]).context("invalid clip start")?;
            let last = sub(
                add(
                    start,
                    integer(&clip["duration"]).context("invalid duration")?,
                )?,
                1,
            )?;
            let (r, tc) = rates[id];
            let hold = match hold_mode {
                "in" => moving(clip, start)?,
                "out" => moving(clip, last)?,
                "playhead" => moving(clip, ph)?,
                "sequenceTime" => moving(
                    clip,
                    sequence_time.context("sequenceTime needs time/timecode")?,
                )?,
                "sourceTimecode" => {
                    if let Some(text) = params["timecode"].as_str() {
                        let f = filmcraft_time::parse_timecode(text, r, false, 0)
                            .map_err(|e| anyhow::anyhow!("invalid source timecode: {e:?}"))?;
                        frame_tick(r, sub(f, tc)?)?
                    } else {
                        integer(&params["time"]).context("sourceTimecode needs timecode or time")?
                    }
                }
                _ => anyhow::bail!("unknown holdOn"),
            };
            let hold = frame_tick(r, r.frame_at(Tick(hold.max(0))))?;
            clip["frame_hold"] = json!(hold);
            if hold_filters {
                clip["hold_filters"] = json!(true)
            } else {
                clip.as_object_mut().unwrap().remove("hold_filters");
            }
            out.push(json!({"clip":id.to_string(),"hold":hold.to_string()}));
        }
    }
    super::filmcraft_marks::check_sequence(doc.envelope.pointer(&qpath).unwrap())?;
    let result = if fit || command == "clip.frameHoldOptions" {
        json!({"clips":out})
    } else if command == "clip.fieldOptions" {
        json!({"clips":targets.len()})
    } else {
        json!({"mode":mode,"clips":targets.len()})
    };
    let text = String::from_utf8(doc.encode_preserved()?)?;
    Ok(
        json!({"result":result,"document":doc.envelope,"documentText":text,"videoOptionsEdit":true,"sequence":active.to_string()}),
    )
}
