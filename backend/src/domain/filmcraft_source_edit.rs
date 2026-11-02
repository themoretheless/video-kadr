//! Source Insert/Overwrite adapted from Filmcraft (MIT), pinned
//! 5231852443363f001c3f6b396dd9b1e6461ae2be commands.rs / sequence_tools.rs.
use super::{
    filmcraft_duplicate::nest_cycle, filmcraft_effect_catalog::definitions,
    filmcraft_effect_edit::fresh, filmcraft_marks::check_sequence,
    filmcraft_project_file::SourceProjectDocument, filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::id, filmcraft_trim,
};
use anyhow::{ensure, Context, Result};
use filmcraft_time::{FrameRate, Tick};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct ContextState {
    active_sequence: Option<Value>,
    source_item: Option<Value>,
    source_playhead: Option<Value>,
    playhead: Option<Value>,
    playhead_frames: Option<Value>,
    video_dest: Option<Value>,
    audio_dest: Option<Value>,
    targeting: Option<Value>,
    sequences_as_clips: bool,
    ripple_sequence_markers: bool,
    selection_follows_playhead: bool,
    linked_selection: Option<bool>,
    targeted_track_ids: Option<Vec<Value>>,
    default_media_scaling: Option<String>,
    still_image_duration: Option<f64>,
    still_image_unit: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    document: Value,
    #[serde(default)]
    context: ContextState,
}
fn n(v: &Value) -> Result<i64> {
    v.as_i64()
        .or_else(|| v.as_str()?.parse().ok())
        .context("invalid source edit tick")
}
fn add(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b).context("source edit clock overflow")
}
fn sub(a: i64, b: i64) -> Result<i64> {
    a.checked_sub(b).context("source edit clock overflow")
}
fn rate(v: &Value) -> Result<FrameRate> {
    let r: FrameRate = if v.is_null() {
        FrameRate::default()
    } else {
        serde_json::from_value(v.clone())?
    };
    ensure!(
        r.num > 0 && r.den > 0 && r.num <= u32::MAX as i64 && r.den <= u32::MAX as i64,
        "invalid source edit rate"
    );
    Ok(r)
}
fn strings(v: &[u64]) -> Vec<String> {
    v.iter().map(u64::to_string).collect()
}
pub(super) struct Alloc {
    pub(super) next: u64,
    used: BTreeSet<u64>,
}
impl Alloc {
    pub(super) fn new(doc: &SourceProjectDocument) -> Result<Self> {
        fn walk(v: &Value, set: &mut BTreeSet<u64>) {
            match v {
                Value::Object(m) => {
                    for (k, v) in m {
                        if matches!(k.as_str(), "id" | "link" | "group") {
                            if let Some(id) = v.as_u64() {
                                set.insert(id);
                            }
                        }
                        walk(v, set);
                    }
                }
                Value::Array(a) => {
                    for v in a {
                        walk(v, set)
                    }
                }
                _ => {}
            }
        }
        let mut used = BTreeSet::new();
        walk(doc.project(), &mut used);
        Ok(Self {
            next: doc.project()["next_id"]
                .as_u64()
                .context("missing next ID")?,
            used,
        })
    }
    pub(super) fn get(&mut self) -> Result<u64> {
        let next = self.next;
        ensure!(
            !self.used.contains(&next),
            "source edit next ID already allocated"
        );
        self.next = next
            .checked_add(1)
            .context("source edit ID space exhausted")?;
        self.used.insert(next);
        Ok(next)
    }
}
fn duration(q: &Value) -> Result<i64> {
    let mut kind_end = Vec::new();
    for kind in ["video_tracks", "audio_tracks", "caption_tracks"] {
        for track in q[kind].as_array().into_iter().flatten() {
            let mut end = None;
            for clip in track[if kind == "caption_tracks" {
                "captions"
            } else {
                "items"
            }]
            .as_array()
            .into_iter()
            .flatten()
            {
                let e = add(n(&clip["start"])?, n(&clip["duration"])?)?;
                end = Some(end.map_or(e, |old: i64| old.max(e)));
            }
            kind_end.push(end.unwrap_or(0));
        }
    }
    Ok(kind_end.into_iter().max().unwrap_or(0))
}
fn item_rate(item: &Value) -> Result<FrameRate> {
    if let Some(m) = item.pointer("/kind/Media") {
        rate(if !m["interpret"]["frame_rate"].is_null() {
            &m["interpret"]["frame_rate"]
        } else {
            &m["info"]["video"]["frame_rate"]
        })
    } else if let Some(q) = item.pointer("/kind/Sequence") {
        rate(&q["settings"]["frame_rate"])
    } else {
        rate(
            item.pointer("/kind/Graphic/rate")
                .or_else(|| item.pointer("/kind/AdjustmentLayer/rate"))
                .unwrap_or(&Value::Null),
        )
    }
}
fn source_range(item: &Value, seq_rate: FrameRate, ctx: &ContextState) -> Result<(i64, i64)> {
    if let Some(s) = item.pointer("/kind/Subclip") {
        return Ok((n(&s["range"]["start"])?, n(&s["range"]["duration"])?));
    }
    let ordinary = item
        .pointer("/kind/Media")
        .or_else(|| item.pointer("/kind/Sequence"));
    let r = item_rate(item)?;
    let duration = if let Some(m) = item.pointer("/kind/Media") {
        if m["info"]["kind"] == "Still" {
            let value = ctx.still_image_duration.unwrap_or(5.);
            ensure!(value.is_finite(), "invalid still duration");
            let t = if ctx.still_image_unit.as_deref() == Some("frames") {
                let frames = value.max(1.).round() as i64;
                let ticks = (frames as i128)
                    .checked_mul(254016000000)
                    .and_then(|v| v.checked_mul(seq_rate.den as i128))
                    .context("still clock overflow")?
                    / seq_rate.num as i128;
                i64::try_from(ticks).context("still clock overflow")?
            } else {
                ensure!(
                    ctx.still_image_unit
                        .as_deref()
                        .is_none_or(|s| s == "seconds"),
                    "invalid still duration unit"
                );
                ensure!(
                    value.max(0.) * 254016000000. <= Tick::MAX.0 as f64,
                    "still duration exceeds clock"
                );
                seq_rate
                    .snap_nearest(Tick::from_seconds_f64(value.max(0.)))
                    .0
            };
            t.max(seq_rate.frame_duration().0)
        } else {
            n(&m["info"]["duration"])?
        }
    } else if let Some(q) = item.pointer("/kind/Sequence") {
        duration(q)?
    } else if let Some(a) = item.pointer("/kind/AdjustmentLayer") {
        n(&a["duration"])?
    } else {
        3600 * 254016000000
    };
    let start = ordinary.and_then(|v| v["mark_in"].as_i64()).unwrap_or(0);
    let end = ordinary
        .and_then(|v| v["mark_out"].as_i64())
        .map(|t| add(t, r.frame_duration().0))
        .transpose()?
        .unwrap_or(duration)
        .min(if duration > 0 { duration } else { Tick::MAX.0 });
    Ok((
        start,
        sub(end.max(add(start, r.frame_duration().0)?), start)?,
    ))
}
fn split_ranges(item: &Value, full: (i64, i64)) -> Result<((i64, i64), (i64, i64))> {
    let split = &item["split"];
    let r = item_rate(item)?;
    let range = |prefix: &str| -> Result<(i64, i64)> {
        let a = split[format!("{prefix}In")].as_i64().unwrap_or(full.0);
        let b = split[format!("{prefix}Out")]
            .as_i64()
            .map(|v| add(v, r.frame_duration().0))
            .transpose()?
            .unwrap_or(add(full.0, full.1)?);
        Ok((a, sub(b.max(add(a, r.frame_duration().0)?), a)?))
    };
    Ok((range("video")?, range("audio")?))
}
fn auto_points(e: &mut Value, frame: (u32, u32), source: (u32, u32)) {
    let def = definitions().iter().find(|d| d["id"] == e["effect"]);
    let normal = def.map(|d| fresh(d, frame.0, frame.1));
    let anchor = def.map(|d| fresh(d, source.0, source.1));
    if let Some(params) = e["params"].as_object_mut() {
        for (key, param) in params {
            if let Some(v) = param.pointer_mut("/value/Vec2") {
                let size = if key == "anchor" { source } else { frame };
                let defaults = if key == "anchor" {
                    anchor.as_ref()
                } else {
                    normal.as_ref()
                };
                for axis in ["x", "y"] {
                    if v[axis].is_null() {
                        v[axis] = defaults
                            .and_then(|d| d.pointer(&format!("/params/{key}/value/Vec2/{axis}")))
                            .filter(|v| !v.is_null())
                            .cloned()
                            .unwrap_or_else(|| {
                                json!(if axis == "x" {
                                    size.0 as f64 / 2.
                                } else {
                                    size.1 as f64 * if key == "end" { 1. } else { 0.5 }
                                })
                            });
                    }
                }
            }
        }
    }
}
fn channel_map(clip: &mut Value, channels: Value) {
    if channels.as_array().is_none_or(Vec::is_empty) {
        clip.as_object_mut().unwrap().remove("source_channels");
    } else {
        clip["source_channels"] = channels;
    }
}
fn make_clip(
    item: &Value,
    kind: &str,
    at: i64,
    range: (i64, i64),
    r: FrameRate,
    alloc: &mut Alloc,
    graphic: Option<&Value>,
) -> Result<Value> {
    let names = if kind == "Video" {
        ["motion", "opacity", "time_remap"]
    } else {
        ["volume", "channel_volume", "panner"]
    };
    // Construction keeps NaN auto points serialized as null until placement resolves them.
    let mut effects:Vec<Value>=names.into_iter().filter_map(|id|definitions().iter().find(|d|d["id"]==id)).map(|d|json!({"effect":d["id"],"enabled":true,"params":d["params"].as_array().unwrap().iter().map(|p|(p["id"].as_str().unwrap().to_string(),json!({"value":p["default"]}))).collect::<serde_json::Map<String,Value>>()})).collect();
    if kind == "Video" {
        if let Some(g) = graphic {
            effects.extend(g["layers"].as_array().into_iter().flatten().cloned());
        }
    }
    let mut clip = json!({"id":alloc.get()?,"item":item["id"],"name":item["name"],"label":item["label"],"start":at,"duration":r.snap_nearest(Tick(range.1)).0.max(r.frame_duration().0),"source_in":range.0,"speed":1.0,"reverse":false,"enabled":true,"link":null,"group":null,"effects":effects,"markers":[],"gain_db":0.0,"frame_hold":null,"scale_to_frame":false});
    if let Some(m) = item
        .pointer("/kind/Sequence/multicam")
        .filter(|v| !v.is_null())
    {
        let angle = m["cameras"]
            .as_array()
            .into_iter()
            .flatten()
            .position(|c| !c["video_track"].is_null())
            .unwrap_or(0);
        clip["multicam"] = json!({"enabled":true,"angle":angle});
    }
    if kind == "Audio" {
        if let Some(channels) = item.pointer("/kind/Media/interpret/audio_channels/clips/0") {
            channel_map(&mut clip, channels.clone());
        }
    }
    if kind == "Video" {
        if let Some(meta) = graphic.and_then(|g| g.get("meta")).filter(|v| !v.is_null()) {
            clip["graphic"] = meta.clone();
        }
    }
    Ok(clip)
}
fn clip_to(item: &Value, range: (i64, i64)) -> Result<Option<Value>> {
    let start = n(&item["start"])?;
    let end = add(start, n(&item["duration"])?)?;
    let a = start.max(range.0);
    let b = end.min(add(range.0, range.1)?);
    if b <= a {
        return Ok(None);
    }
    let mut clip = item.clone();
    if item["frame_hold"].is_null() {
        let delta = if item["reverse"].as_bool().unwrap_or(false) {
            sub(end, b)?
        } else {
            sub(a, start)?
        };
        let speed = item["speed"]
            .as_f64()
            .context("invalid sequence clip speed")?
            .abs();
        ensure!(speed.is_finite(), "invalid sequence clip speed");
        clip["source_in"] = json!(add(
            n(&item["source_in"])?,
            (delta as f64 * speed).round() as i64
        )?);
    }
    clip["start"] = json!(a);
    clip["duration"] = json!(sub(b, a)?);
    Ok(Some(clip))
}
fn new_track(id: u64, kind: &str, index: usize) -> Value {
    json!({"id":id,"kind":kind,"name":format!("{kind} {index}"),"items":[],"transitions":[],"locked":false,"sync_lock":true,"enabled":true,"muted":false,"solo":false,"channels":"Stereo","volume_db":0.0,"pan":0.0,"effects":[],"mixer":{"mode":"Read","output":null,"record_arm":false,"solo_safe":false,"input_map":"Stereo"}})
}
fn flattened(
    doc: &mut SourceProjectDocument,
    sequence: u64,
    source: &Value,
    range: (i64, i64),
    at: i64,
    video: Option<u64>,
    audio: Option<u64>,
    alloc: &mut Alloc,
) -> Result<(Vec<(u64, Value)>, Vec<(u64, Value)>)> {
    let dst = &doc.envelope["project"]["items"][sequence.to_string()]["kind"]["Sequence"];
    let mut base = BTreeMap::new();
    let mut lanes = BTreeMap::new();
    let mut items = Vec::new();
    for (kind, key, dest) in [
        (false, "video_tracks", video),
        (true, "audio_tracks", audio),
    ] {
        let Some(first) = dest.and_then(|id| {
            dst[key]
                .as_array()?
                .iter()
                .position(|t| t["id"].as_u64() == Some(id))
        }) else {
            continue;
        };
        base.insert(kind, first);
        let mut lane = 0;
        for (ti, track) in source[key].as_array().into_iter().flatten().enumerate() {
            let clips: Vec<_> = track["items"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|clip| clip_to(clip, range))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect();
            if clips.is_empty() {
                continue;
            }
            lanes.insert((kind, ti), lane);
            items.extend(clips.into_iter().map(|c| (kind, lane, c)));
            lane += 1;
        }
    }
    ensure!(
        !items.is_empty(),
        "no destination track for this sequence's clips"
    );
    let mut idmap = BTreeMap::new();
    let mut links = BTreeMap::new();
    let mut placed = Vec::new();
    for (kind, lane, mut clip) in items {
        let old = clip["id"]
            .as_u64()
            .context("invalid source sequence clip ID")?;
        let id = alloc.get()?;
        idmap.insert(old, id);
        clip["id"] = json!(id);
        clip["start"] = json!(add(at, sub(n(&clip["start"])?, range.0)?)?);
        if let Some(link) = clip["link"].as_u64() {
            let link = if let Some(id) = links.get(&link) {
                *id
            } else {
                let id = alloc.get()?;
                links.insert(link, id);
                id
            };
            clip["link"] = json!(link);
        }
        placed.push((kind, lane, clip));
    }
    let mut transitions = Vec::new();
    for (kind, key) in [(false, "video_tracks"), (true, "audio_tracks")] {
        for (ti, track) in source[key].as_array().into_iter().flatten().enumerate() {
            let Some(lane) = lanes.get(&(kind, ti)).copied() else {
                continue;
            };
            for tr in track["transitions"].as_array().into_iter().flatten() {
                let from = tr["from"].as_u64();
                let to = tr["to"].as_u64();
                let start = n(&tr["start"])?;
                if start < range.0
                    || add(start, n(&tr["duration"])?)? > add(range.0, range.1)?
                    || from.is_some_and(|id| !idmap.contains_key(&id))
                    || to.is_some_and(|id| !idmap.contains_key(&id))
                {
                    continue;
                }
                let mut tr = tr.clone();
                tr["id"] = json!(alloc.get()?);
                tr["from"] = json!(from.map(|id| idmap[&id]));
                tr["to"] = json!(to.map(|id| idmap[&id]));
                tr["start"] = json!(add(at, sub(start, range.0)?)?);
                transitions.push((kind, lane, tr));
            }
        }
    }
    let mut dest = BTreeMap::new();
    let q = &mut doc.envelope["project"]["items"][sequence.to_string()]["kind"]["Sequence"];
    for (kind, lane) in placed
        .iter()
        .map(|(k, l, _)| (*k, *l))
        .collect::<BTreeSet<_>>()
    {
        let key = if kind { "audio_tracks" } else { "video_tracks" };
        let index = base
            .get(&kind)
            .copied()
            .unwrap_or(0)
            .checked_add(lane)
            .context("sequence track index overflow")?;
        let tracks = q[key]
            .as_array_mut()
            .context("invalid destination tracks")?;
        while tracks.len() <= index {
            let id = alloc.get()?;
            tracks.push(new_track(
                id,
                if kind { "Audio" } else { "Video" },
                tracks.len() + 1,
            ));
        }
        dest.insert(
            (kind, lane),
            tracks[index]["id"]
                .as_u64()
                .context("invalid destination track ID")?,
        );
    }
    Ok((
        placed
            .into_iter()
            .map(|(k, l, c)| (dest[&(k, l)], c))
            .collect(),
        transitions
            .into_iter()
            .map(|(k, l, t)| (dest[&(k, l)], t))
            .collect(),
    ))
}
pub fn edit(command: &str, params: Value) -> Result<Value> {
    ensure!(
        matches!(command, "source.insert" | "source.overwrite"),
        "unsupported source edit"
    );
    let r: Request = serde_json::from_value(params)?;
    let ctx = r.context;
    let bytes = match r.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let mut doc = SourceProjectDocument::read(&bytes)?;
    let cycle = nest_cycle(&doc)?;
    let index = ProjectIndex::new(&doc)?;
    let sequence = id(ctx.active_sequence.clone())?.context("no sequence is open")?;
    let q = index
        .item(sequence)
        .and_then(|i| i.pointer("/kind/Sequence"))
        .context("no sequence is open")?
        .clone();
    let source = id(ctx.source_item.clone())?.context("no source clip")?;
    let item = index.item(source).context("no source clip")?.clone();
    let r = item_rate(&json!({"kind":{"Sequence":q}}))?;
    let playhead = if let Some(v) = ctx.playhead.as_ref() {
        n(v)?
    } else {
        let frame = ctx
            .playhead_frames
            .as_ref()
            .map(n)
            .transpose()?
            .unwrap_or(0);
        let ticks = (frame as i128)
            .checked_mul(254016000000)
            .and_then(|v| v.checked_mul(r.den as i128))
            .context("source edit frame overflow")?
            / (r.num as i128);
        i64::try_from(ticks).context("source edit frame overflow")?
    };
    let full = source_range(&item, r, &ctx)?;
    let (vr, ar) = split_ranges(&item, full)?;
    let at = q["mark_in"].as_i64().unwrap_or(playhead);
    let base = vr.0.min(ar.0);
    let vat = add(at, sub(vr.0, base)?)?;
    let aat = add(at, sub(ar.0, base)?)?;
    let target = ctx.targeting.as_ref();
    let video = if let Some(t) = target {
        id(t.get("videoDest").cloned())?
    } else if ctx.video_dest.is_some() {
        id(ctx.video_dest.clone())?
    } else {
        q["video_tracks"][0]["id"].as_u64()
    };
    let audio = if let Some(t) = target {
        id(t.get("audioDest").cloned())?
    } else if ctx.audio_dest.is_some() {
        id(ctx.audio_dest.clone())?
    } else {
        q["audio_tracks"][0]["id"].as_u64()
    };
    let mut alloc = Alloc::new(&doc)?;
    let range = if vr == ar { full } else { vr };
    let place_at = if vr == ar { at } else { vat };
    let flatten = ctx.sequences_as_clips
        && item
            .pointer("/kind/Sequence")
            .is_some_and(|q| q["multicam"].is_null());
    let (placements, transitions) = if flatten {
        flattened(
            &mut doc,
            sequence,
            &item["kind"]["Sequence"],
            range,
            place_at,
            video,
            audio,
            &mut alloc,
        )?
    } else {
        let media = item.pointer("/kind/Media");
        let has_video = video.is_some() && (media.map_or(true, |m| !m["info"]["video"].is_null()));
        let has_audio = audio.is_some()
            && (media.is_some_and(|m| !m["info"]["audio"].is_null())
                || item
                    .pointer("/kind/Sequence/audio_tracks")
                    .and_then(Value::as_array)
                    .is_some_and(|t| !t.is_empty()));
        ensure!(
            has_video || has_audio,
            "no destination track for this media"
        );
        let link = if has_video && has_audio {
            Some(alloc.get()?)
        } else {
            None
        };
        let mut placed = Vec::new();
        let graphic = doc.project()["source_graphics"].get(source.to_string());
        if has_video {
            let mut clip = make_clip(&item, "Video", place_at, range, r, &mut alloc, graphic)?;
            clip["link"] = json!(link);
            let frame = (
                q["settings"]["width"]
                    .as_u64()
                    .context("invalid sequence width")? as u32,
                q["settings"]["height"]
                    .as_u64()
                    .context("invalid sequence height")? as u32,
            );
            let size = item
                .pointer("/kind/Media/info/video")
                .filter(|v| !v.is_null())
                .map(|v| {
                    (
                        v["width"].as_u64().unwrap_or(1920) as u32,
                        v["height"].as_u64().unwrap_or(1080) as u32,
                    )
                })
                .or_else(|| {
                    item.pointer("/kind/Sequence/settings").map(|v| {
                        (
                            v["width"].as_u64().unwrap_or(1920) as u32,
                            v["height"].as_u64().unwrap_or(1080) as u32,
                        )
                    })
                })
                .or_else(|| {
                    item.pointer("/kind/AdjustmentLayer").map(|v| {
                        (
                            v["width"].as_u64().unwrap_or(1920) as u32,
                            v["height"].as_u64().unwrap_or(1080) as u32,
                        )
                    })
                })
                .unwrap_or((1920, 1080));
            for e in clip["effects"].as_array_mut().unwrap() {
                auto_points(e, frame, size);
            }
            if (media.is_some() || item.pointer("/kind/Subclip").is_some())
                && size != frame
                && size.0 > 0
                && size.1 > 0
            {
                match ctx.default_media_scaling.as_deref() {
                    Some("scaleToFrameSize") => clip["scale_to_frame"] = json!(true),
                    Some("setToFrameSize") => {
                        let fit =
                            (frame.0 as f64 / size.0 as f64).min(frame.1 as f64 / size.1 as f64);
                        if let Some(m) = clip["effects"]
                            .as_array_mut()
                            .unwrap()
                            .iter_mut()
                            .find(|e| e["effect"] == "motion")
                        {
                            m["params"]["scale"] =
                                json!({"value":{"Float":(fit*1000.).round()/10.}});
                        }
                    }
                    _ => {}
                }
            }
            placed.push((video.unwrap(), clip));
        }
        if has_audio {
            let mut clip = make_clip(
                &item,
                "Audio",
                if vr == ar { at } else { aat },
                if vr == ar { full } else { ar },
                r,
                &mut alloc,
                None,
            )?;
            let extra: Vec<_> = item
                .pointer("/kind/Media/interpret/audio_channels/clips")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .skip(1)
                .cloned()
                .collect();
            clip["link"] = json!(if link.is_none() && !extra.is_empty() {
                Some(alloc.get()?)
            } else {
                link
            });
            placed.push((audio.unwrap(), clip.clone()));
            let tracks = q["audio_tracks"]
                .as_array()
                .context("invalid audio tracks")?;
            let first = tracks
                .iter()
                .position(|t| t["id"].as_u64() == audio)
                .unwrap_or(0);
            for (k, channels) in extra.into_iter().enumerate() {
                let Some(track) = tracks.get(first + k + 1) else {
                    break;
                };
                let mut b = clip.clone();
                b["id"] = json!(alloc.get()?);
                channel_map(&mut b, channels);
                placed.push((track["id"].as_u64().context("invalid audio track ID")?, b));
            }
        }
        (placed, Vec::new())
    };
    doc.envelope["project"]["next_id"] = json!(alloc.next);
    let insert = command == "source.insert";
    let span = placements
        .iter()
        .map(|(_, c)| n(&c["start"]))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .min()
        .zip(
            placements
                .iter()
                .map(|(_, c)| add(n(&c["start"])?, n(&c["duration"])?))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .max(),
        );
    let clips = filmcraft_trim::place_clips(&mut doc, sequence, &placements, insert)?;
    let q = &mut doc.envelope["project"]["items"][sequence.to_string()]["kind"]["Sequence"];
    if flatten {
        for (track, tr) in transitions {
            let target = ["video_tracks", "audio_tracks"]
                .into_iter()
                .find_map(|k| {
                    q[k].as_array()?
                        .iter()
                        .position(|t| t["id"].as_u64() == Some(track))
                        .map(|i| (k, i))
                })
                .context("missing transition track")?;
            q[target.0][target.1]["transitions"]
                .as_array_mut()
                .context("missing transitions")?
                .push(tr);
        }
        for kind in ["video_tracks", "audio_tracks"] {
            for track in q[kind].as_array_mut().into_iter().flatten() {
                track["items"]
                    .as_array_mut()
                    .unwrap()
                    .sort_by_key(|c| c["start"].as_i64().unwrap_or(0));
                let ids: Vec<_> = track["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|c| c["id"].as_u64())
                    .collect();
                if let Some(transitions) = track["transitions"].as_array_mut() {
                    transitions.sort_by_key(|t| t["start"].as_i64().unwrap_or(0));
                    transitions.retain(|t| {
                        let a = t["from"].as_u64();
                        let b = t["to"].as_u64();
                        a.is_none_or(|a| ids.contains(&a))
                            && b.is_none_or(|b| ids.contains(&b))
                            && (a.is_some() || b.is_some())
                    });
                }
            }
        }
        check_sequence(q)?;
    }
    if insert && ctx.ripple_sequence_markers {
        if let Some((a, b)) = span {
            let shift = sub(b, a)?;
            for marker in q["markers"].as_array_mut().into_iter().flatten() {
                let time = n(&marker["start"])?;
                if time >= a {
                    marker["start"] = json!(add(time, shift)?);
                }
            }
        }
    }
    if cycle.is_none() {
        ensure!(
            nest_cycle(&doc)?.is_none(),
            "a sequence cannot be nested inside itself"
        );
    }
    let end = if vr == ar {
        add(at, r.snap_nearest(Tick(full.1)).0)?
    } else {
        r.snap_nearest(Tick(add(vat, vr.1)?.max(add(aat, ar.1)?))).0
    };
    let playhead = r.snap(Tick(end.max(0))).0;
    let selected = if ctx.selection_follows_playhead {
        super::filmcraft_navigation::selection_at(
            &doc,
            sequence,
            playhead,
            ctx.targeted_track_ids.as_ref(),
            ctx.linked_selection.unwrap_or(true),
        )?
    } else {
        clips.clone()
    };
    Ok(
        json!({"result":{"clips":strings(&clips)},"context":{"activeSequence":sequence.to_string(),"sourceItem":source.to_string(),"sourcePlayhead":ctx.source_playhead.as_ref().map(n).transpose()?.unwrap_or(0).to_string(),"playhead":playhead.to_string(),"playheadFrames":r.frame_at(Tick(playhead)).to_string(),"selection":strings(&selected)},"sourceEdit":true,"documentText":String::from_utf8(doc.encode_preserved()?)?,"document":doc.envelope}),
    )
}
