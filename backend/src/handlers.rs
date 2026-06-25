use std::time::Duration;

use axum::extract::{Multipart, Path as AxPath, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::library::MediaEntry;
use crate::model::{EditRequest, ImportRequest, Job, JobStatus};
use crate::state::AppState;
use crate::tools::{self, Done};

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
    Json(req): Json<ImportRequest>,
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
        if let Err(e) = tools::validate_url(&req.url) {
            st.update_job(&jid, |j| {
                j.status = JobStatus::Error;
                j.error = Some(e.to_string());
            })
            .await;
            st.clear_cancel(&jid).await;
            return;
        }

        // Wait for a queue slot.
        st.update_job(&jid, |j| j.stage = Some("queued".into()))
            .await;
        let _permit = match st.jobs_semaphore.clone().acquire_owned().await {
            Ok(p) => p,
            Err(_) => return,
        };
        if token.is_cancelled() {
            st.update_job(&jid, |j| {
                j.status = JobStatus::Cancelled;
                j.stage = None;
            })
            .await;
            st.clear_cancel(&jid).await;
            return;
        }

        st.update_job(&jid, |j| {
            j.status = JobStatus::Running;
            j.stage = Some("downloading".into());
            j.progress = Some(0.0);
        })
        .await;

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

        drop(tx);
        let _ = drain.await;
        finish_job(&st, &jid, outcome, "source").await;
    });

    Json(json!({ "jobId": job_id }))
}

/// `POST /api/upload` — accept a multipart file upload, store it as a source,
/// probe it, and return the same VideoInfo shape as a completed import (no job:
/// the work is just a disk write plus a quick probe).
pub async fn upload_handler(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Value>, (StatusCode, String)> {
    let video_id = Uuid::new_v4().to_string();
    let sources = state.sources_dir();

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let original = field.file_name().map(|s| s.to_string());
        if field.name() != Some("file") && original.is_none() {
            continue;
        }
        let ext = sanitize_ext(original.as_deref());
        let filename = format!("{video_id}.{ext}");
        let path = sources.join(&filename);

        // Stream the upload to disk chunk by chunk instead of buffering the
        // whole file in memory (videos can be gigabytes).
        let mut total: u64 = 0;
        {
            let mut file = tokio::fs::File::create(&path)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            while let Some(chunk) = field.chunk().await.map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("не удалось прочитать файл: {e}"),
                )
            })? {
                file.write_all(&chunk)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                total += chunk.len() as u64;
            }
            file.flush()
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        if total == 0 {
            let _ = tokio::fs::remove_file(&path).await;
            return Err((StatusCode::BAD_REQUEST, "пустой файл".into()));
        }

        let info = match tools::probe_video(&path).await {
            Ok(i) if i.width > 0 || i.duration > 0.0 => i,
            _ => {
                let _ = tokio::fs::remove_file(&path).await;
                return Err((
                    StatusCode::BAD_REQUEST,
                    "не удалось распознать видео в файле".into(),
                ));
            }
        };
        let size = tokio::fs::metadata(&path).await.map(|m| m.len()).ok();
        let title = original.as_deref().map(|n| {
            std::path::Path::new(n)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(n)
                .to_string()
        });

        let body = json!({
            "id": video_id,
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
        });
        state
            .library
            .add(MediaEntry::from_result("source", &body))
            .await;
        return Ok(Json(body));
    }

    Err((StatusCode::BAD_REQUEST, "файл не найден в запросе".into()))
}

/// Keep only a short alphanumeric extension to avoid path tricks / odd names.
fn sanitize_ext(original: Option<&str>) -> String {
    let ext = original
        .and_then(|n| std::path::Path::new(n).extension())
        .and_then(|e| e.to_str())
        .unwrap_or("mp4");
    let clean: String = ext
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(5)
        .collect::<String>()
        .to_lowercase();
    if clean.is_empty() {
        "mp4".into()
    } else {
        clean
    }
}

/// `POST /api/edit` — apply trim/crop/scale/mute/speed to a previously imported
/// video and return a job id to poll for the rendered result.
pub async fn edit_handler(
    State(state): State<AppState>,
    Json(req): Json<EditRequest>,
) -> Json<Value> {
    let job_id = Uuid::new_v4().to_string();
    let out_id = Uuid::new_v4().to_string();
    state.set_job(Job::pending(job_id.clone())).await;
    let token = state.register_cancel(&job_id).await;

    let st = state.clone();
    let jid = job_id.clone();
    tokio::spawn(async move {
        st.update_job(&jid, |j| j.stage = Some("queued".into()))
            .await;
        let _permit = match st.jobs_semaphore.clone().acquire_owned().await {
            Ok(p) => p,
            Err(_) => return,
        };
        if token.is_cancelled() {
            st.update_job(&jid, |j| {
                j.status = JobStatus::Cancelled;
                j.stage = None;
            })
            .await;
            st.clear_cancel(&jid).await;
            return;
        }

        st.update_job(&jid, |j| {
            j.status = JobStatus::Running;
            j.stage = Some("processing".into());
            j.progress = Some(0.0);
        })
        .await;

        let (tx, rx) = mpsc::unbounded_channel::<f64>();
        let drain = spawn_progress_drain(st.clone(), jid.clone(), rx);

        let sources = st.sources_dir();
        let outputs = st.outputs_dir();
        let ext = tools::output_ext(req.format.as_deref());
        let filename = format!("{out_id}.{ext}");
        let output_path = outputs.join(&filename);

        let outcome = async {
            let input = tools::find_source(&sources, &req.video_id).await?;
            let probe = tools::probe_video(&input).await?;
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

        drop(tx);
        let _ = drain.await;
        finish_job(&st, &jid, outcome, "output").await;
    });

    Json(json!({ "jobId": job_id }))
}

/// `GET /api/jobs/:id` — poll the status of an import or edit job.
pub async fn job_status_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Job>, StatusCode> {
    match state.get_job(&id).await {
        Some(job) => Ok(Json(job)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// `POST /api/jobs/:id/cancel` — request cancellation of a running/pending job.
pub async fn cancel_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> (StatusCode, Json<Value>) {
    match state.get_job(&id).await {
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "job not found" })),
        ),
        Some(job) if job.status.is_terminal() => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "job already finished" })),
        ),
        Some(_) => {
            state.cancel(&id).await;
            state
                .update_job(&id, |j| {
                    if !j.status.is_terminal() {
                        j.status = JobStatus::Cancelled;
                        j.stage = None;
                    }
                })
                .await;
            (StatusCode::OK, Json(json!({ "status": "cancelled" })))
        }
    }
}

/// `GET /api/library` — list persisted sources and outputs, newest first.
pub async fn library_list_handler(
    State(state): State<AppState>,
) -> Json<Vec<crate::library::MediaEntry>> {
    Json(state.library.list().await)
}

/// `DELETE /api/library/:id` — remove a library entry and delete its file.
pub async fn library_delete_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> StatusCode {
    if state.library.remove(&id).await {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

/// `GET /api/health` — readiness plus external tool availability/versions.
pub async fn health_handler(State(state): State<AppState>) -> Json<Value> {
    let t = &state.tools;
    Json(json!({
        "status": if t.ffmpeg && t.ytdlp { "ok" } else { "degraded" },
        "ffmpeg": t.ffmpeg,
        "ytdlp": t.ytdlp,
        "ffmpegVersion": t.ffmpeg_version,
        "ytdlpVersion": t.ytdlp_version,
    }))
}

/// Drain numeric progress updates from a worker into the job record.
fn spawn_progress_drain(
    st: AppState,
    jid: String,
    mut rx: mpsc::UnboundedReceiver<f64>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            st.update_job(&jid, |j| j.progress = Some(p)).await;
        }
    })
}

/// Apply the terminal outcome of a worker to the job and clear its cancel token.
/// `Ok(Some)` -> done (also recorded in the media library under `kind`),
/// `Ok(None)` -> cancelled, `Err` -> error.
async fn finish_job(st: &AppState, jid: &str, outcome: anyhow::Result<Option<Value>>, kind: &str) {
    match outcome {
        Ok(Some(info)) => {
            st.library.add(MediaEntry::from_result(kind, &info)).await;
            st.update_job(jid, |j| {
                j.status = JobStatus::Done;
                j.result = Some(info);
                j.progress = Some(100.0);
                j.stage = None;
            })
            .await
        }
        Ok(None) => {
            st.update_job(jid, |j| {
                j.status = JobStatus::Cancelled;
                j.stage = None;
                j.progress = None;
            })
            .await
        }
        Err(e) => {
            st.update_job(jid, |j| {
                j.status = JobStatus::Error;
                j.error = Some(e.to_string());
                j.stage = None;
            })
            .await
        }
    }
    st.clear_cancel(jid).await;
}
