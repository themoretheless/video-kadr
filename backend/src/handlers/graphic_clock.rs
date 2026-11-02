use crate::{
    domain::graphic_responsive_time::ResponsiveTime,
    error::{ApiJson, AppError, AppResult},
};
use axum::Json;
use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClockRequest {
    pub responsive_time: Option<ResponsiveTime>,
    pub relative_us: i64,
    pub duration_us: i64,
    #[serde(default)]
    pub animation_offset_us: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockResponse {
    pub local_us: i64,
}
pub async fn map_clock(ApiJson(request): ApiJson<ClockRequest>) -> AppResult<Json<ClockResponse>> {
    if request.duration_us <= 0 || request.animation_offset_us < 0 {
        return Err(AppError::bad_request("invalid graphic clock"));
    }
    let local = if let Some(meta) = request.responsive_time.filter(|meta| meta.active()) {
        meta.remap(request.relative_us, request.duration_us)
            .map_err(|error| AppError::bad_request(error.to_string()))?
    } else {
        request
            .relative_us
            .checked_add(request.animation_offset_us)
            .ok_or_else(|| AppError::bad_request("graphic clock overflow"))?
    };
    if local.unsigned_abs() > 9_007_199_254_740_991 {
        return Err(AppError::bad_request(
            "graphic clock exceeds editor precision",
        ));
    }
    Ok(Json(ClockResponse { local_us: local }))
}
