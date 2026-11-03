//! Explicit project-owned copies of temporal SAM regions and their original movie.
use super::person_video_project_assets::{resource_for, ResourceOwner};
use crate::{
    domain::video_region::Attachment,
    error::{ApiJson, AppError, AppResult},
    library::MediaEntry,
    state::AppState,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    pub(super) project_id: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Accepted {
    pub source: MediaEntry,
    pub mask: MediaEntry,
    pub snapshot: Attachment,
}
pub async fn store(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    ApiJson(target): ApiJson<Target>,
) -> AppResult<Json<Accepted>> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    super::person_video_assets::authorize(&state, &target.project_id, &actor).await?;
    let _operation = state.enhance_asset_ops.lock().await;
    let (movie, path) = super::sam_jobs::checked_video(&state, &id, &headers).await?;
    let recipe = super::sam_jobs::points(State(state.clone()), Path(id.clone()), headers.clone())
        .await?
        .0;
    let source_id = recipe["sourceId"]
        .as_str()
        .ok_or_else(|| AppError::conflict("Нет исходника видеорегиона"))?;
    let points = serde_json::from_value(recipe["points"].clone())
        .map_err(|_| AppError::conflict("Неверные точки видеорегиона"))?;
    if recipe["seedFrame"].as_u64() != Some(movie.seed_frame) {
        return Err(AppError::conflict("Кадр видеорегиона изменился"));
    }
    let mut source = state
        .library
        .get(source_id)
        .await
        .filter(|e| e.kind == "source" && e.media_type.as_deref() == Some("video"))
        .ok_or_else(|| AppError::conflict("Исходное видео недоступно"))?;
    let original = state
        .library
        .resolve_media_path(&source)
        .await
        .map_err(|_| AppError::conflict("Исходное видео недоступно"))?;
    let duration = movie.frame_times_us.as_ref().and_then(|t|t.last()).map(|t|*t as f64/1_000_000.).unwrap_or(movie.frames as f64 * f64::from(movie.frame_rate.denominator) / f64::from(movie.frame_rate.numerator));
    source.width.get_or_insert(movie.width);
    source.height.get_or_insert(movie.height);
    source.duration.get_or_insert(duration);
    source.fps.get_or_insert(
        f64::from(movie.frame_rate.numerator) / f64::from(movie.frame_rate.denominator),
    );
    let source = resource_for(
        ResourceOwner::Sam,
        &state,
        &id,
        &target.project_id,
        &actor,
        &headers,
        &original,
        &movie.source_hash,
        "sam-video-original",
        source,
    )
    .await?;
    let entry = MediaEntry::from_result(
        "source",
        &json!({"id":"pending","filename":"regions.mkv","url":"/pending","mediaType":"video","width":256,"height":256,"duration":duration,"fps":f64::from(movie.frame_rate.numerator)/f64::from(movie.frame_rate.denominator),"vcodec":"ffv1","title":"Видеорегион SAM"}),
    );
    let mask = resource_for(
        ResourceOwner::Sam,
        &state,
        &id,
        &target.project_id,
        &actor,
        &headers,
        &path,
        &movie.region_video.file_hash,
        "sam-video-regions",
        entry,
    )
    .await?;
    if mask.kind != "source"
        || mask.media_type.as_deref() != Some("video")
        || mask.width != Some(256)
        || mask.height != Some(256)
        || mask.acodec.is_some()
    {
        return Err(AppError::conflict("Неверный сохранённый видеорегион"));
    }
    let snapshot = Attachment {
        source_id: source.id.clone(),
        mask_source_id: mask.id.clone(),
        source_hash: movie.source_hash,
        mask_hash: movie.tracking.output_hash,
        mask_file_hash: movie.region_video.file_hash,
        encoder_hash: movie.tracking.encoder_hash,
        decoder_hash: movie.tracking.decoder_hash,
        runtime_hash: movie.tracking.runtime_hash,
        prompt_hash: movie.seed_region.prompt_hash,
        points,
        width: movie.width,
        height: movie.height,
        frames: movie.frames,
        frame_rate: movie.frame_rate,
        frame_times_us: movie.frame_times_us,
        seed_frame: movie.seed_frame,
        tracked_frames: movie.tracking.tracked_frames,
        forward_lost_at: movie.tracking.forward_lost_at,
        backward_lost_at: movie.tracking.backward_lost_at,
    };
    snapshot
        .validate()
        .map_err(|_| AppError::conflict("Неверный рецепт видеорегиона"))?;
    super::person_video_assets::authorize(&state, &target.project_id, &actor).await?;
    super::sam_jobs::checked_video(&state, &id, &headers).await?;
    Ok(Json(Accepted {
        source,
        mask,
        snapshot,
    }))
}

/// Persist only the region movie when the project already owns the original.
/// Avoid copying the full original video once for every smart stroke.
pub(super) async fn mask_for_original(state:&AppState,id:&str,project:&str,actor:&str,headers:&HeaderMap,source_id:&str,revision:u64)->AppResult<(MediaEntry,Attachment)>{
 super::person_video_assets::authorize(state,project,actor).await?;
 let saved=state.db.get_composition_project_for(project,actor).await.map_err(|e|AppError::internal("read SAM source binding",e))?.ok_or_else(||AppError::conflict("Проект недоступен"))?;
 if saved.revision!=revision{return Err(AppError::conflict("Проект изменился"))}
 let physical=super::concat_cutout_access::library_id(&saved.document,source_id).map_err(|e|AppError::conflict(e.to_string()))?;
 if !saved.source_ids.contains(&physical){return Err(AppError::forbidden("Исходник не принадлежит проекту"))}
 let _operation=state.enhance_asset_ops.lock().await;
 let(movie,path)=super::sam_jobs::checked_video(state,id,headers).await?;
 let recipe=super::sam_jobs::points(State(state.clone()),Path(id.to_owned()),headers.clone()).await?.0;
 if recipe["sourceId"].as_str()!=Some(physical.as_str())||recipe["seedFrame"].as_u64()!=Some(movie.seed_frame){return Err(AppError::conflict("Задача SAM относится к другому исходнику или кадру"))}
 let points=serde_json::from_value(recipe["points"].clone()).map_err(|_|AppError::conflict("Неверные точки SAM"))?;
 let original=super::concat_cutout_access::source_for_document(state,actor,&saved.document,source_id).await.map_err(|_|AppError::forbidden("Исходник SAM недоступен"))?;
 let before=crate::project_archive::fingerprint_file_with_limit(&original,64*1024*1024*1024).await.map_err(|_|AppError::conflict("Исходник SAM недоступен"))?;
 if before.1!=movie.source_hash{return Err(AppError::conflict("Исходник SAM изменился"))}
 let duration=movie.frames as f64*f64::from(movie.frame_rate.denominator)/f64::from(movie.frame_rate.numerator);
 let entry=MediaEntry::from_result("source",&json!({"id":"pending","filename":"regions.mkv","url":"/pending","mediaType":"video","width":256,"height":256,"duration":duration,"fps":f64::from(movie.frame_rate.numerator)/f64::from(movie.frame_rate.denominator),"vcodec":"ffv1","title":"Видеорегион SAM"}));
 let mask=resource_for(ResourceOwner::Sam,state,id,project,actor,headers,&path,&movie.region_video.file_hash,"sam-video-regions",entry).await?;
 if mask.kind!="source"||mask.media_type.as_deref()!=Some("video")||mask.width!=Some(256)||mask.height!=Some(256)||mask.acodec.is_some(){return Err(AppError::conflict("Неверный ресурс SAM"))}
 let snapshot=Attachment{source_id:source_id.to_owned(),mask_source_id:mask.id.clone(),source_hash:movie.source_hash,mask_hash:movie.tracking.output_hash,mask_file_hash:movie.region_video.file_hash,encoder_hash:movie.tracking.encoder_hash,decoder_hash:movie.tracking.decoder_hash,runtime_hash:movie.tracking.runtime_hash,prompt_hash:movie.seed_region.prompt_hash,points,width:movie.width,height:movie.height,frames:movie.frames,frame_rate:movie.frame_rate,frame_times_us:movie.frame_times_us,seed_frame:movie.seed_frame,tracked_frames:movie.tracking.tracked_frames,forward_lost_at:movie.tracking.forward_lost_at,backward_lost_at:movie.tracking.backward_lost_at};
 snapshot.validate().map_err(|e|AppError::conflict(e.to_string()))?;
 super::person_video_assets::authorize(state,project,actor).await?;super::sam_jobs::checked_video(state,id,headers).await?;
 if crate::project_archive::fingerprint_file_with_limit(&original,64*1024*1024*1024).await.map_err(|e|AppError::conflict(e.to_string()))?!=before{return Err(AppError::conflict("Исходник SAM изменился"))}
 let current=state.db.get_composition_project_for(project,actor).await.map_err(|e|AppError::internal("recheck SAM source binding",e))?.ok_or_else(||AppError::conflict("Проект недоступен"))?;
 if current.revision!=revision||current.document!=saved.document||super::concat_cutout_access::source_for_document(state,actor,&current.document,source_id).await.map_err(|e|AppError::conflict(e.to_string()))?!=original{return Err(AppError::conflict("Исходник или проект изменился"))}
 Ok((mask,snapshot))
}
