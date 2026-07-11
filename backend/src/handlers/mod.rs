use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path as AxPath, State};
use axum::Json;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, Mutex, OwnedMutexGuard, OwnedSemaphorePermit};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::error::{ApiJson, AppError, AppResult};
use crate::library::MediaEntry;
use crate::model::{Crop, EditRequest, ImportRequest, Job, JobStatus, Scale, Trim};
use crate::state::{AppState, CancelJobOutcome};
use crate::tools::{self, Done};

mod health;
mod library;
mod projects;
mod upload;

pub use health::health_handler;
pub use library::{library_delete_handler, library_list_handler};
pub use projects::{
    project_by_video_handler, project_delete_handler, project_get_handler, project_list_handler,
    project_upsert_handler,
};
pub use upload::upload_handler;

/// Per-job wall-clock limit (download or render), overridable via env.
fn job_timeout() -> Duration {
    let secs = std::env::var("JOB_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1800);
    Duration::from_secs(secs)
}

/// `POST /api/import` — accept a video URL, kick off a download in the background,
/// and immediately return a job id to poll.
pub async fn import_handler(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<ImportRequest>,
) -> Json<Value> {
    let job_id = Uuid::new_v4().to_string();
    let video_id = Uuid::new_v4().to_string();
    state.set_job(Job::pending(job_id.clone())).await;
    let token = state.register_cancel(&job_id).await;

    let st = state.clone();
    let jid = job_id.clone();
    let vid = video_id;
    tokio::spawn(async move {
        // Reject bad/unsafe URLs before doing any work.
        if let Err(e) = tools::validate_url(&req.url).await {
            let updated = st
                .update_job_if_open(&jid, |j| {
                    j.status = JobStatus::Error;
                    j.error = Some(e.to_string());
                })
                .await;
            if updated {
                st.persist_job(&jid).await;
            }
            st.clear_cancel(&jid).await;
            return;
        }

        // Wait for a queue slot.
        if !mark_queued(&st, &jid).await {
            st.clear_cancel(&jid).await;
            return;
        }
        let _permit = match acquire_job_permit_or_cancelled(&st, &jid, &token).await {
            Some(p) => p,
            None => return,
        };
        if token.is_cancelled() {
            mark_cancelled(&st, &jid).await;
            return;
        }

        if !mark_running(&st, &jid, "downloading").await {
            st.clear_cancel(&jid).await;
            return;
        }

        let (tx, rx) = mpsc::unbounded_channel::<f64>();
        let drain = spawn_progress_drain(st.clone(), jid.clone(), rx);

        let sources = st.sources_dir();
        let outcome = async {
            let done = tools::download_video(
                &req.url,
                &sources,
                &vid,
                req.start,
                req.end,
                &tx,
                &token,
                job_timeout(),
            )
            .await?;
            if matches!(done, Done::Cancelled) {
                cleanup_files_with_prefix(&sources, &vid).await;
                return Ok::<Option<Value>, anyhow::Error>(None);
            }
            let path = tools::find_source(&sources, &vid).await?;
            let info = tools::probe_video(&path).await?;
            let title = tools::read_title(&sources, &vid).await;
            let size = tokio::fs::metadata(&path).await.map(|m| m.len()).ok();
            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("{vid}.mp4"));
            Ok(Some(json!({
                "id": vid,
                "url": format!("/files/sources/{filename}"),
                "filename": filename,
                "duration": info.duration,
                "width": info.width,
                "height": info.height,
                "title": title,
                "fps": info.fps,
                "vcodec": info.vcodec,
                "acodec": info.acodec,
                "sizeBytes": size,
            })))
        }
        .await;

        if outcome.is_err() {
            cleanup_files_with_prefix(&sources, &vid).await;
        }

        drop(tx);
        let _ = drain.await;
        finish_job(&st, &jid, outcome, "source").await;
    });

    Json(json!({ "jobId": job_id }))
}

/// Content key for the render cache: a hash of the canonical (source + edit)
/// request. Re-serializing the deserialized `EditRequest` normalises omitted
/// defaults, so two equivalent requests map to the same key.
pub fn render_cache_key(req: &EditRequest) -> String {
    let canonical = serde_json::to_string(req).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// `POST /api/edit` — apply trim/crop/scale/mute/speed to a previously imported
/// video and return a job id to poll for the rendered result.
pub async fn edit_handler(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<EditRequest>,
) -> Json<Value> {
    let job_id = Uuid::new_v4().to_string();
    let out_id = Uuid::new_v4().to_string();
    state.set_job(Job::pending(job_id.clone())).await;
    let token = state.register_cancel(&job_id).await;

    // Content-addressed cache: an identical (source + edit) render is reused
    // instead of running ffmpeg again, as long as the output file still exists.
    let cache_key = render_cache_key(&req);
    if finish_from_render_cache(&state, &job_id, &cache_key).await {
        return Json(json!({ "jobId": job_id }));
    }

    let st = state.clone();
    let jid = job_id.clone();
    tokio::spawn(async move {
        if !mark_queued(&st, &jid).await {
            st.clear_cancel(&jid).await;
            return;
        }
        let render_lock = st.render_lock(&cache_key).await;
        let _render_guard =
            match acquire_render_lock_or_cancelled(&st, &jid, &token, render_lock).await {
                Some(g) => g,
                None => return,
            };
        if finish_from_render_cache(&st, &jid, &cache_key).await {
            return;
        }

        let _permit = match acquire_job_permit_or_cancelled(&st, &jid, &token).await {
            Some(p) => p,
            None => return,
        };
        if token.is_cancelled() {
            mark_cancelled(&st, &jid).await;
            return;
        }

        if !mark_running(&st, &jid, "processing").await {
            st.clear_cancel(&jid).await;
            return;
        }

        let (tx, rx) = mpsc::unbounded_channel::<f64>();
        let drain = spawn_progress_drain(st.clone(), jid.clone(), rx);

        let sources = st.sources_dir();
        let outputs = st.outputs_dir();
        let mut req = req;
        let ext = tools::output_ext(req.format.as_deref());
        let filename = format!("{out_id}.{ext}");
        let output_path = outputs.join(&filename);

        let outcome = async {
            let input = tools::find_source(&sources, &req.video_id).await?;
            let probe = tools::probe_video(&input).await?;
            normalize_edit_request(&mut req, probe.width, probe.height, probe.duration)?;
            let expected = tools::expected_output_secs(&req, probe.duration);
            let args = tools::build_ffmpeg_args(&input, &output_path, &req, probe.duration);
            tracing::info!("ffmpeg {}", args.join(" "));
            let done = tools::run_ffmpeg(&args, expected, &tx, &token, job_timeout()).await?;
            if matches!(done, Done::Cancelled) {
                let _ = tokio::fs::remove_file(&output_path).await;
                return Ok::<Option<Value>, anyhow::Error>(None);
            }
            let size = tokio::fs::metadata(&output_path)
                .await
                .map(|m| m.len())
                .ok();
            Ok(Some(json!({
                "id": out_id,
                "url": format!("/files/outputs/{filename}"),
                "filename": filename,
                "sizeBytes": size,
            })))
        }
        .await;

        if outcome.is_err() {
            let _ = tokio::fs::remove_file(&output_path).await;
        }

        let cache_info = match &outcome {
            Ok(Some(info)) => Some(info.clone()),
            _ => None,
        };

        drop(tx);
        let _ = drain.await;
        let updated = finish_job(&st, &jid, outcome, "output").await;
        if updated {
            if let Some(info) = &cache_info {
                if let Err(error) = st.db.cache_put(&cache_key, info, &filename).await {
                    tracing::warn!("cache successful render {cache_key}: {error}");
                }
            }
        }
    });

    Json(json!({ "jobId": job_id }))
}

/// `GET /api/jobs/:id` — poll the status of an import or edit job.
pub async fn job_status_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<Job>> {
    match state.get_job(&id).await {
        Some(job) => Ok(Json(job)),
        None => Err(AppError::not_found("Задача не найдена")),
    }
}

/// `POST /api/jobs/:id/cancel` — request cancellation of a running/pending job.
pub async fn cancel_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<Value>> {
    match state.cancel_open_job(&id).await {
        CancelJobOutcome::NotFound => Err(AppError::not_found("Задача не найдена")),
        CancelJobOutcome::AlreadyFinished => Err(AppError::conflict("Задача уже завершена")),
        CancelJobOutcome::Cancelled => Ok(Json(json!({ "status": "cancelled" }))),
    }
}

pub async fn api_not_found_handler() -> AppError {
    AppError::not_found("API-маршрут не найден")
}

pub async fn method_not_allowed_handler() -> AppError {
    AppError::method_not_allowed("Метод не поддерживается")
}

/// Drain numeric progress updates from a worker into the job record.
fn spawn_progress_drain(
    st: AppState,
    jid: String,
    mut rx: mpsc::UnboundedReceiver<f64>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut last_saved = -1.0_f64;
        while let Some(p) = rx.recv().await {
            if p >= 100.0 || p - last_saved >= 1.0 {
                last_saved = p;
                st.update_job_if_open(&jid, |j| j.progress = Some(p)).await;
            }
        }
    })
}

/// Atomically start an open job. Cancellation may win immediately before this
/// transition; in that case the terminal state must never be overwritten.
async fn mark_running(st: &AppState, jid: &str, stage: &str) -> bool {
    st.update_job_if_open(jid, |j| {
        j.status = JobStatus::Running;
        j.stage = Some(stage.into());
        j.progress = Some(0.0);
    })
    .await
}

async fn mark_queued(st: &AppState, jid: &str) -> bool {
    st.update_job_if_open(jid, |j| j.stage = Some("queued".into()))
        .await
}

async fn finish_from_render_cache(st: &AppState, jid: &str, cache_key: &str) -> bool {
    let Ok(Some((output, filename))) = st.db.cache_get(cache_key).await else {
        return false;
    };
    if !is_plain_filename(&filename) {
        let _ = st.db.cache_delete(cache_key).await;
        return false;
    }
    if tokio::fs::metadata(st.outputs_dir().join(&filename))
        .await
        .is_err()
    {
        let _ = st.db.cache_delete(cache_key).await;
        return false;
    }
    let updated = st
        .update_job_if_open(jid, |j| {
            j.status = JobStatus::Done;
            j.result = Some(output);
            j.progress = Some(100.0);
            j.stage = None;
        })
        .await;
    if updated {
        st.persist_job(jid).await;
    }
    st.clear_cancel(jid).await;
    true
}

fn is_plain_filename(filename: &str) -> bool {
    !filename.is_empty()
        && !filename.contains('/')
        && !filename.contains('\\')
        && filename != "."
        && filename != ".."
        && Path::new(filename)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == filename)
}

async fn acquire_render_lock_or_cancelled(
    st: &AppState,
    jid: &str,
    token: &CancellationToken,
    lock: Arc<Mutex<()>>,
) -> Option<OwnedMutexGuard<()>> {
    tokio::select! {
        guard = lock.lock_owned() => Some(guard),
        _ = token.cancelled() => {
            mark_cancelled(st, jid).await;
            None
        }
    }
}

async fn acquire_job_permit_or_cancelled(
    st: &AppState,
    jid: &str,
    token: &CancellationToken,
) -> Option<OwnedSemaphorePermit> {
    tokio::select! {
        permit = st.acquire_job_slot() => match permit {
            Ok(p) => Some(p),
            Err(_) => {
                mark_queue_closed(st, jid).await;
                None
            }
        },
        _ = token.cancelled() => {
            mark_cancelled(st, jid).await;
            None
        }
    }
}

async fn mark_cancelled(st: &AppState, jid: &str) {
    let updated = st
        .update_job_if_open(jid, |j| {
            j.status = JobStatus::Cancelled;
            j.stage = None;
            j.progress = None;
        })
        .await;
    if updated {
        st.persist_job(jid).await;
    }
    st.clear_cancel(jid).await;
}

async fn mark_queue_closed(st: &AppState, jid: &str) {
    let updated = st
        .update_job_if_open(jid, |j| {
            j.status = JobStatus::Error;
            j.error = Some("очередь задач закрыта".into());
            j.stage = None;
            j.progress = None;
        })
        .await;
    if updated {
        st.persist_job(jid).await;
    }
    st.clear_cancel(jid).await;
}

fn normalize_edit_request(
    edit: &mut EditRequest,
    source_width: u32,
    source_height: u32,
    source_duration: f64,
) -> anyhow::Result<()> {
    let duration = finite_non_negative(source_duration, "Недопустимая длительность источника")?;

    edit.speed = finite_positive(edit.speed, "Недопустимая скорость")?.clamp(0.5, 2.0);
    edit.volume = finite_non_negative(edit.volume, "Недопустимая громкость")?.clamp(0.0, 4.0);
    edit.fade_in = finite_non_negative(edit.fade_in, "Недопустимое появление")?.min(duration);
    edit.fade_out = finite_non_negative(edit.fade_out, "Недопустимое затухание")?.min(duration);
    edit.brightness = finite_number(edit.brightness, "Недопустимая яркость")?.clamp(-1.0, 1.0);
    edit.contrast = finite_non_negative(edit.contrast, "Недопустимый контраст")?.clamp(0.0, 3.0);
    edit.saturation =
        finite_non_negative(edit.saturation, "Недопустимая насыщенность")?.clamp(0.0, 3.0);
    edit.sharpen = finite_non_negative(edit.sharpen, "Недопустимая резкость")?.clamp(0.0, 5.0);
    edit.grain = finite_non_negative(edit.grain, "Недопустимое зерно")?.clamp(0.0, 100.0);
    normalize_trim(&mut edit.trim, duration)?;
    normalize_segments(&mut edit.segments, duration)?;

    if let Some(crop) = edit.crop.as_mut() {
        clamp_rect_to_source(crop, source_width, source_height);
    }
    if let Some(censor) = edit.censor.as_mut() {
        clamp_rect_to_source(censor, source_width, source_height);
    }
    if let Some(fps) = edit.fps {
        if !fps.is_finite() || fps <= 0.0 {
            anyhow::bail!("Недопустимый fps");
        }
        edit.fps = Some(fps.clamp(1.0, 240.0));
    }
    if let Some(scale) = &edit.scale {
        validate_scale(scale)?;
    }
    Ok(())
}

fn finite_number(value: f64, msg: &str) -> anyhow::Result<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        anyhow::bail!(msg.to_string())
    }
}

fn finite_non_negative(value: f64, msg: &str) -> anyhow::Result<f64> {
    let value = finite_number(value, msg)?;
    if value >= 0.0 {
        Ok(value)
    } else {
        anyhow::bail!(msg.to_string())
    }
}

fn finite_positive(value: f64, msg: &str) -> anyhow::Result<f64> {
    let value = finite_number(value, msg)?;
    if value > 0.0 {
        Ok(value)
    } else {
        anyhow::bail!(msg.to_string())
    }
}

fn normalize_trim(trim: &mut Option<Trim>, duration: f64) -> anyhow::Result<()> {
    let Some(t) = trim.as_mut() else {
        return Ok(());
    };
    t.start = finite_non_negative(t.start, "Недопустимое начало обрезки")?.min(duration);
    t.end = finite_non_negative(t.end, "Недопустимый конец обрезки")?.min(duration);
    if t.end - t.start <= 0.01 {
        anyhow::bail!("Недопустимый диапазон обрезки");
    }
    Ok(())
}

fn normalize_segments(segments: &mut Option<Vec<Trim>>, duration: f64) -> anyhow::Result<()> {
    let Some(segs) = segments.as_mut() else {
        return Ok(());
    };
    let mut normalized = Vec::with_capacity(segs.len());
    for s in segs.iter() {
        let start = finite_non_negative(s.start, "Недопустимое начало сегмента")?.min(duration);
        let end = finite_non_negative(s.end, "Недопустимый конец сегмента")?.min(duration);
        if end - start > 0.01 {
            normalized.push(Trim { start, end });
        }
    }
    normalized.sort_by(|a, b| a.start.total_cmp(&b.start));
    for pair in normalized.windows(2) {
        if pair[1].start < pair[0].end {
            anyhow::bail!("Сегменты не должны пересекаться");
        }
    }
    *segments = (!normalized.is_empty()).then_some(normalized);
    Ok(())
}

fn clamp_rect_to_source(rect: &mut Crop, source_width: u32, source_height: u32) {
    if source_width == 0 || source_height == 0 {
        return;
    }

    let min_w = if source_width >= 2 { 2 } else { 1 };
    let min_h = if source_height >= 2 { 2 } else { 1 };
    rect.x = rect.x.min(source_width - min_w);
    rect.y = rect.y.min(source_height - min_h);

    let max_w = source_width - rect.x;
    let max_h = source_height - rect.y;
    rect.w = rect.w.clamp(min_w, max_w);
    rect.h = rect.h.clamp(min_h, max_h);
    if min_w == 2 {
        rect.w = (rect.w & !1).max(2);
    }
    if min_h == 2 {
        rect.h = (rect.h & !1).max(2);
    }
}

fn validate_scale(scale: &Scale) -> anyhow::Result<()> {
    fn valid_dim(v: i32) -> bool {
        matches!(v, -2 | -1) || (2..=7680).contains(&v)
    }
    if !valid_dim(scale.w) || !valid_dim(scale.h) || (scale.w < 0 && scale.h < 0) {
        anyhow::bail!("Недопустимый размер экспорта");
    }
    Ok(())
}

async fn cleanup_files_with_prefix(dir: &Path, prefix: &str) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let matches_prefix = name == prefix
            || name
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('.'));
        if name == ".gitkeep" || !matches_prefix {
            continue;
        }
        let _ = tokio::fs::remove_file(entry.path()).await;
    }
}

/// Apply the terminal outcome of a worker to the job and clear its cancel token.
/// `Ok(Some)` -> done (also recorded in the media library under `kind`),
/// `Ok(None)` -> cancelled, `Err` -> error.
async fn finish_job(
    st: &AppState,
    jid: &str,
    outcome: anyhow::Result<Option<Value>>,
    kind: &str,
) -> bool {
    let updated = match outcome {
        Ok(Some(info)) => {
            let updated = st
                .update_job_if_open(jid, |j| {
                    j.status = JobStatus::Done;
                    j.result = Some(info.clone());
                    j.progress = Some(100.0);
                    j.stage = None;
                })
                .await;
            if updated {
                st.library.add(MediaEntry::from_result(kind, &info)).await;
            }
            updated
        }
        Ok(None) => {
            st.update_job_if_open(jid, |j| {
                j.status = JobStatus::Cancelled;
                j.stage = None;
                j.progress = None;
            })
            .await
        }
        Err(e) => {
            tracing::error!("job {jid} failed: {e}");
            st.update_job_if_open(jid, |j| {
                j.status = JobStatus::Error;
                j.error = Some(e.to_string());
                j.stage = None;
            })
            .await
        }
    };
    // Persist the terminal state so it survives a restart.
    if updated {
        st.persist_job(jid).await;
    }
    st.clear_cancel(jid).await;
    updated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::library::Library;
    use crate::state::ToolInfo;
    use serde_json::json;

    async fn state() -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        tokio::fs::create_dir_all(storage.join("sources"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(storage.join("outputs"))
            .await
            .unwrap();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        (AppState::new(storage, 2, ToolInfo::default(), lib, db), dir)
    }

    #[tokio::test]
    async fn finish_job_does_not_overwrite_terminal_cancelled_job() {
        let (st, _dir) = state().await;
        st.set_job(Job::pending("j1".into())).await;
        st.update_job("j1", |j| j.status = JobStatus::Cancelled)
            .await;
        st.persist_job("j1").await;

        let updated = finish_job(
            &st,
            "j1",
            Ok(Some(json!({
                "id": "out",
                "filename": "out.mp4",
                "url": "/files/outputs/out.mp4"
            }))),
            "output",
        )
        .await;

        assert!(!updated);
        let job = st.get_job("j1").await.unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.result.is_none());
        assert!(st.library.list().await.is_empty());
    }

    #[tokio::test]
    async fn mark_running_does_not_revive_a_cancelled_job() {
        let (st, _dir) = state().await;
        st.set_job(Job::pending("cancelled-before-start".into()))
            .await;
        assert_eq!(
            st.cancel_open_job("cancelled-before-start").await,
            CancelJobOutcome::Cancelled
        );

        assert!(!mark_running(&st, "cancelled-before-start", "processing").await);
        assert!(!mark_queued(&st, "cancelled-before-start").await);
        let job = st.get_job("cancelled-before-start").await.unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.stage.is_none());
        assert!(job.progress.is_none());
    }

    #[test]
    fn cache_filenames_must_be_plain_output_names() {
        assert!(is_plain_filename("output.mp4"));
        assert!(is_plain_filename("b5b2b5b2-clip.webm"));

        for filename in ["", ".", "..", "../x.mp4", "dir/x.mp4", "dir\\x.mp4"] {
            assert!(
                !is_plain_filename(filename),
                "{filename:?} must be rejected"
            );
        }
    }

    #[tokio::test]
    async fn finish_from_render_cache_validates_filename_and_file() {
        let (st, _dir) = state().await;

        st.set_job(Job::pending("valid".into())).await;
        let output = json!({
            "id": "out",
            "filename": "out.mp4",
            "url": "/files/outputs/out.mp4"
        });
        tokio::fs::write(st.outputs_dir().join("out.mp4"), b"video")
            .await
            .unwrap();
        st.db
            .cache_put("valid-key", &output, "out.mp4")
            .await
            .unwrap();

        assert!(finish_from_render_cache(&st, "valid", "valid-key").await);
        let job = st.get_job("valid").await.unwrap();
        assert_eq!(job.status, JobStatus::Done);
        assert_eq!(job.progress, Some(100.0));
        assert_eq!(job.result.unwrap()["filename"], "out.mp4");

        st.set_job(Job::pending("unsafe".into())).await;
        st.db
            .cache_put(
                "unsafe-key",
                &json!({ "filename": "../leak.mp4" }),
                "../leak.mp4",
            )
            .await
            .unwrap();
        assert!(!finish_from_render_cache(&st, "unsafe", "unsafe-key").await);
        assert!(st.db.cache_get("unsafe-key").await.unwrap().is_none());
        assert_eq!(
            st.get_job("unsafe").await.unwrap().status,
            JobStatus::Pending
        );

        st.set_job(Job::pending("missing".into())).await;
        st.db
            .cache_put(
                "missing-key",
                &json!({ "filename": "missing.mp4" }),
                "missing.mp4",
            )
            .await
            .unwrap();
        assert!(!finish_from_render_cache(&st, "missing", "missing-key").await);
        assert!(st.db.cache_get("missing-key").await.unwrap().is_none());
        assert_eq!(
            st.get_job("missing").await.unwrap().status,
            JobStatus::Pending
        );
    }

    #[tokio::test]
    async fn cleanup_files_with_prefix_removes_partial_imports() {
        let (st, _dir) = state().await;
        let dir = st.sources_dir();
        tokio::fs::write(dir.join("abc.mp4.part"), b"x")
            .await
            .unwrap();
        tokio::fs::write(dir.join("abc.info.json"), b"x")
            .await
            .unwrap();
        tokio::fs::write(dir.join("abcd.mp4"), b"x").await.unwrap();
        tokio::fs::write(dir.join("abc"), b"x").await.unwrap();
        tokio::fs::write(dir.join(".gitkeep"), b"").await.unwrap();

        cleanup_files_with_prefix(&dir, "abc").await;

        assert!(!dir.join("abc.mp4.part").exists());
        assert!(!dir.join("abc.info.json").exists());
        assert!(!dir.join("abc").exists());
        assert!(dir.join("abcd.mp4").exists());
        assert!(dir.join(".gitkeep").exists());
    }

    #[test]
    fn normalize_edit_request_clamps_rectangles_to_source() {
        let mut edit: EditRequest = serde_json::from_value(json!({
            "videoId": "x",
            "crop": { "x": 9999, "y": 9999, "w": 0, "h": 9999 },
            "censor": { "x": 1919, "y": 1079, "w": 20, "h": 20 },
            "fps": 500.0,
            "scale": { "w": 1280, "h": -2 }
        }))
        .unwrap();

        normalize_edit_request(&mut edit, 1920, 1080, 10.0).unwrap();

        let crop = edit.crop.unwrap();
        assert_eq!((crop.x, crop.y, crop.w, crop.h), (1918, 1078, 2, 2));
        let censor = edit.censor.unwrap();
        assert_eq!((censor.x, censor.y, censor.w, censor.h), (1918, 1078, 2, 2));
        assert_eq!(edit.fps, Some(240.0));
    }

    #[test]
    fn normalize_edit_request_rejects_invalid_fps_and_scale() {
        let mut bad_fps: EditRequest =
            serde_json::from_value(json!({ "videoId": "x", "fps": -1.0 })).unwrap();
        assert!(normalize_edit_request(&mut bad_fps, 100, 100, 10.0).is_err());

        let mut bad_scale: EditRequest = serde_json::from_value(json!({
            "videoId": "x",
            "scale": { "w": -1, "h": -2 }
        }))
        .unwrap();
        assert!(normalize_edit_request(&mut bad_scale, 100, 100, 10.0).is_err());
    }

    #[test]
    fn normalize_edit_request_clamps_timing_and_effect_numbers() {
        let mut edit: EditRequest = serde_json::from_value(json!({
            "videoId": "x",
            "speed": 3.5,
            "volume": 9.0,
            "fadeIn": 99.0,
            "fadeOut": 99.0,
            "brightness": 2.0,
            "contrast": 9.0,
            "saturation": 9.0,
            "sharpen": 9.0,
            "grain": 999.0,
            "trim": { "start": 2.0, "end": 99.0 },
            "segments": [
                { "start": 9.5, "end": 99.0 },
                { "start": 0.0, "end": 1.0 },
                { "start": 2.0, "end": 2.0 }
            ]
        }))
        .unwrap();

        normalize_edit_request(&mut edit, 100, 100, 10.0).unwrap();

        assert_eq!(edit.speed, 2.0);
        assert_eq!(edit.volume, 4.0);
        assert_eq!(edit.fade_in, 10.0);
        assert_eq!(edit.fade_out, 10.0);
        assert_eq!(edit.brightness, 1.0);
        assert_eq!(edit.contrast, 3.0);
        assert_eq!(edit.saturation, 3.0);
        assert_eq!(edit.sharpen, 5.0);
        assert_eq!(edit.grain, 100.0);
        let trim = edit.trim.unwrap();
        assert_eq!((trim.start, trim.end), (2.0, 10.0));
        let segs = edit.segments.unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!((segs[0].start, segs[0].end), (0.0, 1.0));
        assert_eq!((segs[1].start, segs[1].end), (9.5, 10.0));
    }

    #[test]
    fn normalize_edit_request_rejects_bad_timing() {
        let mut bad_speed: EditRequest =
            serde_json::from_value(json!({ "videoId": "x", "speed": 0.0 })).unwrap();
        assert!(normalize_edit_request(&mut bad_speed, 100, 100, 10.0).is_err());

        let mut bad_trim: EditRequest = serde_json::from_value(json!({
            "videoId": "x",
            "trim": { "start": 5.0, "end": 5.0 }
        }))
        .unwrap();
        assert!(normalize_edit_request(&mut bad_trim, 100, 100, 10.0).is_err());

        let mut overlap: EditRequest = serde_json::from_value(json!({
            "videoId": "x",
            "segments": [
                { "start": 0.0, "end": 2.0 },
                { "start": 1.0, "end": 3.0 }
            ]
        }))
        .unwrap();
        assert!(normalize_edit_request(&mut overlap, 100, 100, 10.0).is_err());

        let mut non_finite: EditRequest =
            serde_json::from_value(json!({ "videoId": "x" })).unwrap();
        non_finite.volume = f64::INFINITY;
        assert!(normalize_edit_request(&mut non_finite, 100, 100, 10.0).is_err());
    }
}
