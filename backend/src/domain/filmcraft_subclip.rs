//! Subclip authoring adapted from Filmcraft clip_ops.rs (MIT), pinned
//! 5231852443363f001c3f6b396dd9b1e6461ae2be. Exact snapshot/context protocol.
use super::{
    filmcraft_labels::{clip_paths, context_ids},
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::id,
    filmcraft_source_edit::Alloc,
};
use anyhow::{ensure, Context, Result};
use filmcraft_time::{FrameRate, Tick};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct SessionContext {
    active_sequence: Option<Value>,
    source_item: Option<Value>,
    selection: Vec<Value>,
    project_selection: Vec<Value>,
    still_image_duration: Option<f64>,
    still_image_unit: Option<String>,
}
struct State {
    active: Option<u64>,
    source: Option<u64>,
    selection: Vec<u64>,
    project: Vec<u64>,
    still_duration: f64,
    still_unit: String,
}
fn read(mut params: Value) -> Result<(SourceProjectDocument, State, Value)> {
    let fields = params
        .as_object_mut()
        .context("expected subclip parameters")?;
    let doc = fields
        .remove("document")
        .context("missing subclip document")?;
    let ctx: SessionContext = fields
        .remove("context")
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();
    let bytes = match doc {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    Ok((
        SourceProjectDocument::read(&bytes)?,
        State {
            active: id(ctx.active_sequence)?,
            source: id(ctx.source_item)?,
            selection: context_ids(ctx.selection)?,
            project: context_ids(ctx.project_selection)?,
            still_duration: ctx.still_image_duration.unwrap_or(5.),
            still_unit: ctx.still_image_unit.unwrap_or_else(|| "seconds".into()),
        },
        params,
    ))
}
fn scalar(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_f64().map(|n| n as u64))
        .or_else(|| v.as_str()?.parse().ok())
}
fn integer(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str()?.parse().ok())
}
fn add(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b).context("subclip clock overflow")
}
fn sub(a: i64, b: i64) -> Result<i64> {
    a.checked_sub(b).context("subclip clock overflow")
}
fn frame_tick(rate: FrameRate, frame: i64) -> Result<i64> {
    let r = rate.sane();
    let ticks = (frame as i128)
        .checked_mul(254016000000)
        .and_then(|t| t.checked_mul(r.den as i128))
        .context("subclip frame clock overflow")?;
    Ok(ticks.div_euclid(r.num as i128) as i64)
}
pub(super) fn item_rate(index: &ProjectIndex<'_>, item: u64) -> Result<FrameRate> {
    if let Some(root) = index.resolve_media(item) {
        return super::filmcraft_source_inspect::frame_rate(index, root.item_id);
    }
    let raw = index.item(item).and_then(|item| {
        item.pointer("/kind/Sequence/settings/frame_rate")
            .or_else(|| item.pointer("/kind/Graphic/rate"))
            .or_else(|| item.pointer("/kind/AdjustmentLayer/rate"))
    });
    let r: FrameRate = raw
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()?
        .unwrap_or_default();
    ensure!(
        r.num > 0 && r.den > 0 && r.num <= u32::MAX as i64 && r.den <= u32::MAX as i64,
        "invalid subclip rate"
    );
    Ok(r)
}
fn still(state: &State, rate: FrameRate) -> Result<i64> {
    ensure!(state.still_duration.is_finite(), "invalid still duration");
    let ticks = if state.still_unit == "frames" {
        frame_tick(rate, state.still_duration.max(1.).round() as i64)?
    } else {
        ensure!(state.still_unit == "seconds", "invalid still duration unit");
        ensure!(
            state.still_duration.max(0.) * 254016000000. <= Tick::MAX.0 as f64,
            "still duration exceeds clock"
        );
        rate.snap_nearest(Tick::from_seconds_f64(state.still_duration.max(0.)))
            .0
    };
    Ok(ticks.max(rate.frame_duration().0))
}
fn sub_source(index: &ProjectIndex<'_>, state: &State) -> Option<u64> {
    state
        .source
        .filter(|id| index.resolve_media(*id).is_some())
        .or_else(|| {
            state
                .project
                .iter()
                .copied()
                .find(|id| index.resolve_media(*id).is_some())
        })
}
struct Plan {
    parent: u64,
    start: i64,
    end: i64,
    name: String,
    label: Value,
}
fn make_plan(
    doc: &SourceProjectDocument,
    index: &ProjectIndex<'_>,
    state: &State,
    p: &Value,
) -> Result<Plan> {
    let item = scalar(&p["item"]);
    let clip = scalar(&p["clip"]).or_else(|| {
        if item.is_some() || sub_source(index, state).is_some() {
            None
        } else {
            state.selection.first().copied()
        }
    });
    if let Some(clip) = clip {
        let sequence = state.active.context("no sequence is open")?;
        let index_clip = clip_paths(doc, Some(sequence));
        let path = index_clip
            .iter()
            .find(|(id, _)| *id == clip)
            .map(|(_, p)| p)
            .context("no such clip")?;
        let clip = doc.envelope.pointer(path).context("missing source clip")?;
        let root = index
            .resolve_media(clip["item"].as_u64().context("invalid clip item")?)
            .context("only media clips can be subclipped")?;
        let start = integer(&clip["source_in"]).context("invalid clip source time")?;
        let duration = integer(&clip["duration"]).context("invalid clip duration")?;
        let speed = clip["speed"].as_f64().context("invalid clip speed")?.abs();
        ensure!(speed.is_finite(), "invalid clip speed");
        let end = add(start, (duration as f64 * speed).round() as i64)?;
        Ok(Plan {
            parent: root.item_id,
            start,
            end: end.max(start),
            name: clip["name"].as_str().context("invalid clip name")?.into(),
            label: clip["label"].clone(),
        })
    } else {
        let item = item
            .or_else(|| sub_source(index, state))
            .context("open a clip in the Source monitor")?;
        let root = index
            .resolve_media(item)
            .context("only media clips can be subclipped")?;
        let pi = index.item(item).context("no such item")?;
        let rate = item_rate(index, root.item_id)?;
        let still = still(state, rate)?;
        let (start, end) = if let Some(range) = root.range {
            let a = integer(&range["start"]).context("invalid subclip range")?;
            (
                a,
                add(
                    a,
                    integer(&range["duration"]).context("invalid subclip range")?,
                )?,
            )
        } else {
            let m = root.media;
            let duration = integer(&m["info"]["duration"]).context("invalid source duration")?;
            let full = if duration > 0 && m["info"]["kind"] != "Still" {
                duration
            } else {
                still
            };
            let a = m["mark_in"].as_i64().unwrap_or(0);
            let b = m["mark_out"]
                .as_i64()
                .map(|v| add(v, rate.frame_duration().0))
                .transpose()?
                .unwrap_or(full);
            (a, b.max(a))
        };
        Ok(Plan {
            parent: root.item_id,
            start,
            end,
            name: pi["name"].as_str().context("invalid source name")?.into(),
            label: pi["label"].clone(),
        })
    }
}
fn edit_item(index: &ProjectIndex<'_>, state: &State, p: &Value) -> Result<u64> {
    scalar(&p["item"])
        .or_else(|| {
            state.project.iter().copied().find(|id| {
                index
                    .item(*id)
                    .is_some_and(|item| item.pointer("/kind/Subclip").is_some())
            })
        })
        .context("select a subclip")
}
pub(super) fn parent_path(bin: &Value, path: &str, item: u64) -> Option<String> {
    for (i, child) in bin["children"].as_array()?.iter().enumerate() {
        if child["Item"].as_u64() == Some(item) {
            return Some(path.into());
        }
        if let Some(b) = child.get("Bin") {
            if let Some(found) = parent_path(b, &format!("{path}/children/{i}/Bin"), item) {
                return Some(found);
            }
        }
    }
    None
}
fn overridden(
    p: &Value,
    key: &str,
    frame_key: &str,
    rate: FrameRate,
    fallback: i64,
) -> Result<i64> {
    if let Some(t) = integer(&p[key]) {
        Ok(t)
    } else if let Some(frame) = integer(&p[frame_key]) {
        frame_tick(rate, frame)
    } else {
        Ok(fallback)
    }
}
pub fn defaults(params: Value) -> Result<Value> {
    let (doc, state, p) = read(params)?;
    let index = ProjectIndex::new(&doc)?;
    let editing = p["forEdit"].as_bool().unwrap_or(false);
    let (parent, start, end, name, restrict) = if editing {
        let item = edit_item(&index, &state, &p)?;
        let pi = index.item(item).context("no such subclip")?;
        let subclip = pi.pointer("/kind/Subclip").context("not a subclip")?;
        let start = integer(&subclip["range"]["start"]).context("invalid subclip range")?;
        (
            subclip["parent"]
                .as_u64()
                .context("invalid subclip parent")?,
            start,
            add(
                start,
                integer(&subclip["range"]["duration"]).context("invalid subclip range")?,
            )?,
            pi["name"]
                .as_str()
                .context("invalid source name")?
                .to_string(),
            subclip["restrict_trims"].as_bool().unwrap_or(false),
        )
    } else {
        let plan = make_plan(&doc, &index, &state, &p)?;
        (
            plan.parent,
            plan.start,
            plan.end,
            format!("{}.Subclip", plan.name),
            true,
        )
    };
    let rate = item_rate(&index, parent)?;
    Ok(
        json!({"name":name,"parentId":parent.to_string(),"startTicks":start.to_string(),"endTicks":end.to_string(),"startFrame":rate.frame_at(Tick(start)).to_string(),"endFrame":rate.frame_at(Tick(end)).to_string(),"fps":rate.as_f64(),"frameRate":{"num":rate.num,"den":rate.den},"restrictTrims":restrict}),
    )
}
pub fn edit(command: &str, params: Value) -> Result<Value> {
    ensure!(
        matches!(command, "clip.makeSubclip" | "clip.editSubclip"),
        "unsupported subclip command"
    );
    let (mut doc, mut state, p) = read(params)?;
    let index = ProjectIndex::new(&doc)?;
    let created = command == "clip.makeSubclip";
    let known_clip = scalar(&p["clip"]).is_some_and(|id| {
        clip_paths(&doc, state.active)
            .iter()
            .any(|(clip, _)| *clip == id)
    });
    let known_item = scalar(&p["item"]);
    if created {
        ensure!(
            sub_source(&index, &state).is_some()
                || !state.selection.is_empty()
                || known_clip
                || known_item.is_some_and(|id| index.resolve_media(id).is_some()),
            "open a clip in the Source monitor or select one"
        );
    } else {
        ensure!(
            state
                .project
                .iter()
                .copied()
                .chain(known_item)
                .any(|id| index
                    .item(id)
                    .is_some_and(|item| item.pointer("/kind/Subclip").is_some())),
            "select a subclip in the Project panel"
        );
    }
    let (item, parent, old_a, old_b, base, label, old_restrict) = if created {
        let plan = make_plan(&doc, &index, &state, &p)?;
        (
            None,
            plan.parent,
            plan.start,
            plan.end,
            plan.name,
            plan.label,
            true,
        )
    } else {
        let item = edit_item(&index, &state, &p)?;
        let pi = index.item(item).context("not a subclip")?;
        let sc = pi.pointer("/kind/Subclip").context("not a subclip")?;
        let a = integer(&sc["range"]["start"]).context("invalid subclip start")?;
        (
            Some(item),
            sc["parent"].as_u64().context("invalid subclip parent")?,
            a,
            add(
                a,
                integer(&sc["range"]["duration"]).context("invalid subclip duration")?,
            )?,
            pi["name"].as_str().context("invalid subclip name")?.into(),
            pi["label"].clone(),
            sc["restrict_trims"].as_bool().unwrap_or(false),
        )
    };
    let rate = item_rate(&index, parent)?;
    let a = overridden(&p, "start", "startFrame", rate, old_a)?;
    let b = overridden(&p, "end", "endFrame", rate, old_b)?;
    ensure!(a >= 0 && b > a, "the subclip's end must be after its start");
    let restrict = p["restrictTrims"].as_bool().unwrap_or(old_restrict);
    let converted = !created && p["convertToMaster"].as_bool().unwrap_or(false);
    let item = if created {
        // AddItem intentionally resets metadata and split points while retaining planned label/name.
        let mut alloc = Alloc::new(&doc)?;
        let id = alloc.get()?;
        let name = p["name"]
            .as_str()
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{base}.Subclip"));
        let bin = parent_path(&doc.envelope["project"]["root"], "/project/root", parent)
            .unwrap_or_else(|| "/project/root".into());
        super::filmcraft_bin_rename::locations(&doc)?;
        doc.envelope["project"]["items"][id.to_string()] = json!({"id":id,"name":name,"label":label,"kind":{"Subclip":{"parent":parent,"range":{"start":a,"duration":sub(b,a)?},"restrict_trims":restrict}},"metadata":{},"created":id});
        doc.envelope
            .pointer_mut(&bin)
            .context("missing subclip bin")?["children"]
            .as_array_mut()
            .context("invalid subclip bin children")?
            .push(json!({"Item":id}));
        doc.envelope["project"]["next_id"] = json!(alloc.next);
        state.project = vec![id];
        id
    } else {
        let item = item.unwrap();
        let kind = if converted {
            // Original conversion requires the direct parent to be Media, not another Subclip.
            let mut media = index
                .item(parent)
                .and_then(|item| item.pointer("/kind/Media"))
                .context("the subclip's media is gone")?
                .clone();
            let r = super::filmcraft_source_inspect::frame_rate(&index, parent)?;
            media["mark_in"] = json!(a);
            media["mark_out"] = json!(sub(b, r.frame_duration().0)?.max(a));
            media["markers"]
                .as_array_mut()
                .context("invalid parent markers")?
                .retain(|m| {
                    m["start"]
                        .as_i64()
                        .is_some_and(|start| start >= a && start < b)
                });
            json!({"Media":media})
        } else {
            json!({"Subclip":{"parent":parent,"range":{"start":a,"duration":sub(b,a)?},"restrict_trims":restrict}})
        };
        doc.envelope["project"]["items"][item.to_string()]["kind"] = kind;
        item
    };
    let result = if created {
        json!({"item":item.to_string()})
    } else {
        json!({"item":item.to_string(),"converted":converted})
    };
    Ok(
        json!({"result":result,"subclipEdit":{"itemId":item.to_string(),"parentId":parent.to_string(),"created":created,"converted":converted,"bindingSourceId":if converted{Some(parent.to_string())}else{None}},"context":{"projectSelection":state.project.iter().map(u64::to_string).collect::<Vec<_>>()},"documentText":String::from_utf8(doc.encode_preserved()?)?,"document":doc.envelope}),
    )
}

/// Authoring metadata for dialog/UI; never reconstruct project JSON in the browser.
pub fn describe(item: &Value) -> Option<Value> {
    let subclip = item.pointer("/kind/Subclip")?;
    let parent = subclip["parent"].as_u64()?;
    let start = subclip["range"]["start"].as_i64()?;
    let end = start.checked_add(subclip["range"]["duration"].as_i64()?)?;
    Some(
        json!({"parentId":parent.to_string(),"startTicks":start.to_string(),"endTicks":end.to_string(),"restrictTrims":subclip["restrict_trims"].as_bool().unwrap_or(false)}),
    )
}
