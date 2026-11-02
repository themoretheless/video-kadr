//! Filmcraft masks.* snapshot commands (MIT, pinned 52318524).
//! Attribution: docs/licenses/filmcraft-MIT.txt. Session state is explicit.
use super::{
    filmcraft_parameter_sample::sample, filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex, filmcraft_sequence_inspect::canonical_parameter,
    filmcraft_sequence_source::SourceTrackItem,
};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
fn id(v: Option<Value>) -> Result<Option<u64>> {
    super::filmcraft_project_inspect::id(v)
}
fn exact(mut v: Value) -> Value {
    super::filmcraft_project_inspect::exact_numbers(&mut v);
    v
}
fn clip_id(v: Option<&Value>) -> Option<u64> {
    v.and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}
fn pt(v: &Value) -> Option<[f64; 2]> {
    Some(if let Some(a) = v.as_array() {
        [a.first()?.as_f64()?, a.get(1)?.as_f64()?]
    } else {
        [v["x"].as_f64()?, v["y"].as_f64()?]
    })
}
fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: [f64; 2], b: f64) -> [f64; 2] {
    [a[0] * b, a[1] * b]
}
fn vertex(p: [f64; 2], t: [f64; 2]) -> Value {
    json!({"p":{"x":p[0],"y":p[1]},"t_in":{"x":-t[0],"y":-t[1]},"t_out":{"x":t[0],"y":t[1]}})
}
fn put(p: &mut Value, field: &str, v: [f64; 2]) {
    p[field] = json!({"x":v[0],"y":v[1]});
}
fn path(v: &Value) -> Result<Value> {
    Ok(super::filmcraft_effect_set_param::path(v)?["Path"].clone())
}
fn canonical_path(v: Value) -> Result<Value> {
    Ok(canonical_parameter(json!({"Path":v}))?["Path"].clone())
}
fn path_at(m: &Value, t: i64) -> Result<Value> {
    Ok(sample(&m["path"], t)?
        .get("Path")
        .cloned()
        .unwrap_or(json!({"vertices":[],"closed":false})))
}
fn set_at(p: &mut Value, t: i64, value: Value) -> Result<()> {
    let mut keys = p
        .get("keyframes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if keys.is_empty() {
        p["value"] = value;
        p.as_object_mut()
            .context("invalid parameter")?
            .remove("keyframes");
        return Ok(());
    }
    ensure!(
        keys.iter().all(|k| k["time"].as_i64().is_some()),
        "invalid key time"
    );
    match keys.binary_search_by_key(&t, |k| k["time"].as_i64().unwrap()) {
        Ok(i) => keys[i]["value"] = value,
        Err(i) => {
            let interp = keys
                .get(i.saturating_sub(1))
                .and_then(|k| k.get("interp"))
                .cloned()
                .unwrap_or(json!("Linear"));
            keys.insert(i,json!({"time":t,"value":value,"interp":interp,"in_influence":1./3.,"out_influence":1./3.}));
        }
    }
    p["keyframes"] = json!(keys);
    Ok(())
}
fn set_path(m: &mut Value, t: i64, p: Value) -> Result<()> {
    set_at(&mut m["path"], t, json!({"Path":canonical_path(p)?}))
}
pub(super) fn selector(
    effects: &[Value],
    v: Option<&Value>,
    sel: Option<&Value>,
    clip: u64,
) -> Option<usize> {
    match v {
        Some(Value::Number(n)) => n
            .as_u64()
            .and_then(|i| usize::try_from(i).ok())
            .filter(|i| *i < effects.len()),
        Some(Value::String(s)) => effects.iter().position(|e| e["effect"] == *s),
        _ => sel
            .filter(|s| id(s.get("clip").cloned()).ok().flatten() == Some(clip))
            .and_then(|s| s["effect"].as_u64())
            .and_then(|i| usize::try_from(i).ok())
            .filter(|i| *i < effects.len())
            .or_else(|| effects.iter().position(|e| e["effect"] == "opacity")),
    }
}
pub(super) fn clock(p: &Value, seq: &Value) -> Result<i64> {
    let ph = p
        .pointer("/context/playhead")
        .map(|v| match v {
            Value::String(s) => s.parse().context("invalid playhead"),
            v => v.as_i64().context("invalid playhead"),
        })
        .transpose()?
        .unwrap_or(0);
    let c = super::filmcraft_export_command_range::Clock {
        rate: serde_json::from_value(seq["settings"]["frame_rate"].clone())?,
        drop_frame: seq["settings"]["drop_frame"].as_bool().unwrap_or(false),
        playhead: filmcraft_time::Tick(ph),
    };
    Ok(c.time(&json!({"maskTime":p["time"],"maskFrame":p["frame"],"maskSeconds":p["seconds"],"maskTimecode":p["timecode"]}),"mask")?.unwrap_or(ph))
}
pub(super) fn media(item: &SourceTrackItem<'_>, ph: i64) -> Result<i64> {
    let end = item
        .end()
        .context("clip end overflow")?
        .checked_sub(1)
        .context("clip end underflow")?;
    ensure!(
        end >= item.timing.timeline_start,
        "non-positive clip duration"
    );
    if let Some(t) = item.timing.frame_hold {
        return Ok(t);
    }
    Ok(item
        .clocks(ph.clamp(item.timing.timeline_start, end))?
        .frame)
}
fn scalar(p: &Value, t: i64) -> Result<f64> {
    let v = sample(p, t)?;
    Ok(v["Float"]
        .as_f64()
        .or_else(|| v["Choice"].as_u64().map(|v| v as f64))
        .or_else(|| v["Bool"].as_bool().map(|v| u8::from(v) as f64))
        .unwrap_or(0.))
}
fn path_json(p: &Value) -> Value {
    json!({"closed":p["closed"],"vertices":p["vertices"].as_array().unwrap().iter().map(|v|json!({"p":pt(&v["p"]).unwrap(),"in":pt(&v["t_in"]).unwrap_or([0.,0.]),"out":pt(&v["t_out"]).unwrap_or([0.,0.])})).collect::<Vec<_>>()})
}
pub(super) fn target_clip(p: &Value, seq: &Value) -> Result<u64> {
    let sel = p.pointer("/context/selectedMask").filter(|v| v.is_object());
    Ok(clip_id(p.get("clip"))
        .or(id(sel.and_then(|s| s.get("clip")).cloned())?)
        .or_else(|| {
            p.pointer("/context/selection")
                .and_then(Value::as_array)
                .and_then(|selection| {
                    selection
                        .iter()
                        .filter_map(|v| id(Some(v.clone())).ok().flatten())
                        .find(|c| {
                            seq["video_tracks"].as_array().is_some_and(|tracks| {
                                tracks.iter().any(|tr| {
                                    tr["items"].as_array().is_some_and(|items| {
                                        items.iter().any(|i| i["id"].as_u64() == Some(*c))
                                    })
                                })
                            })
                        })
                })
        })
        .context("select a video clip or pass clip")?)
}
pub fn run(command: &str, p: Value) -> Result<Value> {
    ensure!(p.is_object(), "command params must be an object");
    let document = p.get("document").context("missing document")?;
    let bytes = match document {
        Value::String(s) => s.as_bytes().to_vec(),
        v => serde_json::to_vec(v)?,
    };
    let doc = SourceProjectDocument::read(&bytes)?;
    let index = ProjectIndex::new(&doc)?;
    let sequence = id(p.get("sequence").cloned())?
        .or(id(p.pointer("/context/activeSequence").cloned())?)
        .context("need active sequence")?;
    let seq = index
        .item(sequence)
        .and_then(|v| v.pointer("/kind/Sequence"))
        .context("missing sequence")?;
    let sel = p.pointer("/context/selectedMask").filter(|v| v.is_object());
    if command == "masks.list" {
        let ph = clock(&p, seq)?;
        let only = clip_id(p.get("clip"));
        let mut out = Vec::new();
        for tr in seq["video_tracks"]
            .as_array()
            .context("missing video tracks")?
        {
            for raw in tr["items"].as_array().context("missing track items")? {
                let item = SourceTrackItem::read(raw)?;
                if only.is_some_and(|c| c != item.id) {
                    continue;
                }
                let mt = media(&item, ph)?;
                for (ei, e) in raw["effects"]
                    .as_array()
                    .context("missing effects")?
                    .iter()
                    .enumerate()
                {
                    for (mi, m) in e
                        .get("masks")
                        .and_then(Value::as_array)
                        .map(Vec::as_slice)
                        .unwrap_or(&[])
                        .iter()
                        .enumerate()
                    {
                        let method = match m["track_method"]
                            .as_str()
                            .unwrap_or("PositionScaleRotation")
                        {
                            "Position" => "Position",
                            "PositionRotation" => "Position & Rotation",
                            _ => "Position, Scale & Rotation",
                        };
                        out.push(json!({"clip":item.id,"effect":ei,"effectId":e["effect"],"mask":mi,"name":m["name"],"mode":m.get("mode").cloned().unwrap_or(json!("Add")),"inverted":m["inverted"].as_bool().unwrap_or(false),"trackMethod":method,"feather":scalar(&m["feather"],mt)?,"opacity":scalar(&m["opacity"],mt)?,"expansion":scalar(&m["expansion"],mt)?,"path":path_json(&path_at(m,mt)?),"pathKeyframes":m["path"].get("keyframes").and_then(Value::as_array).map(|k|k.iter().map(|k|k["time"].clone()).collect::<Vec<_>>()).unwrap_or_default(),"selected":sel.is_some_and(|s|id(s.get("clip").cloned()).ok().flatten()==Some(item.id)&&s["effect"].as_u64()==Some(ei as u64)&&s["mask"].as_u64()==Some(mi as u64))}));
                    }
                }
            }
        }
        return Ok(exact(json!({"masks":out})));
    }
    let mut selected = sel.cloned().unwrap_or(Value::Null);
    let mut changed = doc.clone();
    let clear = command == "masks.select"
        && (p["none"] == true
            || p.as_object()
                .unwrap()
                .keys()
                .all(|k| ["document", "sequence", "context"].contains(&k.as_str())));
    let result = if clear {
        selected = Value::Null;
        Value::Null
    } else {
        let clip = target_clip(&p, seq)?;
        let (location, _locked) = super::filmcraft_parameter_edit::location(&doc, sequence, clip)?;
        let item = SourceTrackItem::read(doc.envelope.pointer(&location).unwrap())?;
        let effects = item.raw["effects"].as_array().context("missing effects")?;
        let ei = selector(
            effects,
            p.get("effect"),
            if matches!(command, "masks.add" | "masks.select") {
                None
            } else {
                sel
            },
            clip,
        )
        .context("no such effect")?;
        let effect = &mut changed.envelope.pointer_mut(&location).unwrap()["effects"][ei];
        if effect.get("masks").is_none() {
            effect["masks"] = json!([]);
        }
        let kind = effect["effect"]
            .as_str()
            .context("missing effect ID")?
            .to_owned();
        let masks = effect["masks"].as_array_mut().context("invalid masks")?;
        if command == "masks.add" {
            ensure!(
                super::filmcraft_parameter_edit::maskable_effect(&kind),
                "masks require a video effect other than Motion"
            );
            let (w, h) = source_size(&index, item.source_id, seq);
            let center = p.get("center").and_then(pt).unwrap_or([w / 2., h / 2.]);
            let r = w.min(h) * 0.2;
            let size = p.get("size").and_then(pt).unwrap_or([r * 2., r * 2.]);
            let shape = p["shape"]
                .as_str()
                .unwrap_or("ellipse")
                .to_ascii_lowercase();
            let shape = if let Some(v) = p.get("path") {
                path(v)?
            } else {
                let c = center;
                let rx = size[0] * 0.5;
                let ry = size[1] * 0.5;
                match shape.as_str() {
                    "ellipse" => {
                        let k = 0.552_284_749_830_793_4;
                        json!({"closed":true,"vertices":[vertex(add(c,[0.,-ry]),[rx*k,0.]),vertex(add(c,[rx,0.]),[0.,ry*k]),vertex(add(c,[0.,ry]),[-rx*k,0.]),vertex(add(c,[-rx,0.]),[0.,-ry*k])]})
                    }
                    "polygon" | "rectangle" | "rect" | "4-point" => {
                        json!({"closed":true,"vertices":[vertex([c[0]-size[0]/2.,c[1]-size[1]/2.],[0.,0.]),vertex([c[0]+size[0]/2.,c[1]-size[1]/2.],[0.,0.]),vertex([c[0]+size[0]/2.,c[1]+size[1]/2.],[0.,0.]),vertex([c[0]-size[0]/2.,c[1]+size[1]/2.],[0.,0.])]})
                    }
                    _ => anyhow::bail!("bezier mask needs path"),
                }
            };
            ensure!(
                shape["closed"] == false
                    || shape["vertices"]
                        .as_array()
                        .context("invalid vertices")?
                        .len()
                        >= 3,
                "a closed mask needs three vertices"
            );
            let n = masks
                .iter()
                .filter_map(|m| {
                    m["name"]
                        .as_str()?
                        .strip_prefix("Mask (")?
                        .strip_suffix(')')?
                        .parse::<usize>()
                        .ok()
                })
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .context("mask name overflow")?;
            let name = format!("Mask ({n})");
            let mi = masks.len();
            masks.push(json!({"name":name,"path":{"value":{"Path":canonical_path(shape)?}},"feather":{"value":{"Float":10.}},"opacity":{"value":{"Float":100.}},"expansion":{"value":{"Float":0.}},"inverted":false,"mode":"Add","track_method":"PositionScaleRotation"}));
            selected = json!({"clip":clip,"effect":ei,"mask":mi});
            json!({"clip":clip,"effect":ei,"mask":mi,"name":name})
        } else {
            let mi = if command == "masks.select" {
                p["mask"].as_u64().unwrap_or(0)
            } else {
                p["mask"]
                    .as_u64()
                    .or_else(|| {
                        sel.filter(|s| id(s.get("clip").cloned()).ok().flatten() == Some(clip))
                            .and_then(|s| s["mask"].as_u64())
                    })
                    .context("need mask")?
            };
            let mi = usize::try_from(mi)?;
            ensure!(mi < masks.len(), "no such mask");
            if command == "masks.select" {
                selected = json!({"clip":clip,"effect":ei,"mask":mi});
            } else if command == "masks.remove" {
                masks.remove(mi);
                if sel.is_some_and(|s| {
                    id(s.get("clip").cloned()).ok().flatten() == Some(clip)
                        && s["effect"].as_u64() == Some(ei as u64)
                }) {
                    selected = Value::Null;
                }
            } else {
                let mt = media(&item, clock(&p, seq)?)?;
                edit(&mut masks[mi], mt, command, &p)?;
            }
            if masks.is_empty() {
                effect.as_object_mut().unwrap().remove("masks");
            }
            Value::Null
        }
    };
    let text = String::from_utf8(changed.encode_preserved()?)?;
    Ok(
        json!({"result":exact(result),"document":changed.envelope,"documentText":text,"context":exact(json!({"selectedMask":selected}))}),
    )
}
fn source_size(index: &ProjectIndex<'_>, id: u64, seq: &Value) -> (f64, f64) {
    let size = index
        .item(id)
        .and_then(|i| i.get("kind"))
        .and_then(|k| {
            if let Some(s) = k.get("Subclip") {
                let parent = index.item(s["parent"].as_u64()?)?;
                let media = parent.pointer("/kind/Media/info/video")?;
                Some((media["width"].as_u64()?, media["height"].as_u64()?))
            } else {
                index.source_size(id).map(|(w, h)| (w as u64, h as u64))
            }
        })
        .unwrap_or((
            seq["settings"]["width"].as_u64().unwrap_or(1920),
            seq["settings"]["height"].as_u64().unwrap_or(1080),
        ));
    (size.0 as f64, size.1 as f64)
}
fn edit(m: &mut Value, t: i64, command: &str, p: &Value) -> Result<()> {
    if command == "masks.set" {
        if let Some(s) = p["name"].as_str() {
            m["name"] = json!(s);
        }
        if let Some(v) = p["inverted"].as_bool() {
            m["inverted"] = json!(v);
        }
        if let Some(s) = p["mode"].as_str() {
            let mode = [
                "None",
                "Add",
                "Subtract",
                "Intersect",
                "Lighten",
                "Darken",
                "Difference",
            ]
            .into_iter()
            .find(|v| v.eq_ignore_ascii_case(s))
            .context("unknown mask mode")?;
            m["mode"] = json!(mode);
        }
        if let Some(s) = p["trackMethod"].as_str() {
            let key = s
                .to_ascii_lowercase()
                .replace([' ', '_', '-', '&', ','], "");
            m["track_method"] = json!(match key.as_str() {
                "position" => "Position",
                "positionrotation" => "PositionRotation",
                "positionscalerotation" => "PositionScaleRotation",
                _ => anyhow::bail!("unknown tracking method"),
            });
        }
        for name in ["feather", "opacity", "expansion"] {
            if let Some(v) = p[name].as_f64() {
                set_at(
                    &mut m[name],
                    t,
                    json!({"Float":match name{"feather"=>v.max(0.),"opacity"=>v.clamp(0.,100.),_=>v}}),
                )?;
            }
        }
        if let Some(v) = p.get("path") {
            set_path(m, t, path(v)?)?;
        }
        return Ok(());
    }
    let mut current = path_at(m, t)?;
    let vi = p["vertex"].as_u64().map(usize::try_from).transpose()?;
    if command == "masks.removeVertex" {
        ensure!(
            current["vertices"]
                .as_array()
                .context("invalid vertices")?
                .len()
                > 3,
            "a mask keeps at least three vertices"
        );
        let vi = vi.context("need vertex")?;
        let drop = |v: &mut Value| {
            if let Some(vertices) = v
                .pointer_mut("/Path/vertices")
                .and_then(Value::as_array_mut)
            {
                if vi < vertices.len() {
                    vertices.remove(vi);
                }
            }
        };
        drop(&mut m["path"]["value"]);
        if let Some(keys) = m["path"].get_mut("keyframes").and_then(Value::as_array_mut) {
            for k in keys {
                drop(&mut k["value"]);
            }
        }
        return Ok(());
    }
    if command == "masks.addVertex" {
        let after = usize::try_from(p["after"].as_u64().context("need after")?)?;
        let at = p.get("at").and_then(pt).context("need at")?;
        let key_count = m["path"]["keyframes"].as_array().map_or(0, Vec::len);
        if key_count > 1 {
            for k in m["path"]["keyframes"].as_array_mut().unwrap() {
                if let Some(v) = k
                    .pointer_mut("/value/Path/vertices")
                    .and_then(Value::as_array_mut)
                {
                    ensure!(!v.is_empty(), "cannot insert into an empty animated path");
                    let i = after
                        .checked_add(1)
                        .context("vertex overflow")?
                        .min(v.len());
                    let a = pt(&v[after.min(v.len() - 1)]["p"]).unwrap();
                    let b = pt(&v[i % v.len()]["p"]).unwrap();
                    v.insert(i, vertex(add(a, mul(sub(b, a), 0.5)), [0., 0.]));
                }
            }
            for key in m["path"]["keyframes"].as_array_mut().unwrap() {
                key["value"] = canonical_parameter(key["value"].clone())?;
            }
            current = path_at(m, t)?;
        }
        let v = current["vertices"]
            .as_array_mut()
            .context("invalid vertices")?;
        let i = after
            .checked_add(1)
            .context("vertex overflow")?
            .min(v.len());
        if key_count > 1 {
            ensure!(i < v.len(), "incompatible animated path topology");
            v[i] = vertex(at, [0., 0.]);
        } else {
            v.insert(i, vertex(at, [0., 0.]));
        }
        return set_path(m, t, current);
    }
    let vertices = current["vertices"]
        .as_array_mut()
        .context("invalid vertices")?;
    match command {
        "masks.translate" => {
            let d = p.get("delta").and_then(pt).context("need delta")?;
            for v in vertices {
                let q = pt(&v["p"]).unwrap();
                put(
                    v,
                    "p",
                    [1. * q[0] + 0. * q[1] + d[0], 0. * q[0] + 1. * q[1] + d[1]],
                );
                for name in ["t_in", "t_out"] {
                    let q = pt(&v[name]).unwrap_or([0., 0.]);
                    put(
                        v,
                        name,
                        [1. * q[0] + 0. * q[1] + 0., 0. * q[0] + 1. * q[1] + 0.],
                    );
                }
            }
        }
        "masks.moveVertex" => {
            let vi = vi.context("need vertex")?;
            let v = vertices.get_mut(vi).context("no such vertex")?;
            let base = pt(&v["p"]).unwrap();
            let to = p.get("to").and_then(pt);
            let delta = p.get("delta").and_then(pt);
            ensure!(to.is_some() || delta.is_some(), "need to or delta");
            let handle = p["handle"].as_str().unwrap_or("point");
            if matches!(handle, "in" | "out") {
                let name = if handle == "in" { "t_in" } else { "t_out" };
                let cur = add(base, pt(&v[name]).unwrap_or([0., 0.]));
                let tan = sub(
                    to.unwrap_or_else(|| add(cur, delta.unwrap_or([0., 0.]))),
                    base,
                );
                put(v, name, tan);
                if p["breakHandles"] != true {
                    put(
                        v,
                        if name == "t_in" { "t_out" } else { "t_in" },
                        mul(tan, -1.),
                    );
                }
            } else {
                put(
                    v,
                    "p",
                    to.unwrap_or_else(|| add(base, delta.unwrap_or([0., 0.]))),
                );
            }
        }
        "masks.toggleVertexSmooth" => {
            let vi = vi.context("need vertex")?;
            let n = vertices.len();
            ensure!(vi < n, "no such vertex");
            let v = &vertices[vi];
            let base = pt(&v["p"]).unwrap();
            let corner = pt(&v["t_in"]).unwrap_or([0., 0.]) == [0., 0.]
                && pt(&v["t_out"]).unwrap_or([0., 0.]) == [0., 0.];
            let tan = if corner {
                mul(
                    sub(
                        pt(&vertices[(vi + 1) % n]["p"]).unwrap(),
                        pt(&vertices[(vi + n - 1) % n]["p"]).unwrap(),
                    ),
                    1. / 6.,
                )
            } else {
                [0., 0.]
            };
            vertices[vi] = vertex(base, tan);
        }
        _ => anyhow::bail!("unsupported mask command"),
    }
    set_path(m, t, current)
}
