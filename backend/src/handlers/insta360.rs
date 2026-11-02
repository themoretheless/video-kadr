use crate::{
    domain::insta360_metadata::{self, Metadata},
    error::{AppError, AppResult},
    state::AppState,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
pub async fn metadata(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<Option<Metadata>>> {
    if !crate::db::valid_composition_source_id(&id) {
        return Err(AppError::bad_request("invalid source id"));
    }
    super::library::authorize_source_access(&state, &id, &headers).await?;
    let entry = state
        .library
        .get(&id)
        .await
        .filter(|e| e.kind == "source")
        .ok_or_else(|| AppError::not_found("Исходник не найден"))?;
    let path = state
        .library
        .resolve_media_path(&entry)
        .await
        .map_err(|e| AppError::internal("resolve camera source", e))?;
    let result = tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(path)?;
        insta360_metadata::inspect(&mut file)
    })
    .await
    .map_err(|e| AppError::internal("inspect camera source", e))?
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(result))
}

/// Read a bounded capture-clock gyro window. Unknown camera axes are returned as
/// a diagnostic, never silently treated as renderer coordinates.
pub async fn gyro(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<crate::domain::insta360_gyro::GyroRequest>,
) -> AppResult<Json<Option<crate::domain::insta360_gyro::GyroAnalysis>>> {
    request.validate().map_err(|e|AppError::bad_request(e.to_string()))?;
    if !crate::db::valid_composition_source_id(&id) {
        return Err(AppError::bad_request("invalid source id"));
    }
    super::library::authorize_source_access(&state, &id, &headers).await?;
    let entry = state.library.get(&id).await.filter(|e| e.kind == "source")
        .ok_or_else(|| AppError::not_found("Исходник не найден"))?;
    let path = state.library.resolve_media_path(&entry).await
        .map_err(|e| AppError::internal("resolve camera source", e))?;
    let cancel=tokio_util::sync::CancellationToken::new();
    let _cancel=cancel.clone().drop_guard();
    let permit=super::concat_cutout_analyse::acquire_analysis(&state,&cancel).await
        .map_err(|e|AppError::bad_request(e.to_string()))?;
    let worker_cancel=cancel.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit=permit;
        crate::services::insta360_visual_drift::analyze_path(&path, &request, &worker_cancel)
    }).await.map_err(|e| AppError::internal("read camera gyro", e))?
      .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(result))
}

#[derive(serde::Serialize)]
#[serde(rename_all="camelCase")]
pub struct ProfileResponse {
    calibration: crate::domain::insta360_camera_profile::CameraProfileReport,
    source_layout: Option<crate::domain::spherical_video::SourceLayout>,
    packing_reason: String,
}
pub async fn profile(
    State(state): State<AppState>, Path(id): Path<String>, headers: HeaderMap,
) -> AppResult<Json<Option<ProfileResponse>>> {
    if !crate::db::valid_composition_source_id(&id) { return Err(AppError::bad_request("invalid source id")); }
    super::library::authorize_source_access(&state,&id,&headers).await?;
    let entry=state.library.get(&id).await.filter(|e|e.kind=="source").ok_or_else(||AppError::not_found("Исходник не найден"))?;
    let path=state.library.resolve_media_path(&entry).await.map_err(|e|AppError::internal("resolve camera profile",e))?;
    let local=path.clone();
    let metadata=tokio::task::spawn_blocking(move||{let mut file=std::fs::File::open(local)?;crate::domain::insta360_metadata::inspect(&mut file)})
        .await.map_err(|e|AppError::internal("read camera calibration",e))?.map_err(|e|AppError::bad_request(e.to_string()))?;
    let Some(metadata)=metadata else{return Ok(Json(None))};
    let calibration=crate::domain::insta360_camera_profile::inspect(&metadata);
    let probe=crate::tools::probe_video(&state.process_runtime,&path).await.map_err(|e|AppError::bad_request(e.to_string()))?;
    let streams:Vec<_>=probe.streams.iter().filter(|s|s.kind==crate::domain::media_probe::StreamKind::Video&&!s.dispositions.contains("attached_pic")).collect();
    let (source_layout,packing_reason)=match streams.as_slice(){
        [a,b] if a.width==b.width&&a.height==b.height&&a.width.is_some()&&a.time_base.is_some()&&a.frame_rate.is_some()&&a.time_base==b.time_base&&a.frame_rate==b.frame_rate&&a.start_time_seconds==b.start_time_seconds&&a.duration_seconds==b.duration_seconds&&a.frames==b.frames =>
            (Some(crate::domain::spherical_video::SourceLayout::DualVideoStreams),"Two lens streams with matching declared clocks and dimensions".into()),
        [_] => (None,"Single video track: packed lens layout has not been established".into()),
        _ => (None,"Lens track layout or clocks are unsupported".into()),
    };
    Ok(Json(Some(ProfileResponse{calibration,source_layout,packing_reason})))
}
