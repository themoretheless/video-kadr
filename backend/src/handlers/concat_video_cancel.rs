//! Explicit cancellation remains effective when an HTTP proxy keeps work alive.
use axum::{extract::{Path,State},http::{HeaderMap,StatusCode}};use serde::Deserialize;
use crate::{state::AppState,error::{ApiJson,AppError,AppResult}};
#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]pub struct Request{operation_id:String}
pub async fn handler(State(state):State<AppState>,Path(project):Path<String>,headers:HeaderMap,ApiJson(request):ApiJson<Request>)->AppResult<StatusCode>{let actor=super::publish::authenticated_actor(&state,&headers).await?;super::cutout_assets::authorize(&state,&project,&actor).await?;state.concat_video_cancellations.cancel(&actor,&project,&request.operation_id).map_err(|error|AppError::bad_request(error.to_string()))?;Ok(StatusCode::NO_CONTENT)}
