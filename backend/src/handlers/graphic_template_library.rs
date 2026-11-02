use crate::{
    domain::graphic_template_library::GraphicTemplateLibrary,
    error::{ApiJson, AppError, AppResult},
    state::AppState,
};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
#[derive(Serialize)]
pub struct Entry {
    pub file: String,
    pub document: serde_json::Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Install {
    pub contents: String,
    #[serde(default)]
    pub replace: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Remove {
    pub file: String,
}
fn library(state: &AppState) -> GraphicTemplateLibrary {
    GraphicTemplateLibrary::new(state.storage.join("graphic-templates"))
}
pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<Entry>>> {
    let library = library(&state);
    let entries = tokio::task::spawn_blocking(move || library.list())
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(
        entries
            .into_iter()
            .map(|(file, document)| Entry {
                file,
                document: document.value().clone(),
            })
            .collect(),
    ))
}
pub async fn install(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<Install>,
) -> AppResult<Json<String>> {
    let library = library(&state);
    let file = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let document = crate::domain::graphic_template_file::GraphicTemplateFile::read(
            request.contents.as_bytes(),
        )?;
        let resources: crate::domain::graphic_template_fonts::Resources = serde_json::from_value(
            document
                .value()
                .get("resources")
                .cloned()
                .unwrap_or(serde_json::json!({})),
        )?;
        let file = if request.replace { library.install_replacing(request.contents.as_bytes())? }
        else { library.install(request.contents.as_bytes())? };
        crate::domain::graphic_template_fonts::register(&resources);
        Ok(file)
    })
    .await
    .map_err(|error| AppError::bad_request(error.to_string()))?
    .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(file))
}
pub async fn remove(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<Remove>,
) -> AppResult<Json<serde_json::Value>> {
    let library = library(&state);
    tokio::task::spawn_blocking(move || library.remove(&request.file))
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(serde_json::json!({"removed":true})))
}
