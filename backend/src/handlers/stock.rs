use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::stock_catalog::{StockKind, StockOrientation, StockSearchResult};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StockSearchQuery {
    pub q: String,
    pub kind: StockKind,
    #[serde(default)]
    pub orientation: Option<StockOrientation>,
    #[serde(default = "default_page")]
    pub page: u32,
}

fn default_page() -> u32 {
    1
}

pub async fn stock_search_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<StockSearchQuery>,
) -> AppResult<Json<StockSearchResult>> {
    let token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::unauthorized("Требуется сессия для stock catalog"))?;
    state
        .db
        .resolve_auth_session(token)
        .await
        .map_err(|error| AppError::internal("authenticate stock request", error))?
        .ok_or_else(|| AppError::unauthorized("Сессия недействительна или истекла"))?;
    let catalog = state.stock_catalog.as_ref().ok_or_else(|| {
        AppError::service_unavailable("Stock catalog не настроен: задайте PEXELS_API_KEY")
    })?;
    catalog
        .search(&query.q, query.kind, query.orientation, query.page)
        .await
        .map(Json)
        .map_err(|error| {
            let message = error.to_string();
            if message.contains("invalid stock") {
                AppError::bad_request(message)
            } else {
                AppError::service_unavailable("Pexels временно недоступен")
            }
        })
}
