use crate::domain::animated_color::{sample_colors, Color, ColorSampleRequest};
use crate::error::{ApiJson, AppError, AppResult};
use axum::Json;
use serde::Serialize;
use tokio::sync::Semaphore;

static COLOR_SLOTS: Semaphore = Semaphore::const_new(2);

#[derive(Serialize)]
pub struct ColorSampleResponse {
    pub colors: Vec<Color>,
}

pub async fn animated_color_handler(
    ApiJson(request): ApiJson<ColorSampleRequest>,
) -> AppResult<Json<ColorSampleResponse>> {
    let permit = COLOR_SLOTS
        .acquire()
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let colors = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        sample_colors(&request)
    })
    .await
    .map_err(|error| AppError::bad_request(error.to_string()))?
    .map_err(AppError::bad_request)?;
    Ok(Json(ColorSampleResponse { colors }))
}
