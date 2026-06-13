use std::time::Duration;

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use uuid::Uuid;

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
        st.update_job(&jid, |j| j.stage = Some("queued".into())).await;
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
            let done =
                tools::download_video(&req.url, &sources, &vid, req.start, req.end, &tx, &token, job_timeout())
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
        finish_job(&st, &jid, outcome).await;
    });

    Json(json!({ "jobId": job_id }))
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
        st.update_job(&jid, |j| j.stage = Some("queued".into())).await;
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
            let size = tokio::fs::metadata(&output_path).await.map(|m| m.len()).ok();
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
        finish_job(&st, &jid, outcome).await;
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
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "job not found" }))),
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
/// `Ok(Some)` -> done, `Ok(None)` -> cancelled, `Err` -> error.
async fn finish_job(st: &AppState, jid: &str, outcome: anyhow::Result<Option<Value>>) {
    match outcome {
        Ok(Some(info)) => {
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
