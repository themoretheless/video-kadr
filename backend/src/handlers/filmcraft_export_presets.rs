//! Authenticated, owner-scoped preset library. File interchange uses JSON bodies.
use crate::{
    domain::filmcraft_export_presets::{Library, Preset, DEFAULT_PRESET},
    error::{ApiJson, AppError, AppResult},
    state::AppState,
};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Default, Deserialize)]
pub struct Filters {
    #[serde(default)]
    pub query: String,
    pub category: Option<String>,
    pub format: Option<String>,
    #[serde(default)]
    pub favorites: bool,
}

fn preset_json(library: &Library, preset: &Preset) -> Value {
    let mut value = serde_json::to_value(preset).expect("preset JSON serialization");
    value["builtin"] = json!(preset.builtin);
    value["favorite"] = json!(library.is_favorite(&preset.name));
    value
}

fn library_error(error: anyhow::Error) -> AppError {
    if error.downcast_ref::<sqlx::Error>().is_some() {
        AppError::internal("export preset library", error)
    } else {
        AppError::bad_request(error.to_string())
    }
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(filters): Query<Filters>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let library = state
        .db
        .filmcraft_export_presets(&owner)
        .await
        .map_err(library_error)?;
    let entries = library.list(
        &filters.query,
        filters.category.as_deref(),
        filters.format.as_deref(),
        filters.favorites,
    );
    Ok(Json(
        json!({"presets":entries.iter().map(|p|preset_json(&library,p)).collect::<Vec<_>>(),"default":DEFAULT_PRESET}),
    ))
}

#[derive(Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Command {
    Get {
        name: String,
    },
    Save {
        name: String,
        from: Option<String>,
        settings: Option<Value>,
        category: Option<String>,
        description: Option<String>,
        overwrite: Option<bool>,
        format: Option<Value>,
        width: Option<Value>,
        height: Option<Value>,
        fps: Option<Value>,
        scale: Option<Value>,
        audio: Option<Value>,
        quality: Option<Value>,
        bitrate_kbps: Option<Value>,
        max_bitrate_kbps: Option<Value>,
        bitrate_mode: Option<Value>,
        keyframe_distance: Option<Value>,
        hardware_encoding: Option<Value>,
        burn_captions: Option<Value>,
        prores_profile: Option<Value>,
        dnx_profile: Option<Value>,
        apv_profile: Option<Value>,
        mxf_video_codec: Option<Value>,
        sdr: Option<Value>,
        caption_sidecar: Option<Value>,
        loudness_lufs: Option<Value>,
    },
    Delete {
        name: String,
    },
    Favorite {
        name: String,
        favorite: Option<bool>,
    },
    Import {
        file: Value,
    },
    Export {
        names: Option<Vec<String>>,
    },
}

pub(super) fn merge(base: &mut Value, patch: Value) {
    match (base, patch) {
        (Value::Object(base), Value::Object(patch)) => {
            for (key, value) in patch {
                merge(base.entry(key).or_insert(Value::Null), value);
            }
        }
        (base, patch) => *base = patch,
    }
}

fn default_settings() -> Value {
    serde_json::to_value(crate::domain::filmcraft_export_preset_settings::ExportSettings::default())
        .expect("source default settings serialize")
}

pub async fn command(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(command): ApiJson<Command>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let result = state
        .db
        .edit_filmcraft_export_presets(&owner, move |library| {
            Ok(match command {
                Command::Get { name } => {
                    let preset = library
                        .find(&name)
                        .ok_or_else(|| anyhow::anyhow!("unknown preset"))?;
                    preset_json(library, &preset)
                }
                Command::Save {
                    name,
                    from,
                    settings,
                    category,
                    description,
                    overwrite,
                    format,
                    width,
                    height,
                    fps,
                    scale,
                    audio,
                    quality,
                    bitrate_kbps,
                    max_bitrate_kbps,
                    bitrate_mode,
                    keyframe_distance,
                    hardware_encoding,
                    burn_captions,
                    prores_profile,
                    dnx_profile,
                    apv_profile,
                    mxf_video_codec,
                    sdr,
                    caption_sidecar,
                    loudness_lufs,
                } => {
                    let mut resolved = match from {
                        Some(from) => {
                            library
                                .find(&from)
                                .ok_or_else(|| anyhow::anyhow!("unknown source preset"))?
                                .settings
                        }
                        None => default_settings(),
                    };
                    if let Some(settings) = settings {
                        anyhow::ensure!(settings.is_object(), "settings must be an object");
                        merge(&mut resolved, settings);
                    }
                    let mut flat = serde_json::Map::new();
                    if let Some(value) = format {
                        flat.insert("format".into(), value);
                    }
                    if let Some(value) = width {
                        flat.insert("width".into(), value);
                    }
                    if let Some(value) = height {
                        flat.insert("height".into(), value);
                    }
                    if let Some(value) = fps {
                        flat.insert("fps".into(), value);
                    }
                    if let Some(value) = scale {
                        flat.insert("scale".into(), value);
                    }
                    if let Some(value) = audio {
                        flat.insert("audio".into(), value);
                    }
                    if let Some(value) = quality {
                        flat.insert("quality".into(), value);
                    }
                    if let Some(value) = bitrate_kbps {
                        flat.insert("bitrateKbps".into(), value);
                    }
                    if let Some(value) = max_bitrate_kbps {
                        flat.insert("maxBitrateKbps".into(), value);
                    }
                    if let Some(value) = bitrate_mode {
                        flat.insert("bitrateMode".into(), value);
                    }
                    if let Some(value) = keyframe_distance {
                        flat.insert("keyframeDistance".into(), value);
                    }
                    if let Some(value) = hardware_encoding {
                        flat.insert("hardwareEncoding".into(), value);
                    }
                    if let Some(value) = burn_captions {
                        flat.insert("burnCaptions".into(), value);
                    }
                    if let Some(value) = prores_profile {
                        flat.insert("proresProfile".into(), value);
                    }
                    if let Some(value) = dnx_profile {
                        flat.insert("dnxProfile".into(), value);
                    }
                    if let Some(value) = apv_profile {
                        flat.insert("apvProfile".into(), value);
                    }
                    if let Some(value) = mxf_video_codec {
                        flat.insert("mxfVideoCodec".into(), value);
                    }
                    if let Some(value) = sdr {
                        flat.insert("sdr".into(), value);
                    }
                    if let Some(value) = caption_sidecar {
                        flat.insert("captionSidecar".into(), value);
                    }
                    if let Some(value) = loudness_lufs {
                        flat.insert("loudnessLufs".into(), value);
                    }
                    crate::domain::filmcraft_export_preset_overrides::apply(
                        &mut resolved,
                        &Value::Object(flat),
                    )?;
                    let format = resolved["format"].as_str().unwrap_or("export");
                    let description = description.unwrap_or_else(|| format!("{format} (custom)"));
                    library.save(
                        Preset {
                            name: name.clone(),
                            category: category.unwrap_or_else(|| "User".into()),
                            description,
                            settings: resolved,
                            builtin: false,
                        },
                        overwrite.unwrap_or(true),
                    )?;
                    json!({"name":name.trim()})
                }
                Command::Delete { name } => {
                    library.delete(&name)?;
                    json!({"deleted":name})
                }
                Command::Favorite { name, favorite } => {
                    let enabled = library.favorite(&name, favorite)?;
                    json!({"name":library.find(&name).unwrap().name,"favorite":enabled})
                }
                Command::Import { file } => {
                    json!({"imported":library.import(&serde_json::to_vec(&file)?)?})
                }
                Command::Export { names } => {
                    serde_json::to_value(library.export(names.as_deref())?)?
                }
            })
        })
        .await
        .map_err(library_error)?;
    Ok(Json(result))
}
