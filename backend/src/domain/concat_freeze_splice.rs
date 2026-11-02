//! Source-document FreezeFrame splice. No paths are opened or media IDs minted.
//! Native typed-track conversion/admission and project history remain orchestration-owned.
use super::concat_retime_edit::{
    self as edit, Clock, Edit, LanePlacement, PlanRequest, TrackGroups,
};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};

fn tracks(clip: &Value) -> Result<TrackGroups> {
    let mut tracks = TrackGroups::default();
    for key in clip["keys"].as_array().into_iter().flatten() {
        tracks
            .clip
            .entry(
                key["property"]
                    .as_str()
                    .context("missing key property")?
                    .into(),
            )
            .or_default()
            .push(serde_json::from_value(key.clone())?);
    }
    for (field, runs) in [
        ("filters", &mut tracks.audio),
        ("videoEffects", &mut tracks.video),
    ] {
        for (index, effect) in clip[field].as_array().into_iter().flatten().enumerate() {
            for (name, keys) in effect["keys"].as_object().into_iter().flatten() {
                runs.insert(
                    format!("{index}/{name}"),
                    serde_json::from_value(keys.clone())?,
                );
            }
        }
    }
    Ok(tracks)
}
fn source_run<T: serde::Serialize>(run: &T) -> Result<Value> {
    let mut value = serde_json::to_value(run)?;
    for key in value.as_array_mut().context("invalid key run")? {
        key.as_object_mut()
            .context("invalid key")?
            .remove("derived");
    }
    Ok(value)
}
fn apply(clip: &Value, part: &edit::PlannedPart) -> Result<Value> {
    let mut next = clip.clone();
    for (name, value) in serde_json::to_value(&part.clock)?.as_object().unwrap() {
        if value.is_null() {
            next.as_object_mut().context("invalid clip")?.remove(name);
        } else {
            next[name] = value.clone();
        }
    }
    next["fadeIn"] = json!(part.fade_in);
    next["fadeOut"] = json!(part.fade_out);
    let mut keys = vec![];
    for name in [
        "scale", "offsetX", "offsetY", "rotation", "opacity", "volume",
    ] {
        if let Some(run) = part.tracks.clip.get(name) {
            for mut key in source_run(run)?.as_array().unwrap().clone() {
                key["property"] = json!(name);
                keys.push(key);
            }
        }
    }
    if keys.is_empty() {
        next.as_object_mut().unwrap().remove("keys");
    } else {
        next["keys"] = json!(keys);
    }
    if part.role == "still" {
        next["filters"] = json!([]);
    }
    for (field, runs) in [
        ("filters", &part.tracks.audio),
        ("videoEffects", &part.tracks.video),
    ] {
        for (index, effect) in next[field].as_array_mut().into_iter().flatten().enumerate() {
            if let Some(keys) = effect["keys"].as_object_mut() {
                for (name, run) in keys {
                    *run = source_run(
                        runs.get(&format!("{index}/{name}"))
                            .context("missing effect run")?,
                    )?;
                }
            }
        }
    }
    Ok(next)
}
/// Input clips must already have source Editor defaults/normalization. Host-supplied
/// IDs and image registration are explicit; none are inferred from a client path.
pub fn freeze(
    clips: &[Value],
    target: &str,
    time: f64,
    hold: Option<f64>,
    still_media: Option<&str>,
    tail_id: &str,
    still_id: &str,
) -> Result<Option<Vec<Value>>> {
    let Some(picture) = clips.iter().find(|clip| clip["id"] == target) else {
        return Ok(None);
    };
    if !matches!(picture["kind"].as_str(), Some("video" | "image")) {
        return Ok(None);
    }
    let items: Vec<LanePlacement> = clips
        .iter()
        .map(|clip| {
            Ok(LanePlacement {
                id: clip["id"].as_str().context("missing clip ID")?.into(),
                track_id: clip["trackId"].as_str().context("missing track ID")?.into(),
                clock: serde_json::from_value::<Clock>(clip.clone())?,
            })
        })
        .collect::<Result<_>>()?;
    let Some(lane) = edit::freeze_lane(&items, target, time, hold)? else {
        return Ok(None);
    };
    let media = match (picture["kind"].as_str(), still_media) {
        (_, Some(id)) => id,
        (Some("image"), None) => picture["mediaId"]
            .as_str()
            .context("missing image media ID")?,
        _ => return Ok(None),
    };
    ensure!(
        !media.is_empty()
            && !tail_id.is_empty()
            && !still_id.is_empty()
            && tail_id != still_id
            && !items
                .iter()
                .any(|clip| clip.id == tail_id || clip.id == still_id),
        "invalid generated freeze identity"
    );
    let plan = edit::plan(PlanRequest {
        tracks: tracks(picture)?,
        fade_in: picture["fadeIn"].as_f64().unwrap_or(0.),
        fade_out: picture["fadeOut"].as_f64().unwrap_or(0.),
        clock: serde_json::from_value(picture.clone())?,
        rate_num: 30,
        rate_den: 1,
        command: Edit::Freeze {
            time,
            duration: hold,
        },
    })?;
    let head = apply(picture, &plan.parts[0])?;
    let mut tail = apply(picture, &plan.parts[2])?;
    tail["id"] = json!(tail_id);
    tail.as_object_mut().unwrap().remove("transitionIn");
    let mut still = apply(picture, &plan.parts[1])?;
    still["id"] = json!(still_id);
    still["kind"] = json!("image");
    still["mediaId"] = json!(media);
    still["volume"] = json!(1.);
    still["filters"] = json!([]);
    for name in ["muted", "detachedFrom", "transitionIn", "text"] {
        still.as_object_mut().unwrap().remove(name);
    }
    let shifted: std::collections::BTreeMap<_, _> = lane
        .shifted
        .iter()
        .map(|clip| (clip.id.as_str(), clip.clock.start))
        .collect();
    let mut output = Vec::with_capacity(clips.len() + 2);
    for clip in clips {
        if clip["id"] == target {
            output.push(head.clone());
            output.push(tail.clone());
        } else {
            let mut next = clip.clone();
            if let Some(start) = clip["id"].as_str().and_then(|id| shifted.get(id)) {
                next["start"] = json!(start);
            }
            output.push(next);
        }
    }
    output.push(still);
    Ok(Some(output))
}
