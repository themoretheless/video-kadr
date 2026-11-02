//! Native conversion preview; the result is never implicitly applied to a project.
use crate::error::{ApiJson, AppError, AppResult};
use axum::Json;
use tokio::sync::Semaphore;
use weftcut_conversion::{Conversion, Request};
static CONVERSION_SLOT: Semaphore = Semaphore::const_new(1);

pub async fn position_conversion_handler(
    ApiJson(request): ApiJson<Request>,
) -> AppResult<Json<Conversion>> {
    let permit = CONVERSION_SLOT
        .acquire()
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        weftcut_conversion::convert(request)
    })
    .await
    .map_err(|error| AppError::bad_request(error.to_string()))?
    .map_err(AppError::bad_request)?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        serde_json::from_value(serde_json::json!({
          "nodes":null,"tracks":[
            {"keys":[{"t_us":0,"value":0,"in_":[0.6666666666666666,0.6666666666666666],"out":[0.3333333333333333,0.3333333333333333],"segment":{"kind":"Linear"}},{"t_us":1000000,"value":100,"in_":[0.6666666666666666,0.6666666666666666],"out":[0.3333333333333333,0.3333333333333333],"segment":{"kind":"Linear"}}],"extrapolate":{"before":"Hold","after":"Hold"}},
            {"keys":[{"t_us":0,"value":20,"in_":[0.6666666666666666,0.6666666666666666],"out":[0.3333333333333333,0.3333333333333333],"segment":{"kind":"Linear"}}],"extrapolate":{"before":"Hold","after":"Hold"}}
          ],"options":{"fpsNum":30,"fpsDen":1,"startFrame":0,"endFrame":30,"tolerancePx":0.5,"everyFrames":1,"xyMode":"editable"}
        })).unwrap()
    }
    #[tokio::test]
    async fn native_preview_fits_xy_and_refuses_invalid_options() {
        let result = position_conversion_handler(ApiJson(request()))
            .await
            .unwrap()
            .0;
        assert!(result.within_tolerance);
        assert!(result.max_error_px <= 0.5);
        assert!(result.nodes.as_ref().unwrap().len() <= 128);
        assert_eq!(result.tracks.len(), 1);
        let mut invalid = request();
        invalid.options.tolerance_px = 0.0;
        assert!(position_conversion_handler(ApiJson(invalid)).await.is_err());
    }
}
