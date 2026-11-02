//! Original MIT source monitor / mark navigation, pinned 5231852443363f001c3f6b396dd9b1e6461ae2be.
use super::{
    filmcraft_clip_rename::target_id, filmcraft_labels::context_ids,
    filmcraft_project_file::SourceProjectDocument, filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::id, filmcraft_source_inspect,
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
    source_playhead: Option<Value>,
    playhead: Option<Value>,
    playhead_frames: Option<Value>,
    selection: Vec<Value>,
    project_selection: Vec<Value>,
    targeted_track_ids: Option<Vec<Value>>,
    selection_follows_playhead: bool,
    linked_selection: Option<bool>,
    still_image_duration: Option<f64>,
    still_image_unit: Option<String>,
}
fn integer(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str()?.parse().ok())
}
fn tick(v: Option<Value>, fallback: i64) -> Result<i64> {
    v.map(|v| integer(&v).context("invalid navigation clock"))
        .transpose()
        .map(|v| v.unwrap_or(fallback))
}
fn strings(ids: &[u64]) -> Vec<String> {
    ids.iter().map(u64::to_string).collect()
}
// Preserve source tick_of casts for representable arithmetic; refuse i128 overflow
// instead of allowing a hostile frame/rational pair to panic in the CPU worker.
fn frame_tick(rate: FrameRate, frame: i64) -> Result<i64> {
    let rate = rate.sane();
    let ticks = (frame as i128)
        .checked_mul(254016000000)
        .and_then(|v| v.checked_mul(rate.den as i128))
        .context("navigation frame clock overflow")?;
    Ok(ticks.div_euclid(rate.num as i128) as i64)
}
fn rate(q: Option<&Value>) -> Result<FrameRate> {
    q.map(|q| serde_json::from_value(q["settings"]["frame_rate"].clone()))
        .transpose()
        .map(|r| r.unwrap_or_default())
        .map_err(Into::into)
}
fn marked(v: &Value) -> bool {
    ["videoIn", "videoOut", "audioIn", "audioOut"]
        .into_iter()
        .any(|key| v["split"][key].as_i64().is_some())
}
fn front_clip<'a>(q: &'a Value, time: i64, targeted: Option<&[u64]>) -> Result<Option<&'a Value>> {
    let tracks = q["video_tracks"]
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .chain(q["audio_tracks"].as_array().into_iter().flatten());
    for track in tracks {
        if targeted.is_some_and(|ids| !track["id"].as_u64().is_some_and(|id| ids.contains(&id))) {
            continue;
        }
        let Some(clip) = track["items"]
            .as_array()
            .into_iter()
            .flatten()
            .rev()
            .find(|clip| clip["start"].as_i64().is_some_and(|start| start <= time))
        else {
            continue;
        };
        let end = clip["start"]
            .as_i64()
            .context("invalid match clip start")?
            .checked_add(
                clip["duration"]
                    .as_i64()
                    .context("invalid match clip duration")?,
            )
            .context("match clip end overflow")?;
        if time < end {
            return Ok(Some(clip));
        }
    }
    Ok(None)
}
fn select(q: &Value, time: i64, targeted: Option<&[u64]>, linked: bool) -> Result<Vec<u64>> {
    let tracks: Vec<_> = ["video_tracks", "audio_tracks"]
        .into_iter()
        .flat_map(|key| q[key].as_array().into_iter().flatten())
        .collect();
    let mut out = Vec::new();
    for track in &tracks {
        let track_id = track["id"].as_u64().context("invalid track ID")?;
        if targeted.is_some_and(|ids| !ids.contains(&track_id)) {
            continue;
        }
        let items = track["items"].as_array().context("invalid track items")?;
        // Original item_at chooses the last start <= t before checking its end.
        if let Some(item) = items
            .iter()
            .rev()
            .find(|item| item["start"].as_i64().is_some_and(|t| t <= time))
        {
            let end = item["start"]
                .as_i64()
                .context("invalid clip start")?
                .checked_add(item["duration"].as_i64().context("invalid clip duration")?)
                .context("clip end overflow")?;
            if time < end {
                out.push(item["id"].as_u64().context("invalid clip ID")?);
            }
        }
    }
    if linked {
        let links: Vec<_> = tracks
            .iter()
            .flat_map(|t| t["items"].as_array().into_iter().flatten())
            .filter(|i| i["id"].as_u64().is_some_and(|id| out.contains(&id)))
            .filter_map(|i| i["link"].as_u64())
            .collect();
        for item in tracks
            .iter()
            .flat_map(|t| t["items"].as_array().into_iter().flatten())
        {
            if item["link"]
                .as_u64()
                .is_some_and(|link| links.contains(&link))
            {
                let id = item["id"].as_u64().context("invalid linked clip ID")?;
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
    }
    Ok(out)
}
pub fn navigate(command: &str, mut params: Value) -> Result<Value> {
    let fields = params
        .as_object_mut()
        .context("expected navigation parameters")?;
    let document = fields
        .remove("document")
        .context("missing navigation document")?;
    let context: SessionContext = fields
        .remove("context")
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();
    let bytes = match document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let document = SourceProjectDocument::read(&bytes)?;
    let index = ProjectIndex::new(&document)?;
    let active = id(context.active_sequence)?;
    let q = active
        .and_then(|id| index.item(id))
        .and_then(|i| i.pointer("/kind/Sequence"));
    let program_rate = rate(q)?;
    let mut source = id(context.source_item)?;
    let mut source_time = tick(context.source_playhead, 0)?;
    let mut playhead = if active.is_none() {
        0
    } else {
        match context.playhead {
            Some(v) => tick(Some(v), 0)?,
            None => frame_tick(program_rate, tick(context.playhead_frames, 0)?)?,
        }
    };
    let mut selection = context_ids(context.selection)?;
    let mut project_selection = context_ids(context.project_selection)?;
    let targeted = context.targeted_track_ids.map(context_ids).transpose()?;
    let source_target = params["target"] == "source";
    let mut result = Value::Null;
    let mut event = None;
    let mut event_type = "openSource";
    let mut seek_program = None;
    match command {
        "sequence.matchFrame" => {
            let q = q.context("no sequence is open")?;
            // Match Frame deliberately ignores targeting, enabled/mute/lock state and selection.
            let clip = front_clip(q, playhead, None)?.context("no clip at the playhead")?;
            let item = clip["item"].as_u64().context("invalid match source item")?;
            source_time = match clip["frame_hold"].as_i64() {
                Some(held) => held,
                None => super::filmcraft_clip_edit::source_at(clip, playhead)?,
            };
            source = Some(item);
            event = Some(item);
            result = json!({"item":item.to_string()});
        }
        "clip.revealInProject" | "sequence.revealInProject" => {
            let q = q.context("no sequence is open")?;
            let item = if command == "sequence.revealInProject" {
                target_id(params.get("item").cloned())?
                    .or(active)
                    .context("no sequence is open")?
            } else {
                let clips = params["clips"]
                    .as_array()
                    .map(|ids| {
                        ids.iter()
                            .filter_map(|id| id.as_u64().or_else(|| id.as_str()?.parse().ok()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| {
                        target_id(params.get("clip").cloned())
                            .ok()
                            .flatten()
                            .map(|id| vec![id])
                            .unwrap_or_else(|| selection.clone())
                    });
                let picked = clips.iter().find_map(|id| {
                    super::filmcraft_trim::clip_location(q, *id)
                        .ok()
                        .map(|(key, track, clip)| &q[key][track]["items"][clip])
                });
                (match picked {
                    Some(clip) => Some(clip),
                    None => front_clip(q, playhead, targeted.as_deref())?,
                })
                .and_then(|clip| clip["item"].as_u64())
                .context("select a clip to reveal")?
            };
            index
                .item(item)
                .context("that clip's source is not in the project")?;
            project_selection = vec![item];
            event = Some(item);
            event_type = "revealInProject";
            result = json!({"item":item.to_string()});
        }
        "source.open" => {
            let item = target_id(params.get("item").cloned())?
                .or(project_selection.first().copied())
                .context("need source item or Project panel selection")?;
            index.item(item).context("no such source item")?;
            source = Some(item);
            event = Some(item);
            let view=filmcraft_source_inspect::inspect(json!({"document":document.envelope,"item":item,"context":{"stillImageDuration":context.still_image_duration.unwrap_or(5.),"stillImageUnit":context.still_image_unit.as_deref().unwrap_or("seconds")}})).ok();
            source_time = view
                .as_ref()
                .and_then(|v| integer(&v["start"]))
                .unwrap_or(0);
        }
        "source.setPlayhead" => {
            let item = source.context("no source clip")?;
            let view=filmcraft_source_inspect::inspect(json!({"document":document.envelope,"item":item,"context":{"stillImageDuration":context.still_image_duration.unwrap_or(5.),"stillImageUnit":context.still_image_unit.as_deref().unwrap_or("seconds")}})).ok();
            let r = if view.is_some() {
                filmcraft_source_inspect::frame_rate(&index, item).unwrap_or_default()
            } else {
                FrameRate::default()
            };
            let time = if let Some(t) = integer(&params["time"]) {
                t
            } else if let Some(frame) = integer(&params["frame"]) {
                frame_tick(r, frame)?
            } else {
                params["seconds"]
                    .as_f64()
                    .map(|s| Tick::from_seconds_f64(s).0)
                    .unwrap_or(0)
            };
            let (lo, hi) = view
                .as_ref()
                .and_then(|v| Some((integer(&v["start"])?, integer(&v["end"])?)))
                .unwrap_or((0, Tick::MAX.0));
            let hi = hi
                .checked_sub(r.frame_duration().0)
                .context("source monitor span overflow")?
                .max(lo);
            source_time = r.snap(Tick(time.clamp(lo, hi))).0.max(lo);
            result = json!({"time":source_time.to_string()});
        }
        "markers.goToIn" | "markers.goToOut" => {
            let q = q.context("no sequence is open")?;
            seek_program = q[if command == "markers.goToIn" {
                "mark_in"
            } else {
                "mark_out"
            }]
            .as_i64();
        }
        "markers.goToSplitVideoIn"
        | "markers.goToSplitVideoOut"
        | "markers.goToSplitAudioIn"
        | "markers.goToSplitAudioOut" => {
            let source_item = source.and_then(|id| index.item(id));
            ensure!(
                q.is_some_and(marked) || source_item.is_some_and(marked),
                "there are no split points"
            );
            let key = match command {
                "markers.goToSplitVideoIn" => "videoIn",
                "markers.goToSplitVideoOut" => "videoOut",
                "markers.goToSplitAudioIn" => "audioIn",
                _ => "audioOut",
            };
            let ordinary = if command.ends_with("In") {
                "mark_in"
            } else {
                "mark_out"
            };
            let time = if source_target {
                source.context("no source clip")?;
                let item = source_item.context("no such source item")?;
                item["split"][key].as_i64().or_else(|| {
                    item.pointer("/kind/Media")
                        .or_else(|| item.pointer("/kind/Sequence"))
                        .and_then(|v| v[ordinary].as_i64())
                })
            } else {
                let q = q.context("no sequence is open")?;
                q["split"][key].as_i64().or_else(|| q[ordinary].as_i64())
            }
            .context("there is no requested split point")?;
            if source_target {
                source_time = time;
                result = json!({"time":time.to_string()});
            } else {
                seek_program = Some(time);
            }
        }
        _ => anyhow::bail!("unsupported navigation command"),
    }
    if let Some(time) = seek_program {
        playhead = program_rate.snap(Tick(time.max(0))).0;
        if context.selection_follows_playhead {
            selection = select(
                q.unwrap(),
                playhead,
                targeted.as_deref(),
                context.linked_selection.unwrap_or(true),
            )?;
        }
        if command.starts_with("markers.goToSplit") {
            result = json!({"time":playhead.to_string()});
        }
    }
    Ok(
        json!({"result":result,"context":{"activeSequence":active.map(|id|id.to_string()),"sourceItem":source.map(|id|id.to_string()),"sourcePlayhead":source_time.to_string(),"playhead":playhead.to_string(),"playheadFrames":program_rate.frame_at(Tick(playhead)).to_string(),"selection":strings(&selection),"projectSelection":strings(&project_selection)},"events":event.into_iter().map(|id|json!({"type":event_type,"item":id.to_string()})).collect::<Vec<_>>()}),
    )
}

/// Apply the original selection-follows-playhead policy after a document edit.
pub(super) fn selection_at(
    document: &SourceProjectDocument,
    sequence: u64,
    time: i64,
    targeted: Option<&Vec<Value>>,
    linked: bool,
) -> Result<Vec<u64>> {
    let targets = targeted.map(|v| context_ids(v.clone())).transpose()?;
    let q = document
        .envelope
        .pointer(&format!("/project/items/{sequence}/kind/Sequence"))
        .context("no sequence is open")?;
    select(q, time, targets.as_deref(), linked)
}
