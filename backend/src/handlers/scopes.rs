use crate::{
    domain::{
        group_surface::GroupSurface,
        scopes::analyze::{analyze, Options},
    },
    error::{ApiJson, AppError, AppResult},
};
use axum::{body::Bytes, http::HeaderMap, Json};
use serde::Deserialize;
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub frame: super::color_match::MatchFrame,
    #[serde(default)]
    pub options: Options,
}
static SLOTS: Semaphore = Semaphore::const_new(2);
pub async fn scopes_handler(ApiJson(request): ApiJson<Request>) -> AppResult<Json<Value>> {
    run_analysis(
        GroupSurface {
            width: request.frame.width,
            height: request.frame.height,
            rgba: request.frame.rgba,
        },
        request.options,
    )
    .await
}
pub async fn scopes_binary_handler(headers: HeaderMap, body: Bytes) -> AppResult<Json<Value>> {
    let dimension = |name: &str| -> AppResult<usize> {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or_else(|| AppError::bad_request("invalid scope frame dimensions"))
    };
    let options = match headers.get("x-scope-options") {
        None => Options::default(),
        Some(value) => {
            let text = value
                .to_str()
                .map_err(|_| AppError::bad_request("invalid scope options"))?;
            if text.len() > 2048 {
                return Err(AppError::bad_request("scope options too large"));
            }
            serde_json::from_str(text).map_err(|e| AppError::bad_request(e.to_string()))?
        }
    };
    let frame = GroupSurface {
        width: dimension("x-frame-width")?,
        height: dimension("x-frame-height")?,
        rgba: body.to_vec(),
    };
    frame
        .validate()
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    run_analysis(frame, options).await
}
async fn run_analysis(frame: GroupSurface, options: Options) -> AppResult<Json<Value>> {
    let permit = SLOTS
        .try_acquire()
        .map_err(|_| AppError::too_many_requests("слишком много расчётов Scopes"))?;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let deadline = Instant::now() + Duration::from_secs(10);
        analyze(&frame, &options, &mut || {
            anyhow::ensure!(Instant::now() < deadline, "Scopes timeout");
            Ok(())
        })
    })
    .await
    .map_err(|e| AppError::internal("join native scopes", e))?
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(result))
}
