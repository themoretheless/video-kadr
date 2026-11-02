//! effects.apply from pinned Filmcraft commands.rs (MIT). Explicit snapshot context.
use super::{
    filmcraft_effect_catalog::definitions, filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct SessionContext {
    active_sequence: Option<Value>,
    selection: Vec<Value>,
    playhead: Option<Value>,
    targeted_tracks: Option<Vec<Value>>,
    video_transition_duration: Option<f64>,
    video_transition_unit: Option<String>,
    audio_transition_duration: Option<f64>,
    audio_transition_unit: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    document: Value,
    sequence: Option<Value>,
    effect: String,
    clips: Option<Vec<Value>>,
    clip: Option<Value>,
    #[serde(default)]
    context: SessionContext,
    frames: Option<i64>,
    edge: Option<String>,
    params: Option<Value>,
    #[serde(default)]
    reverse: bool,
}
fn ids(values: Vec<Value>) -> Result<Vec<u64>> {
    values
        .into_iter()
        .map(|v| super::filmcraft_project_inspect::id(Some(v))?.context("invalid selected ID"))
        .collect()
}
fn preference(value: f64, unit: &str, num: i64, den: i64) -> Result<i64> {
    ensure!(value.is_finite(), "invalid transition preference");
    let estimated = if unit == "frames" {
        value.max(1.).round() * 254016000000. * den as f64 / num as f64
    } else {
        value.max(0.) * 254016000000.
    };
    ensure!(
        estimated.is_finite() && estimated <= (i64::MAX / 4) as f64,
        "transition preference exceeds source clock"
    );
    super::filmcraft_transition::preference_duration(value, unit, num, den)
}
pub fn apply(params: Value) -> Result<Value> {
    let request: Request = serde_json::from_value(params)?;
    let sequence = super::filmcraft_project_inspect::id(request.sequence)?
        .or(super::filmcraft_project_inspect::id(
            request.context.active_sequence,
        )?)
        .context("effects.apply requires active sequence")?;
    let clip = super::filmcraft_project_inspect::id(request.clip)?;
    let selected = if let Some(clips) = request.clips {
        ids(clips)?
    } else if let Some(clip) = clip {
        vec![clip]
    } else {
        ids(request.context.selection)?
    };
    let bytes = match request.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let mut document = SourceProjectDocument::read(&bytes)?;
    let index = ProjectIndex::new(&document)?;
    let seq = index
        .item(sequence)
        .and_then(|v| v.pointer("/kind/Sequence"))
        .context("source sequence missing")?;
    let audio_selection = !selected.is_empty()
        && selected.iter().all(|id| {
            ["video_tracks", "audio_tracks"]
                .into_iter()
                .flat_map(|key| seq[key].as_array().into_iter().flatten())
                .find(|track| {
                    track["items"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|c| c["id"].as_u64() == Some(*id))
                })
                .is_some_and(|track| track["kind"] == "Audio")
        });
    let exact = definitions().iter().find(|d| d["id"] == request.effect);
    let named: Vec<_> = definitions()
        .iter()
        .filter(|d| {
            d["name"]
                .as_str()
                .unwrap()
                .eq_ignore_ascii_case(&request.effect)
        })
        .collect();
    let definition = exact
        .or_else(|| {
            if named.len() <= 1 {
                named.first().copied().or_else(|| {
                    definitions()
                        .iter()
                        .find(|d| d["id"] == request.effect.to_ascii_lowercase())
                })
            } else {
                let want = if audio_selection { "Audio" } else { "Video" };
                named
                    .iter()
                    .copied()
                    .find(|d| d["intrinsic"] != true && d["kind"] == want)
                    .or_else(|| named.iter().copied().find(|d| d["intrinsic"] != true))
                    .or_else(|| named.first().copied())
            }
        })
        .context("unknown effect")?;
    let effect = definition["id"].as_str().unwrap();
    ensure!(
        !matches!(effect, "graphic_text" | "graphic_shape"),
        "graphic layers are added with graphics.newText / graphics.newShape"
    );
    let result = if matches!(
        definition["kind"].as_str(),
        Some("VideoTransition" | "AudioTransition")
    ) {
        let video = definition["kind"] == "VideoTransition";
        let kind = if video {
            "video_tracks"
        } else {
            "audio_tracks"
        };
        let rate: filmcraft_time::FrameRate =
            serde_json::from_value(seq["settings"]["frame_rate"].clone())?;
        ensure!(
            rate.num > 0
                && rate.den > 0
                && rate.num <= u32::MAX as i64
                && rate.den <= u32::MAX as i64,
            "invalid transition rate"
        );
        let duration = if let Some(frames) = request.frames {
            i64::try_from(
                i128::from(frames)
                    .checked_mul(254016000000i128)
                    .and_then(|v| v.checked_mul(i128::from(rate.den)))
                    .context("transition frame clock overflow")?
                    .div_euclid(i128::from(rate.num)),
            )?
        } else {
            preference(
                if video {
                    request.context.video_transition_duration.unwrap_or(30.)
                } else {
                    request.context.audio_transition_duration.unwrap_or(1.)
                },
                if video {
                    request
                        .context
                        .video_transition_unit
                        .as_deref()
                        .unwrap_or("frames")
                } else {
                    request
                        .context
                        .audio_transition_unit
                        .as_deref()
                        .unwrap_or("seconds")
                },
                rate.num,
                rate.den,
            )?
        };
        let targeted = if let Some(targets) = request.context.targeted_tracks {
            ids(targets)?
        } else {
            ["video_tracks", "audio_tracks"]
                .into_iter()
                .flat_map(|key| seq[key].as_array().into_iter().flatten())
                .map(|t| t["id"].as_u64().context("invalid track ID"))
                .collect::<Result<Vec<_>>>()?
        };
        let time = match request.context.playhead {
            None => 0,
            Some(Value::String(s)) => s.parse()?,
            Some(v) => v.as_i64().context("invalid transition playhead")?,
        };
        let edge = match request.edge.as_deref() {
            Some("in") => Some(super::filmcraft_trim::Edge::In),
            Some("out") => Some(super::filmcraft_trim::Edge::Out),
            _ => None,
        };
        let target = super::filmcraft_trim::plan_transition(
            &document, sequence, kind, &targeted, clip, edge, time, duration,
        )?;
        let mut instance = super::filmcraft_transition::instance(
            effect,
            if video {
                "VideoTransition"
            } else {
                "AudioTransition"
            },
        )?;
        if let Some(params) = request.params {
            super::filmcraft_transition::set_params(&mut instance, &params)?;
        }
        let transition = json!({"id":0,"effect":instance,"start":target["start"],"duration":duration,"from":target["from"],"to":target["to"],"align":"CenterAtCut","reverse":request.reverse});
        let id = super::filmcraft_trim::add_transition(
            &mut document,
            sequence,
            target["track"].as_u64().unwrap(),
            &transition,
        )?;
        json!({"transition":id})
    } else {
        ensure!(!selected.is_empty(), "select clips first");
        let width = u32::try_from(
            seq["settings"]["width"]
                .as_u64()
                .context("invalid sequence width")?,
        )?;
        let height = u32::try_from(
            seq["settings"]["height"]
                .as_u64()
                .context("invalid sequence height")?,
        )?;
        let audio = definition["kind"] == "Audio";
        let seq = document
            .envelope
            .pointer_mut(&format!("/project/items/{sequence}/kind/Sequence"))
            .unwrap();
        for key in ["video_tracks", "audio_tracks"] {
            for track in seq[key].as_array_mut().context("missing source tracks")? {
                if (track["kind"] == "Audio") != audio {
                    continue;
                }
                for item in track["items"]
                    .as_array_mut()
                    .context("missing source items")?
                {
                    if !item["id"].as_u64().is_some_and(|id| selected.contains(&id)) {
                        continue;
                    }
                    let effects = item["effects"]
                        .as_array_mut()
                        .context("missing clip effects")?;
                    let position = if audio {
                        effects.len()
                    } else {
                        effects
                            .iter()
                            .position(|e| {
                                definitions()
                                    .iter()
                                    .any(|d| d["id"] == e["effect"] && d["intrinsic"] == true)
                            })
                            .unwrap_or(effects.len())
                    };
                    effects.insert(
                        position,
                        super::filmcraft_effect_edit::fresh(definition, width, height),
                    );
                }
            }
        }
        Value::Null
    };
    let document_text = String::from_utf8(document.encode_preserved()?)?;
    Ok(
        json!({"result":result,"document":document.envelope,"documentText":document_text,"sequence":sequence.to_string()}),
    )
}
