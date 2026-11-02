//! Clip Modify / Breakout to Mono from Filmcraft clip_ops.rs (MIT), pinned
//! 5231852443363f001c3f6b396dd9b1e6461ae2be. Authoring never opens source paths.
use super::{
    filmcraft_labels::{clip_paths, context_ids},
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::id,
    filmcraft_source_edit::Alloc,
};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
pub const COMMANDS: [&str; 3] = [
    "clip.audioChannels",
    "clip.modifyTimecode",
    "clip.breakoutToMono",
];
/// Read-only summary for source modification dialogs; source paths are omitted.
pub fn describe(item: &Value) -> Option<Value> {
    let media = item.pointer("/kind/Media")?;
    Some(json!({
        "audioChannels":media.pointer("/info/audio/channels").and_then(Value::as_u64),
        "channelMap":media.pointer("/interpret/audio_channels"),
        "startTimecode":media.pointer("/info/start_timecode").and_then(Value::as_i64).map(|v|v.to_string()),
        "tapeName":item.pointer("/metadata/Tape Name").and_then(Value::as_str),
        "fileTimecode":item.pointer("/metadata/File Timecode").and_then(Value::as_str),
    }))
}
fn scalar(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_f64().map(|f| f as u64))
        .or_else(|| v.as_str()?.parse().ok())
}
fn array(v: &Value) -> Option<Vec<u64>> {
    v.as_array().map(|a| {
        a.iter()
            .filter_map(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
            .collect()
    })
}
fn audio(item: &Value) -> bool {
    item.pointer("/kind/Media/info/audio")
        .is_some_and(|v| !v.is_null())
}
fn strings(ids: &[u64]) -> Vec<String> {
    ids.iter().map(u64::to_string).collect()
}
fn format(raw: &str) -> Option<&'static str> {
    match raw.to_ascii_lowercase().as_str() {
        "mono" => Some("Mono"),
        "stereo" => Some("Stereo"),
        "5.1" | "surround51" | "surround" => Some("Surround51"),
        "adaptive" => Some("Adaptive"),
        _ => None,
    }
}
fn default_map(format: &str, n: u16) -> Vec<Vec<u16>> {
    let n = n.max(1);
    match format {
        "Mono" => (0..n).map(|c| vec![c]).collect(),
        "Stereo" => (0..n)
            .step_by(2)
            .map(|c| if c + 1 < n { vec![c, c + 1] } else { vec![c] })
            .collect(),
        _ => vec![(0..n).collect()],
    }
}
pub fn edit(command: &str, mut params: Value) -> Result<Value> {
    ensure!(COMMANDS.contains(&command), "unknown media modify command");
    let fields = params
        .as_object_mut()
        .context("invalid media modify parameters")?;
    let raw = fields.remove("document").context("missing document")?;
    let ctx = fields.remove("context").unwrap_or_else(|| json!({}));
    let bytes = match raw {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let mut doc = SourceProjectDocument::read(&bytes)?;
    let active = id(ctx.get("activeSequence").cloned())?;
    let selection = context_ids(ctx["selection"].as_array().cloned().unwrap_or_default())?;
    let mut project = context_ids(
        ctx["projectSelection"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
    )?;
    let index = ProjectIndex::new(&doc)?;
    let paths = clip_paths(&doc, active);
    let selected_audio: Vec<_> = selection
        .iter()
        .copied()
        .filter(|id| {
            paths
                .iter()
                .any(|(c, path)| c == id && path.contains("/audio_tracks/"))
        })
        .collect();
    let named_items = if command == "clip.modifyTimecode" {
        scalar(&params["item"]).map(|id| vec![id])
    } else {
        array(&params["items"])
    }
    .unwrap_or_default();
    let media_selected = |ids: &[u64]| {
        ids.iter().any(|id| {
            index.item(*id).is_some_and(|item| {
                if command == "clip.modifyTimecode" {
                    item.pointer("/kind/Media").is_some()
                } else {
                    audio(item)
                }
            })
        })
    };
    ensure!(
        media_selected(&project)
            || media_selected(&named_items)
            || (command == "clip.audioChannels" && !selected_audio.is_empty()),
        "select a media clip with the required streams"
    );
    let items = array(&params["items"]).unwrap_or_else(|| {
        scalar(&params["item"])
            .map(|id| vec![id])
            .unwrap_or_else(|| project.clone())
    });
    ensure!(items.len() <= 4096, "too many source items");
    let mut copies = vec![];
    let result = if command == "clip.modifyTimecode" {
        let item = scalar(&params["item"])
            .or_else(|| {
                project.iter().copied().find(|id| {
                    index
                        .item(*id)
                        .is_some_and(|v| v.pointer("/kind/Media").is_some())
                })
            })
            .context("select a media clip")?;
        ensure!(
            index
                .item(item)
                .and_then(|v| v.pointer("/kind/Media"))
                .is_some(),
            "not a media clip"
        );
        let frame = if let Some(tc) = params["timecode"].as_str() {
            let rate = super::filmcraft_source_inspect::frame_rate(&index, item)?;
            Some(
                filmcraft_time::parse_timecode(tc, rate, false, 0)
                    .map_err(|e| anyhow::anyhow!("invalid timecode: {e:?}"))?,
            )
        } else {
            params["frame"]
                .as_i64()
                .or_else(|| params["frame"].as_str()?.parse().ok())
        };
        let tape = params["tapeName"].as_str();
        let reset = params["reset"].as_bool().unwrap_or(false);
        ensure!(
            frame.is_some() || tape.is_some() || reset,
            "need timecode, frame, tapeName or reset"
        );
        ensure!(
            !frame.is_some_and(|f| f < 0),
            "timecode must not be negative"
        );
        let it = &mut doc.envelope["project"]["items"][item.to_string()];
        let cur = it
            .pointer("/kind/Media/info/start_timecode")
            .and_then(Value::as_i64);
        if reset {
            if let Some(orig) = it["metadata"]
                .as_object_mut()
                .context("invalid metadata")?
                .remove("File Timecode")
            {
                it["kind"]["Media"]["info"]["start_timecode"] = orig
                    .as_str()
                    .and_then(|s| s.parse::<i64>().ok())
                    .map_or(Value::Null, Value::from);
            }
        } else if let Some(frame) = frame {
            it["metadata"]
                .as_object_mut()
                .context("invalid metadata")?
                .entry("File Timecode")
                .or_insert_with(|| json!(cur.map_or_else(|| "none".into(), |v| v.to_string())));
            it["kind"]["Media"]["info"]["start_timecode"] = json!(frame);
        }
        if let Some(tape) = tape {
            if tape.is_empty() {
                it["metadata"].as_object_mut().unwrap().remove("Tape Name");
            } else {
                it["metadata"]["Tape Name"] = json!(tape);
            }
        }
        json!({"item":item.to_string(),"startTimecode":it["kind"]["Media"]["info"]["start_timecode"].as_i64().map(|v|v.to_string())})
    } else {
        let items: Vec<_> = items
            .into_iter()
            .filter(|id| index.item(*id).is_some_and(audio))
            .collect();
        if command == "clip.audioChannels" && (items.is_empty() || params.get("channels").is_some())
        {
            // The documented clips parameter is a channel-map array, so timeline clip IDs
            // do not enable this command; they are execution targets once state/items enable it.
            let clips = match params["clips"].as_array() {
                Some(a)
                    if a.iter().all(|v| {
                        v.as_u64().is_some() || v.as_str().is_some_and(|s| s.parse::<u64>().is_ok())
                    }) =>
                {
                    array(&params["clips"]).unwrap()
                }
                _ => selected_audio,
            };
            ensure!(!clips.is_empty(), "select a clip with audio");
            ensure!(clips.len() <= 4096, "too many channel targets");
            let channels: Vec<u16> = params["channels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_u64().map(|v| v as u16))
                .collect();
            let active = active.context("no active sequence")?;
            let qpath = format!("/project/items/{active}/kind/Sequence");
            ensure!(doc.envelope.pointer(&qpath).is_some(), "no active sequence");
            for c in &clips {
                if let Some((_, path)) = paths.iter().find(|(id, _)| id == c) {
                    let clip = doc.envelope.pointer_mut(path).unwrap();
                    if channels.is_empty() {
                        clip.as_object_mut().unwrap().remove("source_channels");
                    } else {
                        clip["source_channels"] = json!(channels);
                    }
                }
            }
            super::filmcraft_marks::check_sequence(doc.envelope.pointer(&qpath).unwrap())?;
            json!({"clips":strings(&clips),"channels":channels})
        } else if command == "clip.audioChannels" {
            let chosen = params["format"]
                .as_str()
                .map(|s| format(s).context("format is mono, stereo, 5.1 or adaptive"))
                .transpose()?;
            let custom = params["clips"].as_array().map(|clips| {
                clips
                    .iter()
                    .map(|clip| {
                        clip.as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|v| v.as_u64().map(|v| v as u16))
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            });
            let mut out = vec![];
            for id in items {
                let media = &mut doc.envelope["project"]["items"][id.to_string()]["kind"]["Media"];
                let n = media["info"]["audio"]["channels"].as_u64().unwrap_or(2) as u16;
                let fmt = chosen.unwrap_or_else(|| {
                    media
                        .pointer("/interpret/audio_channels/format")
                        .and_then(Value::as_str)
                        .unwrap_or("Stereo")
                });
                let map = if let Some(custom) = &custom {
                    ensure!(
                        !custom.is_empty()
                            && custom
                                .iter()
                                .all(|row| !row.is_empty() && row.iter().all(|c| *c < n)),
                        "each clip needs valid source channels"
                    );
                    custom.clone()
                } else {
                    default_map(fmt, n)
                };
                out.push(json!({"item":id.to_string(),"format":fmt,"clips":map}));
                let value = json!({"format":fmt,"clips":map});
                if fmt == "Stereo" && map == vec![vec![0, 1]] {
                    media["interpret"]
                        .as_object_mut()
                        .context("invalid interpretation")?
                        .remove("audio_channels");
                } else {
                    media["interpret"]["audio_channels"] = value;
                }
            }
            json!({"items":out})
        } else {
            ensure!(
                !items.is_empty(),
                "select a clip with audio in the Project panel"
            );
            let mut alloc = Alloc::new(&doc)?;
            let mut made = vec![];
            for source in items {
                let it = doc.envelope["project"]["items"][source.to_string()].clone();
                let media = &it["kind"]["Media"];
                let n = media["info"]["audio"]["channels"]
                    .as_u64()
                    .unwrap_or(1)
                    .max(1) as u16;
                ensure!(
                    made.len() + n as usize <= 4096,
                    "too many mono source aliases"
                );
                let bin = super::filmcraft_subclip::parent_path(
                    &doc.envelope["project"]["root"],
                    "/project/root",
                    source,
                )
                .unwrap_or_else(|| "/project/root".into());
                for c in 0..n {
                    let suffix = match (n, c) {
                        (2, 0) => "Left".into(),
                        (2, 1) => "Right".into(),
                        _ => format!("Ch {}", c + 1),
                    };
                    let mut m = media.clone();
                    m["interpret"]["audio_channels"] = json!({"format":"Mono","clips":[[c]]});
                    m["mark_in"] = Value::Null;
                    m["mark_out"] = Value::Null;
                    m["markers"] = json!([]);
                    m["proxy"] = Value::Null;
                    let id = alloc.get()?;
                    doc.envelope["project"]["items"][id.to_string()] = json!({"id":id,"name":format!("{} {suffix}",it["name"].as_str().context("invalid media name")?),"label":it["label"],"kind":{"Media":m},"metadata":{},"created":id});
                    doc.envelope
                        .pointer_mut(&bin)
                        .context("missing mono destination bin")?["children"]
                        .as_array_mut()
                        .context("invalid bin children")?
                        .push(json!({"Item":id}));
                    made.push(id);
                    copies.push(json!({"id":id.to_string(),"sourceId":source.to_string()}));
                }
            }
            doc.envelope["project"]["next_id"] = json!(alloc.next);
            project = made.clone();
            json!({"items":strings(&made)})
        }
    };
    let text = String::from_utf8(doc.encode_preserved()?)?;
    Ok(
        json!({"result":result,"modifyEdit":true,"context":{"projectSelection":strings(&project)},"copies":copies,"document":doc.envelope,"documentText":text}),
    )
}
