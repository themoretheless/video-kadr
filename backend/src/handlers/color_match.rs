use crate::{
    domain::{
        color_match::{solve_with_library, Match},
        group_surface::GroupSurface,
        linear_color_frame::LinearColorFrame,
        lumetri::Lumetri,
    },
    error::{ApiJson, AppError, AppResult},
};
use axum::Json;
use serde::Deserialize;
use std::{
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MatchFrame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MatchRequest {
    pub current: MatchFrame,
    pub reference: MatchFrame,
    #[serde(default)]
    pub parameters: Lumetri,
    #[serde(default)]
    pub skin_protect: bool,
    #[serde(default)]
    pub luts: Vec<crate::domain::project_luts::ProjectLut>,
}
pub async fn color_match_handler(
    ApiJson(request): ApiJson<MatchRequest>,
) -> AppResult<Json<Match>> {
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    let slot = SLOTS
        .get_or_init(|| Arc::new(Semaphore::new(2)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::too_many_requests("слишком много расчётов Color Match"))?;
    let result = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut checkpoint = || {
            anyhow::ensure!(
                Instant::now() < deadline,
                "Color Match time budget exceeded"
            );
            Ok(())
        };
        let frame = |input: MatchFrame, checkpoint: &mut dyn FnMut() -> anyhow::Result<()>| {
            LinearColorFrame::from_surface(
                &GroupSurface {
                    width: input.width,
                    height: input.height,
                    rgba: input.rgba,
                },
                checkpoint,
            )
        };
        let current = frame(request.current, &mut checkpoint)?;
        let reference = frame(request.reference, &mut checkpoint)?;
        solve_with_library(
            &current,
            &reference,
            &request.parameters,
            request.skin_protect,
            &request.luts,
            &mut checkpoint,
        )
    })
    .await
    .map_err(|e| AppError::internal("join native Color Match", e))?
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(result))
}
