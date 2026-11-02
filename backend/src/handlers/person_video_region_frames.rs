//! Private model-free region planes from persisted project-origin inventory.
use crate::{
    config::resource_classes::ResourceClass,
    error::{AppError, AppResult},
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap},
    response::Response,
};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
struct Cancel(CancellationToken);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
pub async fn frame(
    State(state): State<AppState>,
    Path((id, region_id, ordinal)): Path<(String, String, u64)>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    super::person_video_jobs::authorize_job(&state, &id, &headers, true).await?;
    let origin = super::person_video_jobs::project_origin(&state, &id)
        .await?
        .ok_or_else(|| AppError::conflict("Нужны сохранённые области проекта"))?;
    let region = origin
        .snapshot
        .tracked_regions
        .iter()
        .find(|r| r.mask_source_id == region_id)
        .cloned()
        .ok_or_else(|| AppError::not_found("Область не найдена"))?;
    if ordinal >= region.frames {
        return Err(AppError::bad_request("Кадр вне области видео"));
    }
    let cancel = state.shutdown_token();
    let _cancel = Cancel(cancel.clone());
    let st = state.clone();
    let plane=state.spawn_task(async move{
        let deadline=tokio::time::Instant::now()+Duration::from_secs(15);
        let _permit=tokio::select!{_=cancel.cancelled()=>return Err(AppError::service_unavailable("Просмотр области остановлен")),permit=tokio::time::timeout_at(deadline,st.acquire_job_slot(ResourceClass::Analysis))=>permit.map_err(|_|AppError::service_unavailable("Просмотр области ожидает обработчик"))?.map_err(|_|AppError::service_unavailable("Просмотр области останавливается"))?};
        let (metadata,path)=tokio::select!{_=cancel.cancelled()=>return Err(AppError::service_unavailable("Просмотр области остановлен")),result=tokio::time::timeout_at(deadline,async{
            let (movie,_)=super::person_video_jobs::checked_result(&st,&id,&headers).await?;
            if movie.source_hash!=region.source_hash || (movie.width,movie.height,movie.frames,movie.frame_rate)!=(region.width,region.height,region.frames,region.frame_rate){return Err(AppError::conflict("Исходник области изменился"));}
            let path=super::person_video_jobs::source(&st,&region.mask_source_id,&actor).await.map_err(|_|AppError::forbidden("Область недоступна"))?;let size=tokio::fs::metadata(&path).await.map_err(|_|AppError::conflict("Область недоступна"))?.len();
            Ok::<_,AppError>((crate::services::local_person_mask_video::Metadata{width:256,height:256,frames:region.frames,frame_rate:region.frame_rate,mask_hash:region.mask_hash.clone(),file_hash:region.mask_file_hash.clone(),size_bytes:size},path))
        })=>result.map_err(|_|AppError::service_unavailable("Проверка области превысила время ожидания"))??};
        let codecs=tokio::select!{_=cancel.cancelled()=>return Err(AppError::service_unavailable("Просмотр области остановлен")),result=tokio::time::timeout_at(deadline,crate::services::local_enhance_video::resolve_tools())=>result.map_err(|_|AppError::service_unavailable("Видеокодеки не ответили"))?.map_err(|_|AppError::service_unavailable("Видеокодеки недоступны"))?};
        let plane=crate::services::local_region_frame::png(&st.process_runtime,crate::services::local_region_frame::Tools{ffmpeg:&codecs.ffmpeg,ffprobe:&codecs.ffprobe},&path,&metadata,ordinal,&cancel,deadline.saturating_duration_since(tokio::time::Instant::now())).await.map_err(|_|AppError::conflict("Не удалось прочитать неизменённый кадр области"))?;
        tokio::select!{_=cancel.cancelled()=>return Err(AppError::service_unavailable("Просмотр области остановлен")),result=tokio::time::timeout_at(deadline,async{
            super::person_video_jobs::checked_result(&st,&id,&headers).await?;
            let current=super::person_video_jobs::project_origin(&st,&id).await?.ok_or_else(||AppError::conflict("Область изменилась"))?;
            if current.snapshot.tracked_regions.iter().find(|r|r.mask_source_id==region_id)!=Some(&region){return Err(AppError::conflict("Область изменилась"));}Ok::<_,AppError>(())
        })=>result.map_err(|_|AppError::service_unavailable("Проверка области превысила время ожидания"))??};
        if cancel.is_cancelled(){return Err(AppError::service_unavailable("Просмотр области остановлен"));}Ok::<_,AppError>(plane)
    }).await.map_err(|_|AppError::service_unavailable("Просмотр области остановлен"))??;
    Response::builder()
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CACHE_CONTROL, "no-store")
        .header("x-mask-sha256", plane.mask_hash)
        .header("x-source-frame", ordinal.to_string())
        .body(Body::from(plane.png))
        .map_err(|e| AppError::internal("region frame response", e))
}
