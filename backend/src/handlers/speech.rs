use super::library::authorize_source_access;
use crate::{
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    services::local_speech,
    state::AppState,
};
use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;
#[derive(Clone, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub(super) source_id: String,
    pub(super) start_us: u64,
    pub(super) end_us: u64,
    pub(super) language: String,
    #[serde(default)]
    pub(super) segmentation: crate::domain::speech_alignment::SpeechSegmentation,
}
pub async fn models(State(state): State<AppState>,headers:HeaderMap) -> AppResult<Json<serde_json::Value>> {
    let owner=super::speech_models::optional_owner(&state,&headers).await?;
    let(config,model_id)=super::speech_models::config_for_owner(&state,owner.as_deref()).await?;
    let available = match &config.model {
        Some(path) => tokio::fs::metadata(path)
            .await
            .map_or(false, |m| m.is_file() && m.len() >= 1_000_000),
        None => false,
    };
    let model:Option<String>=if !available{None}else if let(Some(owner),Some(id))=(owner.as_deref(),model_id.as_deref()){sqlx::query_scalar("SELECT name FROM speech_models WHERE owner=? AND id=?").bind(owner).bind(id).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("load selected model name",e))?}else{config.model.as_ref().and_then(|p|p.file_name()).map(|s|s.to_string_lossy().into_owned())};
    Ok(Json(
        serde_json::json!({"engine":"whisper.cpp","available":available,"model":model,"maxDurationUs":600000000}),
    ))
}
pub async fn transcribe(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Request>,
) -> AppResult<Json<local_speech::Transcript>> {
    authorize_source_access(&state, &request.source_id, &headers).await?;
    let model_operation=state.speech_model_ops.lock().await;
    let owner=super::speech_models::optional_owner(&state,&headers).await?;
    let(config,model_id)=super::speech_models::config_for_owner(&state,owner.as_deref()).await?;
    let _model_use=super::speech_models::ModelUse::new(&state,model_id,true);
    drop(model_operation);
    if config.model.is_none() {
        return Err(AppError::service_unavailable(
            "Локальная модель Whisper не настроена",
        ));
    }
    if !state.tools.ffmpeg {
        return Err(AppError::service_unavailable(
            "Для распознавания нужен локальный FFmpeg",
        ));
    }
    if request.start_us >= request.end_us || request.end_us - request.start_us > 600_000_000 {
        return Err(AppError::bad_request("Выберите диапазон речи до 10 минут"));
    }
    let entry = state
        .library
        .get(&request.source_id)
        .await
        .filter(|entry| entry.kind == "source")
        .ok_or_else(|| AppError::not_found("Источник не найден"))?;
    let path = state
        .library
        .resolve_media_path(&entry)
        .await
        .map_err(|_| AppError::not_found("Исходный файл не найден"))?;
    let _permit = state
        .acquire_job_slot(ResourceClass::Analysis)
        .await
        .map_err(|_| AppError::service_unavailable("Распознавание останавливается"))?;
    let probe = crate::tools::probe_video(&state.process_runtime, &path)
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    if probe.acodec.is_none()
        || !probe.duration.is_finite()
        || probe.duration <= 0.0
        || request.end_us as f64 > (probe.duration * 1_000_000.0).ceil()
    {
        return Err(AppError::bad_request(
            "Нужен звуковой диапазон внутри длительности источника",
        ));
    }
    let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
    let cancel = tokio_util::sync::CancellationToken::new();
    let transcript = local_speech::transcribe_segmented(
        &state.process_runtime,
        &config,
        &path,
        request.start_us,
        request.end_us,
        &state.storage.join("speech-staging"),
        &request.language,
        &cancel,
        request.segmentation,
        &progress,
    )
    .await
    .map_err(|error| AppError::bad_request(error.to_string()))?;
    authorize_source_access(&state, &request.source_id, &headers).await?;
    Ok(Json(transcript))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClipRequest {
    project: crate::domain::nested_composition::NestedCompositionProject,
    composition_id: String,
    clip_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AlignmentRequest {
    project: crate::domain::nested_composition::NestedCompositionProject,
    composition_id: String,
    clip_id: String,
    source_id: String,
    transcript: local_speech::Transcript,
}
pub async fn clip_plan(
    ApiJson(request): ApiJson<ClipRequest>,
) -> AppResult<Json<crate::domain::speech_alignment::SpeechClipPlan>> {
    let permit = SPEECH_ALIGN_SLOTS
        .acquire()
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let value = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        crate::domain::speech_alignment::plan(
            &request.project,
            &request.composition_id,
            &request.clip_id,
        )
    })
    .await
    .map_err(|e| AppError::internal("speech clip planning", e))?
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(value))
}
pub async fn reference_clip_plan(ApiJson(request):ApiJson<ClipRequest>)->AppResult<Json<crate::domain::speech_alignment::VoiceReferencePlan>> {
 let permit=SPEECH_ALIGN_SLOTS.acquire().await.map_err(|e|AppError::bad_request(e.to_string()))?;
 let value=tokio::task::spawn_blocking(move||{let _permit=permit;crate::domain::speech_alignment::reference_plan(&request.project,&request.composition_id,&request.clip_id)}).await.map_err(|e|AppError::internal("voice reference clip planning",e))?.map_err(|e|AppError::bad_request(e.to_string()))?;Ok(Json(value))
}
pub async fn align(
    ApiJson(request): ApiJson<AlignmentRequest>,
) -> AppResult<Json<crate::domain::caption_timeline::CaptionTrack>> {
    let permit = SPEECH_ALIGN_SLOTS
        .acquire()
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let value = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        crate::domain::speech_alignment::align(
            &request.project,
            &request.composition_id,
            &request.clip_id,
            &request.source_id,
            &request.transcript,
        )
    })
    .await
    .map_err(|e| AppError::internal("speech alignment", e))?
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(value))
}

static SPEECH_ALIGN_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
