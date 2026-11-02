//! Pure original Concat link conversion; no project/media is read or modified.
use crate::{
    domain::concat_effect_upgrade::{native_effect, native_transition, upgrade, AppliedFilter},
    error::{ApiJson, AppError, AppResult},
};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    schema_version: u32,
    links: Vec<AppliedFilter>,
    #[serde(default)]
    include_native_effects: bool,
    #[serde(default)]
    include_native_transitions: bool,
}
pub async fn upgrade_handler(ApiJson(request): ApiJson<Request>) -> AppResult<Json<Value>> {
    if request.schema_version != 1 || request.links.len() > 128 {
        return Err(AppError::bad_request(
            "Unsupported Concat upgrade schema or too many links",
        ));
    }
    let result = request
        .links
        .into_iter()
        .map(upgrade)
        .collect::<anyhow::Result<Vec<_>>>()
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let mut response = json!({"schemaVersion":1,"links":result});
    if request.include_native_effects {
        let effects = result
            .iter()
            .map(|r| native_effect(&r.link))
            .collect::<anyhow::Result<Vec<_>>>()
            .map_err(|error| AppError::bad_request(error.to_string()))?;
        response["nativeEffects"] = serde_json::to_value(effects)
            .map_err(|error| AppError::bad_request(error.to_string()))?;
    }
    if request.include_native_transitions {
        let transitions = result
            .iter()
            .map(|r| native_transition(&r.link))
            .collect::<anyhow::Result<Vec<_>>>()
            .map_err(|error| AppError::bad_request(error.to_string()))?;
        response["nativeTransitions"] = serde_json::to_value(transitions)
            .map_err(|error| AppError::bad_request(error.to_string()))?;
    }
    Ok(Json(response))
}
