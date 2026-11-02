use crate::{
    domain::{graphic_pin::Pin, graphic_pin_command::GraphicPinCapture},
    error::{ApiJson, AppError, AppResult},
};
use axum::Json;
use serde::Serialize;
#[derive(Serialize)]
pub struct PinResponse {
    pub pin: Pin,
}
pub async fn capture(ApiJson(request): ApiJson<GraphicPinCapture>) -> AppResult<Json<PinResponse>> {
    let pin = request
        .execute()
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(PinResponse { pin }))
}
