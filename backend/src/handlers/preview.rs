use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderValue, Response, StatusCode};
use serde::Deserialize;

use crate::artifacts::fingerprint_file;
use crate::domain::artifact_graph::Fingerprint;
use crate::error::{ApiJson, AppError, AppResult};
use crate::model::{EditRequest, Scale};
use crate::ports::{ExportCommandCompiler, ExportCompileRequest};
use crate::preview_cache::{PreviewCacheAcquire, PreviewCacheBudget, PreviewCacheCatalog};
use crate::render::frame_renderer::{
    resolve_frame_path, FrameRenderService, FrameRequest, PreviewFrameFormat, PreviewRenderSettings,
};
use crate::services::render::{
    EditPlan, ExportExecutionProfile, RenderExecution, SourceMediaMetadata,
};
use crate::state::AppState;
use crate::tools::{
    self,
    preview_frame::{select_edited_output_frame, FfmpegPreviewFrameRenderer},
};

const PREVIEW_CACHE_BYTES: u64 = 256 * 1024 * 1024;
const PREVIEW_CACHE_ENTRIES: u64 = 4096;
const PREVIEW_LEASE_SECONDS: u64 = 45;
const PREVIEW_API_TIME_BASE: u32 = 1_000_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptimizedPreviewRequest {
    edit: EditRequest,
    timeline_tick: u64,
    timeline_time_base: u32,
    width: u32,
    height: u32,
    #[serde(default = "default_quality")]
    quality: u8,
}

fn default_quality() -> u8 {
    82
}

fn now_secs() -> i64 {
    crate::library::now_secs() as i64
}

fn publication_id_from_locator(locator: &str) -> Option<String> {
    std::path::Path::new(locator)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty() && value.len() <= 64)
        .map(str::to_owned)
}

pub async fn optimized_preview_frame(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<OptimizedPreviewRequest>,
) -> AppResult<Response<Body>> {
    let input = tools::find_source(&state.storage.join("sources"), &body.edit.video_id)
        .await
        .map_err(|error| AppError::not_found(format!("Source недоступен: {error}")))?;
    let cancellation = state.shutdown_token().child_token();
    let _cancel_on_drop = cancellation.clone().drop_guard();
    let source_identity =
        fingerprint_file(&state.cpu_pool, input.clone(), cancellation.child_token())
            .await
            .map_err(|error| AppError::internal("fingerprint preview source", error))?;
    let probe = tools::probe_video(&state.process_runtime, &input)
        .await
        .map_err(|error| AppError::bad_request(format!("Source не читается: {error}")))?;
    if probe.width == 0 || probe.height == 0 {
        return Err(AppError::bad_request("Preview frame требует видеодорожку"));
    }
    let metadata = SourceMediaMetadata::from_probe(&probe)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let resources = super::resolve_render_resources(&state, &body.edit)
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let graph_plan = EditPlan::compile(source_identity.sha256.clone(), body.edit.clone(), metadata)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let runtime = super::render_runtime_fingerprint(state.tools.as_ref());
    let graph_version = Fingerprint::combine([
        b"optimized-preview-graph-v1".as_slice(),
        graph_plan.plan_fingerprint.as_str().as_bytes(),
        resources.lut_sha256().unwrap_or("no-lut").as_bytes(),
        runtime.as_bytes(),
    ]);
    let compatibility = format!("ffmpeg-preview-v2:{runtime}");
    let settings = PreviewRenderSettings {
        timeline_tick: body.timeline_tick,
        timeline_time_base: body.timeline_time_base,
        width: body.width,
        height: body.height,
        max_fps_milli: 30_000,
        quality: body.quality,
        format: PreviewFrameFormat::Jpeg,
    };
    settings
        .validate()
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    if body.timeline_time_base != PREVIEW_API_TIME_BASE {
        return Err(AppError::bad_request(
            "Preview API требует канонический time base 1000000",
        ));
    }
    let request = FrameRequest {
        source_fingerprint: source_identity.sha256.clone(),
        graph_version,
        renderer_compatibility: compatibility.clone(),
        publication_id: "pending".into(),
        settings,
    };
    let key = request.key();
    let lock = state.render_lock(key.as_str()).await;
    let _guard = lock.lock().await;

    let mut still = body.edit;
    // Compile the complete temporal graph. Only the delivery profile changes;
    // frame selection is appended after trim/concat/reverse/speed/fades/grain.
    still.format = Some("mp4".into());
    still.codec = Some("h264".into());
    still.mute = true;
    still.quality = None;
    still.scale = Some(Scale {
        w: i32::try_from(body.width)
            .map_err(|_| AppError::bad_request("Preview width overflow"))?,
        h: i32::try_from(body.height)
            .map_err(|_| AppError::bad_request("Preview height overflow"))?,
    });
    let still_plan = Arc::new(
        EditPlan::compile(source_identity.sha256, still, metadata)
            .map_err(|error| AppError::bad_request(error.to_string()))?,
    );
    let execution = RenderExecution::new_with_resources(
        still_plan,
        ExportExecutionProfile {
            encode_budget: state.encode_budget.clone(),
            verify_checksums: true,
        },
        resources,
    );
    let placeholder = state.storage.join("frames").join("preview.jpg");
    let mut command = tools::FfmpegExportCompiler
        .compile(ExportCompileRequest {
            input: &input,
            destination: &placeholder,
            parallel_jobs: state.render_parallelism(),
            execution: &execution,
        })
        .map_err(|error| AppError::internal("compile optimized preview", error))?;
    select_edited_output_frame(
        &mut command,
        body.timeline_tick,
        body.timeline_time_base,
        body.quality,
    )
    .map_err(|error| AppError::bad_request(error.to_string()))?;
    let renderer = Arc::new(
        FfmpegPreviewFrameRenderer::new(
            state.process_runtime.clone(),
            command,
            compatibility,
            state.job_timeout(),
        )
        .map_err(|error| AppError::internal("configure optimized preview", error))?,
    );
    let service = FrameRenderService::new(renderer, state.storage.clone(), state.cpu_pool.clone());
    let catalog = PreviewCacheCatalog::new(
        state.db.clone(),
        PreviewCacheBudget {
            max_bytes: PREVIEW_CACHE_BYTES,
            max_entries: PREVIEW_CACHE_ENTRIES,
        },
    )
    .map_err(|error| AppError::internal("configure preview cache", error))?;
    catalog
        .reconcile_files(&state.storage, now_secs())
        .await
        .map_err(|error| AppError::internal("reconcile preview cache", error))?;

    let artifact = loop {
        match catalog
            .acquire(
                key.as_str(),
                now_secs(),
                Duration::from_secs(PREVIEW_LEASE_SECONDS),
            )
            .await
            .map_err(|error| AppError::internal("acquire preview cache", error))?
        {
            PreviewCacheAcquire::Ready(entry) => {
                let Some(publication_id) = publication_id_from_locator(&entry.locator) else {
                    let _ = catalog
                        .invalidate_ready(key.as_str(), &entry.checksum)
                        .await;
                    continue;
                };
                let mut ready_request = request.clone();
                ready_request.publication_id = publication_id;
                let artifact = service
                    .validated(&ready_request, cancellation.child_token())
                    .await
                    .map_err(|error| AppError::internal("verify optimized preview", error))?;
                if let Some(artifact) = artifact {
                    if entry.checksum == artifact.file.sha256.as_str()
                        && entry.locator == artifact.file.path
                    {
                        break artifact;
                    }
                }
                let _ = catalog
                    .invalidate_ready(key.as_str(), &entry.checksum)
                    .await;
            }
            PreviewCacheAcquire::Lease(lease) => {
                let mut leased_request = request.clone();
                leased_request.publication_id = lease.token.clone();
                let locator = leased_request
                    .artifact_locator()
                    .map_err(|error| AppError::internal("locate preview build", error))?;
                let reservation = u64::from(body.width)
                    .checked_mul(u64::from(body.height))
                    .and_then(|pixels| pixels.checked_mul(4))
                    .and_then(|raw| raw.checked_add(1024 * 1024))
                    .ok_or_else(|| AppError::bad_request("Preview reservation overflow"))?;
                catalog
                    .register_build_locator(&lease, &locator, reservation)
                    .await
                    .map_err(|error| AppError::internal("register preview build", error))?;
                catalog
                    .reconcile_files(&state.storage, now_secs())
                    .await
                    .map_err(|error| AppError::internal("drain preview cache GC", error))?;
                let _render_permit = tokio::select! {
                    _ = cancellation.cancelled() => return Err(AppError::conflict("Preview request отменён")),
                    permit = state.acquire_render_slot() => permit.map_err(|_| AppError::conflict("Render queue закрыта"))?,
                };
                match service
                    .ensure(leased_request, cancellation.child_token())
                    .await
                {
                    Ok(artifact) => {
                        let current_source = match fingerprint_file(
                            &state.cpu_pool,
                            input.clone(),
                            cancellation.child_token(),
                        )
                        .await
                        {
                            Ok(value) => value,
                            Err(error) => {
                                let _ = catalog.abandon(&lease).await;
                                return Err(AppError::internal("recheck preview source", error));
                            }
                        };
                        if current_source.sha256 != artifact.request.source_fingerprint {
                            let _ = catalog.abandon(&lease).await;
                            return Err(AppError::conflict(
                                "Source изменился во время preview render",
                            ));
                        }
                        match catalog
                            .publish(
                                &lease,
                                &artifact.file.path,
                                artifact.file.size,
                                artifact.file.sha256.as_str(),
                            )
                            .await
                        {
                            Ok(_published) => {
                                catalog
                                    .reconcile_files(&state.storage, now_secs())
                                    .await
                                    .map_err(|error| {
                                        AppError::internal("drain preview cache GC", error)
                                    })?;
                                break artifact;
                            }
                            Err(error) => {
                                let _ = catalog.abandon(&lease).await;
                                return Err(AppError::internal("publish preview cache", error));
                            }
                        }
                    }
                    Err(error) => {
                        let _ = catalog.abandon(&lease).await;
                        return Err(AppError::internal("render optimized preview", error));
                    }
                }
            }
            PreviewCacheAcquire::Busy => {
                tokio::select! {
                    _ = cancellation.cancelled() => return Err(AppError::conflict("Preview request отменён")),
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                }
            }
        }
    };
    let _path = resolve_frame_path(&state.storage, &artifact)
        .map_err(|error| AppError::internal("resolve optimized preview", error))?;
    let bytes = catalog
        .read_ready_bytes(
            &state.storage,
            key.as_str(),
            &artifact.file.path,
            artifact.file.sha256.as_str(),
        )
        .await
        .map_err(|error| AppError::internal("read optimized preview", error))?;
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    response.headers_mut().insert(
        "x-preview-cache-key",
        HeaderValue::from_str(key.as_str()).expect("fingerprint is an HTTP-safe value"),
    );
    Ok(response)
}
