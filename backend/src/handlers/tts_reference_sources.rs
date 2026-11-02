//! Create a frozen private voice sample from authorized library media.
use crate::{
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    state::AppState,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceRequest {
    source_id: String,
    start_us: u64,
    end_us: u64,
}
struct Cancel(CancellationToken);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<SourceRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    super::library::authorize_source_access(&state, &request.source_id, &headers).await?;
    if !request
        .end_us
        .checked_sub(request.start_us)
        .is_some_and(|n| (500000..=10000000).contains(&n))
    {
        return Err(AppError::bad_request(
            "Выберите звуковой диапазон от 0,5 до 10 секунд",
        ));
    }
    if !state.tools.ffmpeg {
        return Err(AppError::service_unavailable(
            "Для извлечения образца нужен локальный FFmpeg",
        ));
    }
    let cancel = state.shutdown_token();
    let _guard = Cancel(cancel.clone());
    let st = state.clone();
    state.spawn_task(async move{
  let _permit=tokio::select!{_=cancel.cancelled()=>return Err(AppError::service_unavailable("Извлечение образца остановлено")),result=st.acquire_job_slot(ResourceClass::Analysis)=>result.map_err(|_|AppError::service_unavailable("Извлечение останавливается"))?};
  super::library::authorize_source_access(&st,&request.source_id,&headers).await?;
  let entry=st.library.get(&request.source_id).await.filter(|e|e.kind=="source").ok_or_else(||AppError::not_found("Источник не найден"))?;
  let path=st.library.resolve_media_path(&entry).await.map_err(|_|AppError::not_found("Исходный файл не найден"))?;
  let before=crate::project_archive::fingerprint_file(&path).await.map_err(|_|AppError::bad_request("Не удалось проверить исходник"))?;
  let probe=crate::tools::probe_video(&st.process_runtime,&path).await.map_err(|_|AppError::bad_request("Не удалось прочитать звук исходника"))?;
  if probe.acodec.is_none()||!probe.duration.is_finite()||probe.duration<=0.{return Err(AppError::bad_request("Диапазон должен находиться внутри звукового исходника"));}
  let end_us=request.end_us.min((probe.duration*1_000_000.).floor() as u64);
  if !end_us.checked_sub(request.start_us).is_some_and(|n|n>=500000){return Err(AppError::bad_request("После выбранной позиции осталось меньше 0,5 секунды звука"));}
  let staging=crate::services::tts_model_download::directory(&st.storage,std::path::Path::new("tts-reference-staging")).await.map_err(|e|AppError::internal("prepare reference decoding",e))?;
  let(wav,duration)=crate::services::local_voice_reference::extract(&st.process_runtime,&path,request.start_us,end_us,&staging,&cancel).await.map_err(|_|AppError::bad_request("Не удалось извлечь образец голоса"))?;
  if cancel.is_cancelled(){return Err(AppError::service_unavailable("Извлечение образца остановлено"));}
  super::library::authorize_source_access(&st,&request.source_id,&headers).await?;
  let after=crate::project_archive::fingerprint_file(&path).await.map_err(|_|AppError::bad_request("Исходник изменился"))?;
  if before!=after||st.library.get(&request.source_id).await.is_none(){return Err(AppError::bad_request("Исходник изменился во время извлечения"));}
  super::tts_references::persist(&st,&headers,&owner,wav,duration,Some(super::tts_references::ReferenceOrigin{source_id:request.source_id,source_hash:before.1,start_us:request.start_us,end_us}),false).await
 }).await.map_err(|_|AppError::service_unavailable("Извлечение образца остановлено"))?
}
