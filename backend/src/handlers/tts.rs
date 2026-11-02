//! Authenticated local narration. WAV bytes are returned only to this request;
//! no narration text or generated audio is registered under public file routes.
use crate::{
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    services::local_tts,
    state::AppState,
};
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap},
    response::Response,
    Json,
};
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;
struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
struct OwnedDirectory(PathBuf);
impl Drop for OwnedDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub async fn voices(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<serde_json::Value>> {
    let actor=if headers.contains_key(header::AUTHORIZATION){Some(super::publish::authenticated_actor(&state,&headers).await?)}else{None};
    let config=if let Some(owner)=&actor{super::tts_models::config_for_owner(&state,owner).await?.0}else{state.tts.as_ref().clone()};
    let family=local_tts::family(&config).map_err(|_|AppError::service_unavailable("Некорректный комплект модели"))?;
    let available=config.model_dir.as_ref().is_some_and(|p|match family{local_tts::TtsFamily::Kokoro=>p.join("model.int8.onnx").is_file()||p.join("model.onnx").is_file(),local_tts::TtsFamily::Pocket=>p.join("lm_main.int8.onnx").is_file(),local_tts::TtsFamily::Chatterbox=>p.join("onnx/language_model_quantized.onnx_data").is_file()&&state.chatterbox.is_some()});
    let mut voices=local_tts::model_voices(&config).map_err(|_|AppError::service_unavailable("Голоса модели недоступны"))?;
    if family==local_tts::TtsFamily::Chatterbox{let stock=if let Some(owner)=&actor{super::tts_models::chatterbox_stock_available(&state,owner).await?}else{false};if !stock{voices.retain(|v|v.id!=2000);}}
    Ok(Json(serde_json::json!({"engine":local_tts::engine_name(family),"available":available,"voices":voices,"maxTextBytes":4096,"supportsPauses":family==local_tts::TtsFamily::Kokoro&&state.kokoro.is_some(),"minSpeed":if family==local_tts::TtsFamily::Chatterbox{1.}else{0.5},"maxSpeed":if family==local_tts::TtsFamily::Chatterbox{1.}else{2.}})))
}
pub async fn synthesize(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(submission): ApiJson<super::tts_references::Submission>,
) -> AppResult<Response> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let model_operation = state.tts_model_ops.clone().lock_owned().await;
    let pinned=submission.pin(&state,&owner,&headers).await?;
    let (config, _) = super::tts_models::config_for_owner(&state, &owner).await?;
    super::tts_models::validate_reference(&config,&pinned)?;
    let request=pinned.request;
    let reference_snapshot=pinned.reference;
    request.validate().map_err(|_| {
        AppError::bad_request("Проверьте текст (до 4096 байт), голос и скорость 0,5–2×")
    })?;
    if config.model_dir.is_none() {
        return Err(AppError::service_unavailable(
            "Локальная модель озвучивания не настроена",
        ));
    }
    request.validate_for_model(&config).map_err(|_|AppError::bad_request("Проверьте голос и поддерживаемую скорость выбранной модели"))?;
    let cancel = state.shutdown_token();
    let _request_guard = CancelOnDrop(cancel.clone());
    let st = state.clone();
    // An HTTP disconnect cancels a tracked task; the process runner, rather than
    // dropping its future, owns termination/reaping before staging is removed.
    let task=state.spawn_task(async move{
  let _model_operation=model_operation;
  let _permit=tokio::select!{_ = cancel.cancelled()=>anyhow::bail!("narration cancelled before admission"),permit=st.acquire_job_slot(ResourceClass::Analysis)=>permit?};
  let base=st.storage.join("tts-staging");tokio::fs::create_dir_all(&base).await?;
  let directory=OwnedDirectory(base.join(uuid::Uuid::new_v4().to_string()));tokio::fs::create_dir(&directory.0).await?;
  #[cfg(unix)] {use std::os::unix::fs::PermissionsExt;tokio::fs::set_permissions(&directory.0,std::fs::Permissions::from_mode(0o700)).await?;}
  let output=directory.0.join("narration.wav");
  let reference=if let Some(snapshot)=&reference_snapshot{Some(snapshot.resolve(&st,&owner).await.map_err(|e|anyhow::anyhow!("reference unavailable: {e:?}"))?)}else{None};
  let audio=super::tts_models::synthesize(&st,&config,&request,&output,&cancel,Duration::from_secs(300),reference.as_deref(),None).await?;
  if let Some(snapshot)=&reference_snapshot{snapshot.resolve(&st,&owner).await.map_err(|_|anyhow::anyhow!("reference changed"))?;}
  anyhow::ensure!(!cancel.is_cancelled(),"narration cancelled before response");
  let bytes=tokio::fs::read(&output).await?;
  Ok::<_,anyhow::Error>((audio,bytes))
 });
    let (audio, bytes) = task
        .await
        .map_err(|_| AppError::service_unavailable("Озвучивание остановлено"))?
        .map_err(|_| {
            AppError::service_unavailable(
                "Не удалось создать речь локальной моделью. Проверьте доступность модели и движка.",
            )
        })?;
    Response::builder()
        .header(header::CONTENT_TYPE, "audio/wav")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"narration.wav\"",
        )
        .header(header::CACHE_CONTROL, "no-store")
        .header("x-tts-sample-rate", audio.sample_rate.to_string())
        .header("x-tts-model-sha256", audio.model_hash)
        .body(Body::from(bytes))
        .map_err(|e| AppError::internal("build narration response", e))
}
