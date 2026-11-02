//! Native DSP registry transport; no factory allocation or PCM processing on this route.
use axum::Json;
use axum::http::StatusCode;
use filmcraft_audio_dsp::{Category, Unit};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectResponseRequest {
    effect: String,
    #[serde(default)]
    params: BTreeMap<String, f64>,
    #[serde(default)]
    animation: BTreeMap<String, crate::domain::composition::AnimatableValue>,
    #[serde(default)]
    local_time_seconds: f64,
    sample_rate: u32,
    #[serde(default)]
    frequencies: Vec<f64>,
    #[serde(default)]
    levels: Vec<f32>,
    #[serde(default)]
    band: usize,
}

/// Source configured() editor curves: resolved scalar project parameters, no PCM playback.
pub async fn project_response_handler(
    Json(request): Json<ProjectResponseRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let bad = |message: String| (StatusCode::BAD_REQUEST, Json(json!({"error":message})));
    if request.effect.len() > 128
        || request.params.len() > 128
        || request.animation.len() > 128
        || !request.local_time_seconds.is_finite()
        || request.frequencies.len() > 256
        || request.levels.len() > 256
        || request.band > 15
        || !(8000..=192000).contains(&request.sample_rate)
        || request
            .frequencies
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0 || *v > f64::from(request.sample_rate) / 2.0)
        || request
            .levels
            .iter()
            .any(|v| !v.is_finite() || !(-120.0..=24.0).contains(v))
    {
        return Err(bad("invalid audio response request".into()));
    }
    let authored = crate::domain::filmcraft_dsp::EffectSpec { concat_source_keys: Default::default(),
        id: format!("filmcraft:{}", request.effect),
        params: request
            .params
            .iter()
            .map(|(key, value)| (key.clone(), *value as f32))
            .collect(),
        bypass: false,
        animation: request.animation.clone(),
    };
    crate::domain::filmcraft_dsp::validate_specs(std::slice::from_ref(&authored))
        .map_err(|e| bad(e.to_string()))?;
    let mut resolved = request.params.clone();
    for (key, value) in authored
        .authored_params_at_seconds(request.local_time_seconds)
        .map_err(|e| bad(e.to_string()))?
    {
        if request.animation.contains_key(&key) {
            resolved.insert(key, value);
        }
    }
    let (spec, preroll) = crate::domain::filmcraft_audio_project::resolve(
        &request.effect,
        &resolved,
        false,
        request.sample_rate,
    )
    .map_err(|e| bad(e.to_string()))?;
    let response = tokio::task::spawn_blocking(move || {
        let mut effect = filmcraft_audio_dsp::create_effect(&spec.id, request.sample_rate as f32, 2).expect("validated DSP factory");
        for (id, value) in &spec.params { effect.set_param(id, *value); }
        json!({"dspId":spec.id, "params":spec.params, "latencySamples":effect.latency(), "prerollSamples":preroll,
            "responseDb":request.frequencies.iter().map(|f| effect.response_db(*f).filter(|v| v.is_finite())).collect::<Vec<_>>(),
            "transferDb":request.levels.iter().map(|v| effect.transfer_db(request.band, *v).filter(|v| v.is_finite())).collect::<Vec<_>>()})
    }).await.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":"audio response calculation failed"}))))?;
    Ok(Json(response))
}
pub async fn catalog_handler() -> Json<Value> {
    Json(json!({"schemaVersion": 1,
        "projectCatalog": crate::domain::filmcraft_audio_project::catalog(),
        "effects": filmcraft_audio_dsp::effects().iter().chain(crate::domain::concat_audio::effects()).map(|effect| {
        let category = match effect.category {
            Category::Filter => "filter", Category::Dynamics => "dynamics", Category::Time => "time",
            Category::Channel => "channel", Category::Restoration => "restoration", Category::Pitch => "pitch",
            Category::Modulation => "modulation", Category::Reverb => "reverb", Category::Special => "special",
        };
        let native_export = crate::domain::filmcraft_audio_mapping::native_export_preroll(&[crate::domain::filmcraft_dsp::EffectSpec { concat_source_keys: Default::default(), id: effect.id.into(), ..Default::default() }], 48000).is_ok();
        json!({"id": effect.id, "name": effect.name, "category": category, "nativeExport": native_export,
            "params": effect.params.iter().map(|p| {
                let unit = match p.unit {
                    Unit::None => "none", Unit::Decibels => "decibels", Unit::Hertz => "hertz",
                    Unit::Milliseconds => "milliseconds", Unit::Seconds => "seconds", Unit::Percent => "percent",
                    Unit::Q => "q", Unit::Ratio => "ratio", Unit::Semitones => "semitones", Unit::Bpm => "bpm",
                    Unit::Pan => "pan", Unit::Toggle => "toggle", Unit::Choice => "choice",
                };
                let mut parameter = json!({"id":p.id,"name":p.name,"min":p.min,"max":p.max,"default":p.default,
                    "unit":unit,"logScale":p.log_scale,"choices":p.choices});
                if effect.id.starts_with("concat.") { parameter["animatable"] = json!(false); parameter["step"] = json!(if effect.id == "concat.echo" { 0.05 } else if effect.id == "concat.stadium" { 0.5 } else { crate::domain::concat_tone::step(effect.id).unwrap_or(1.0) }); }
                parameter
            }).collect::<Vec<_>>()})
    }).chain(crate::tools::packet_clip::catalog()).collect::<Vec<_>>() }))
}
