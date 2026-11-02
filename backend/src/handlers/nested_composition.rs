//! Bounded native nested-frame planning for authored project documents.
use crate::domain::nested_composition::{NestedCompositionProject, NestedFrameEntry};
use crate::error::{ApiJson, AppError, AppResult};
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
static NESTED_SLOTS: Semaphore = Semaphore::const_new(2);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NestedFrameRequest {
    pub project: NestedCompositionProject,
    pub time_us: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NestedFrameResponse {
    pub entries: Vec<NestedFrameEntry>,
}

pub async fn nested_frame_handler(
    ApiJson(request): ApiJson<NestedFrameRequest>,
) -> AppResult<Json<NestedFrameResponse>> {
    let permit = NESTED_SLOTS
        .acquire()
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let entries = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        request.project.frame_plan(request.time_us)
    })
    .await
    .map_err(|error| AppError::bad_request(error.to_string()))?
    .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(NestedFrameResponse { entries }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::composition::{CanvasSpec, Composition};
    use crate::domain::nested_composition::NestedCompositionDocument;
    use std::collections::BTreeMap;
    #[tokio::test]
    async fn handler_returns_native_frame_wire_and_refuses_missing_root() {
        let project = NestedCompositionProject {
            project_media: Default::default(),
            fonts:vec![],
            timeline_order: vec![],
            luts:vec![],
            schema_version: 1,
            root: "root".into(),
            compositions: BTreeMap::from([(
                "root".into(),
                NestedCompositionDocument {
                    duration_us: None,
                    content: Composition::new(CanvasSpec::default()),
                    instances: vec![],
                },
            )]),
        };
        let Json(result) = nested_frame_handler(ApiJson(NestedFrameRequest {
            project: project.clone(),
            time_us: 123456,
        }))
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::json!({ "entries": [{ "composition": "root", "sourceTimeUs": 123456, "instancePath": [], "ancestors": [], "ancestorPoses": [], "canvasToRoot": [1.0,0.0,0.0,1.0,0.0,0.0] }] })
        );
        let mut invalid = project;
        invalid.root = "missing".into();
        assert!(nested_frame_handler(ApiJson(NestedFrameRequest {
            project: invalid,
            time_us: 0
        }))
        .await
        .is_err());
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NestedGroupRequest {
    project: NestedCompositionProject,
    parent: String,
    child: String,
    instance_id: String,
}
pub async fn nested_group_handler(
    ApiJson(request): ApiJson<NestedGroupRequest>,
) -> AppResult<Json<NestedCompositionProject>> {
    let permit = NESTED_SLOTS
        .acquire()
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let project = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        request
            .project
            .group_document(&request.parent, &request.child, &request.instance_id)
    })
    .await
    .map_err(|error| AppError::bad_request(error.to_string()))?
    .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(project))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NestedPreviewRequest {
    #[serde(default)]
    saved_project: Option<super::concat_video_bound::SavedProject>,
    project: NestedCompositionProject,
    time_us: u64,
    rendered_outputs: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    composition_id: Option<String>,
}

/// Raw RGBA preview of authorized, previously rendered local compositions.
pub async fn nested_preview_handler(
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
    headers: axum::http::HeaderMap,
    ApiJson(mut request): ApiJson<NestedPreviewRequest>,
) -> AppResult<axum::response::Response> {
    use axum::response::IntoResponse;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let png=headers.get(axum::http::header::ACCEPT).and_then(|value|value.to_str().ok())==Some("image/png");
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    if request.project.compositions.values().any(|node|node.content.tracks.iter().any(|track|matches!(track,crate::domain::composition::CompositionTrack::Video{clips,..} if clips.iter().any(|clip|clip.enabled&&clip.concat_cutout.is_some())))){request.rendered_outputs.clear();}

    request
        .project
        .dependency_order()
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let mut images = std::collections::BTreeMap::new();
    let image_ids = request
        .project
        .frame_image_sources(
            request
                .composition_id
                .as_deref()
                .unwrap_or(&request.project.root),
            request.time_us,
            &request.rendered_outputs.keys().cloned().collect(),
        )
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    for id in image_ids {
        super::library::authorize_source_access(&state, id.as_str(), &headers).await?;
        let entry = state
            .library
            .get(id.as_str())
            .await
            .filter(|entry| entry.kind == "source")
            .ok_or_else(|| AppError::not_found("image source not found"))?;
        let path = state
            .library
            .resolve_media_path(&entry)
            .await
            .map_err(|_| AppError::not_found("image source not found"))?;
        images.insert(id, path);
    }
    let mut files = std::collections::BTreeMap::new();
    for (composition, output) in request.rendered_outputs {
        if !request.project.compositions.contains_key(&composition) {
            return Err(AppError::bad_request("unknown rendered composition"));
        }
        let id = uuid::Uuid::parse_str(&output)
            .map_err(|_| AppError::bad_request("invalid rendered output ID"))?
            .to_string();
        if !state
            .db
            .can_access_output(&id, &actor)
            .await
            .map_err(|error| AppError::internal("authorize nested preview", error))?
        {
            return Err(AppError::not_found("rendered output not found"));
        }
        let path = super::publish::resolve_output_path(&state, &id)
            .await
            .map_err(|_| AppError::not_found("rendered output not found"))?;
        files.insert(composition, path);
    }
    let permit = NESTED_SLOTS
        .acquire()
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    struct CancelOnDrop(Arc<AtomicBool>);
    impl Drop for CancelOnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let _guard = CancelOnDrop(cancelled.clone());
    let token=tokio_util::sync::CancellationToken::new();let _cancel=token.clone().drop_guard();
    let frame = tokio::spawn(async move {
        let _permit=permit;
        let mut masks=super::concat_video_bound::BoundMasks::load(&state,&actor,request.saved_project.as_ref(),&request.project,&token).await?;
        let check=state.clone();let check_actor=actor.clone();let context=if masks.is_empty(){None}else{request.saved_project.clone()};
        let frame=tokio::task::spawn_blocking(move || {
            let mut filter=|node:&str,clip:&crate::domain::composition::CompositionClipId,source:&crate::domain::composition::SourceId,time,frame:&mut crate::domain::group_surface::GroupSurface|masks.apply(node,clip,source,time,frame);
            let frame=crate::domain::group_media::render_project_frame_sources_with_clip_filter(
                &request.project,request.composition_id.as_deref().unwrap_or(&request.project.root),
                std::path::Path::new("ffmpeg"),&files,&images,request.time_us,std::time::Duration::from_secs(30),&cancelled,None,Some(&mut filter),
            )?;
            let body=if png {
                use image::ImageEncoder;
                let mut bytes=Vec::new();
                image::codecs::png::PngEncoder::new(&mut bytes).write_image(&frame.rgba,u32::try_from(frame.width)?,u32::try_from(frame.height)?,image::ExtendedColorType::Rgba8)?;
                bytes
            }else{frame.rgba};
            anyhow::Ok((frame.width,frame.height,body))
        }).await??;
        if let Some(context)=context{
            super::cutout_assets::authorize(&check,&context.project_id,&check_actor).await.map_err(|_|anyhow::anyhow!("saved project access revoked"))?;
            let saved=check.db.get_composition_project_for(&context.project_id,&check_actor).await?.ok_or_else(||anyhow::anyhow!("saved project unavailable"))?;
            anyhow::ensure!(saved.revision==context.expected_revision,"saved project changed during preview");
        }
        anyhow::ensure!(!token.is_cancelled(),"saved preview cancelled");
        anyhow::Ok(frame)
    }).await.map_err(|error|AppError::bad_request(error.to_string()))?.map_err(|error|AppError::bad_request(error.to_string()))?;
    Ok((
        [
            ("content-type", if png { "image/png" } else { "application/octet-stream" }.to_string()),
            ("x-frame-width", frame.0.to_string()),
            ("x-frame-height", frame.1.to_string()),
            ("cache-control", "no-store".to_string()),
        ],
        frame.2,
    )
        .into_response())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NestedSplitRequest {
    project: NestedCompositionProject,
    parent: String,
    instance_id: String,
    right_id: String,
    time_us: u64,
}
pub async fn nested_split_handler(
    ApiJson(request): ApiJson<NestedSplitRequest>,
) -> AppResult<Json<NestedCompositionProject>> {
    let permit = NESTED_SLOTS
        .acquire()
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let project = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        request.project.split_instance(
            &request.parent,
            &request.instance_id,
            &request.right_id,
            request.time_us,
        )
    })
    .await
    .map_err(|error| AppError::bad_request(error.to_string()))?
    .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(project))
}


#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NestedDurationsRequest {
    project: NestedCompositionProject,
}
pub async fn nested_durations_handler(
    ApiJson(request): ApiJson<NestedDurationsRequest>,
) -> AppResult<Json<std::collections::BTreeMap<String, u64>>> {
    let permit = NESTED_SLOTS.acquire().await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let durations = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        request.project.durations_us()
    }).await.map_err(|error| AppError::bad_request(error.to_string()))?
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(durations))
}


#[derive(Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct NestedClipMoveRequest {
    pub project: NestedCompositionProject,
    pub moves: Vec<crate::domain::nested_clip_edit::ClipMove>,
}
static NESTED_EDIT_SLOTS: Semaphore = Semaphore::const_new(2);
pub async fn nested_move_clips_handler(ApiJson(request): ApiJson<NestedClipMoveRequest>) -> AppResult<Json<crate::domain::nested_clip_edit::ClipMoveResult>> {
    let permit = NESTED_EDIT_SLOTS.acquire().await.map_err(|error| AppError::bad_request(error.to_string()))?;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        crate::domain::nested_clip_edit::move_clips(&request.project, &request.moves)
    }).await.map_err(|error| AppError::bad_request(error.to_string()))?.map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(result))
}

/// Read-only source-clock planning; it does not bypass project admission.
pub async fn concat_retime_edit_handler(ApiJson(request):ApiJson<crate::domain::concat_retime_edit::PlanRequest>)->AppResult<Json<crate::domain::concat_retime_edit::EditPlan>>{
    let permit=NESTED_EDIT_SLOTS.acquire().await.map_err(|e|AppError::bad_request(e.to_string()))?;
    let plan=tokio::task::spawn_blocking(move||{let _permit=permit;crate::domain::concat_retime_edit::plan(request)}).await.map_err(|e|AppError::bad_request(e.to_string()))?.map_err(|e|AppError::bad_request(e.to_string()))?;
    Ok(Json(plan))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ProjectMediaPlaceholderRequest {
    project: NestedCompositionProject,
    source_id: crate::domain::composition::SourceId,
    placeholder: bool,
}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct ProjectMediaPlaceholderResponse { project: NestedCompositionProject, applied: bool }
pub async fn project_media_placeholder_handler(ApiJson(request):ApiJson<ProjectMediaPlaceholderRequest>)->AppResult<Json<ProjectMediaPlaceholderResponse>> {
    let permit=NESTED_SLOTS.acquire().await.map_err(|e|AppError::bad_request(e.to_string()))?;
    let (project,applied)=tokio::task::spawn_blocking(move||{let _permit=permit;crate::domain::project_media::set_placeholder(&request.project,&request.source_id,request.placeholder)})
        .await.map_err(|e|AppError::bad_request(e.to_string()))?.map_err(|e|AppError::bad_request(e.to_string()))?;
    Ok(Json(ProjectMediaPlaceholderResponse{project,applied}))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ProjectMediaRemoveRequest { project:NestedCompositionProject, source_id:crate::domain::composition::SourceId }
pub async fn project_media_remove_handler(ApiJson(request):ApiJson<ProjectMediaRemoveRequest>)->AppResult<Json<ProjectMediaPlaceholderResponse>> {
    let permit=NESTED_SLOTS.acquire().await.map_err(|e|AppError::bad_request(e.to_string()))?;
    let (project,applied)=tokio::task::spawn_blocking(move||{let _permit=permit;crate::domain::project_media::remove(&request.project,&request.source_id)})
        .await.map_err(|e|AppError::bad_request(e.to_string()))?.map_err(|e|AppError::bad_request(e.to_string()))?;
    Ok(Json(ProjectMediaPlaceholderResponse{project,applied}))
}
