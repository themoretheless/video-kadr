use crate::{
    domain::{graphic_text_edit_command::TextEditRequest, graphic_text_runs::CharacterStyleRun},
    error::{ApiJson, AppError, AppResult},
};
use axum::Json;
use serde::Serialize;
#[derive(Serialize)]
pub struct TextEditResponse {
    pub runs: Vec<CharacterStyleRun>,
}
pub async fn graphic_text_edit_handler(
    ApiJson(request): ApiJson<TextEditRequest>,
) -> AppResult<Json<TextEditResponse>> {
    let runs = request
        .execute()
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(TextEditResponse { runs }))
}
