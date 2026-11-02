//! Revision-bound video freeze command; all source/media paths are server-owned.
use crate::{
    error::{ApiJson, AppError, AppResult},
    services::{
        concat_freeze_capture::Tools,
        concat_freeze_publish::{self as freeze, Target},
    },
    state::AppState,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub revision: u64,
    pub composition_id: Option<String>,
    pub clip_id: String,
    pub time: f64,
}
async fn editor(state: &AppState, project: &str, actor: &str) -> AppResult<()> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM composition_project_members WHERE project_id=? AND actor=?",
    )
    .bind(project)
    .bind(actor)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| AppError::internal("authorize freeze", e))?;
    if !matches!(role.as_deref(), Some("owner" | "editor")) {
        return Err(AppError::forbidden(
            "Для стоп-кадра нужен доступ редактора к проекту",
        ));
    }
    Ok(())
}
async fn failure(
    state: &AppState,
    project: &str,
    actor: &str,
    revision: u64,
    error: anyhow::Error,
) -> AppError {
    if let Err(error) = editor(state, project, actor).await {
        return error;
    }
    match state.db.get_composition_project_for(project, actor).await {
        Ok(Some(saved)) if saved.revision != revision => {
            AppError::conflict("Проект изменился. Обновите его перед вставкой стоп-кадра")
        }
        Err(error) => AppError::internal("reload freeze project", error),
        _ => {
            tracing::warn!(error=%error,"freeze command refused");
            AppError::bad_request(
                "Не удалось вставить стоп-кадр без потери настроек исходного клипа",
            )
        }
    }
}
pub async fn store(
    State(state): State<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Request>,
) -> AppResult<Json<Value>> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    editor(&state, &project, &actor).await?;
    if request.revision > 9_007_199_254_740_991
        || !request.time.is_finite()
        || !(0. ..=360_000.).contains(&request.time)
        || crate::domain::composition::CompositionClipId::parse(request.clip_id.clone()).is_err()
        || request
            .composition_id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 128)
    {
        return Err(AppError::bad_request("Некорректное положение стоп-кадра"));
    }
    let saved = state
        .db
        .get_composition_project_for(&project, &actor)
        .await
        .map_err(|e| AppError::internal("load freeze project", e))?
        .ok_or_else(|| AppError::not_found("Проект не найден"))?;
    if saved.revision != request.revision {
        return Err(AppError::conflict(
            "Проект изменился. Обновите его перед вставкой стоп-кадра",
        ));
    }
    if saved.document.get("compositions").is_some() != request.composition_id.is_some() {
        return Err(AppError::bad_request(
            "Укажите композицию только для вложенного проекта",
        ));
    }
    let _slot = state
        .try_acquire_upload_slot()
        .ok_or_else(|| AppError::too_many_requests("Повторите вставку стоп-кадра позже"))?;
    let codecs = crate::services::local_enhance_video::resolve_tools()
        .await
        .map_err(|e| AppError::internal("resolve freeze codecs", e))?;
    let cancel = state.shutdown_token().child_token();
    let _guard = cancel.clone().drop_guard();
    let prepared = freeze::prepare(
        &state,
        &actor,
        Target {
            project: project.clone(),
            revision: request.revision,
            composition: request.composition_id,
            clip: request.clip_id,
            time: request.time,
        },
        Tools {
            ffmpeg: &codecs.ffmpeg,
            ffprobe: &codecs.ffprobe,
        },
        Duration::from_secs(120),
        &cancel,
    )
    .await;
    let prepared = match prepared {
        Ok(Some(prepared)) => prepared,
        Ok(None) => return Ok(Json(json!({"changed":false}))),
        Err(error) => return Err(failure(&state, &project, &actor, request.revision, error).await),
    };
    let result = freeze::publish_native(&state, prepared, &cancel).await;
    match result {
        Ok(result) => Ok(Json(
            json!({"changed":true,"project":result.project,"media":result.media}),
        )),
        Err(error) => Err(failure(&state, &project, &actor, request.revision, error).await),
    }
}
