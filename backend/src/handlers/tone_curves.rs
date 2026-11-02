use crate::{
    domain::tone_curves::{curve_lut, periodic_curve_lut},
    error::{ApiJson, AppError, AppResult},
};
use axum::Json;
use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CurveRequest {
    points: Vec<[f32; 2]>,
    samples: usize,
    #[serde(default)]
    periodic: bool,
}
#[derive(Serialize)]
pub struct CurveResponse {
    values: Vec<f32>,
}
pub async fn tone_curve_handler(
    ApiJson(request): ApiJson<CurveRequest>,
) -> AppResult<Json<CurveResponse>> {
    let values = (if request.periodic {
        periodic_curve_lut(&request.points, request.samples)
            .map(|values| values.unwrap_or_else(|| vec![0.5; request.samples]))
    } else {
        curve_lut(&request.points, request.samples)
    })
    .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(CurveResponse { values }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConcatCurveRequest {
    points: Vec<[f64; 2]>,
    samples: usize,
}
pub async fn concat_curve_handler(
    ApiJson(request): ApiJson<ConcatCurveRequest>,
) -> AppResult<Json<CurveResponse>> {
    if !(2..=4096).contains(&request.samples) {
        return Err(AppError::bad_request("curve samples out of range"));
    }
    let curve = crate::domain::concat_curves::PreparedCurve::new(&request.points)
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let values = (0..request.samples)
        .map(|i| curve.sample(i as f32 / (request.samples - 1) as f32))
        .collect();
    Ok(Json(CurveResponse { values }))
}
