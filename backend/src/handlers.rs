use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::model::{EditRequest, ImportRequest, Job, JobStatus};
use crate::state::AppState;
use crate::tools;

/// `POST /api/import` — accept a video URL, kick off a download in the background,
/// and immediately return a job id to poll.
pub async fn import_handler(
    State(state): State<AppState>,
    Json(req): Json<ImportRequest>,
) -> Json<Value> {
    let job_id = Uuid::new_v4().to_string();
    let video_id = Uuid::new_v4().to_string();
    state.set_job(Job::pending(job_id.clone())).await;

    let st = state.clone();
    let url = req.url;
    let start = req.start;
    let end = req.end;
    let jid = job_id.clone();
    let vid = video_id;
    tokio::spawn(async move {
        st.update_job(&jid, |j| j.status = JobStatus::Running).await;

        let sources = st.sources_dir();
        let outcome = async {
            let path = tools::download_video(&url, &sources, &vid, start, end).await?;
            let info = tools::probe_video(&path).await?;
            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("{vid}.mp4"));
            Ok::<Value, anyhow::Error>(json!({
                "id": vid,
                "url": format!("/files/sources/{filename}"),
                "filename": filename,
                "duration": info.duration,
                "width": info.width,
                "height": info.height,
            }))
        }
        .await;

        match outcome {
            Ok(info) => {
                st.update_job(&jid, |j| {
                    j.status = JobStatus::Done;
                    j.result = Some(info);
                })
                .await
            }
            Err(e) => {
                st.update_job(&jid, |j| {
                    j.status = JobStatus::Error;
                    j.error = Some(e.to_string());
                })
                .await
            }
        }
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

    let st = state.clone();
    let jid = job_id.clone();
    tokio::spawn(async move {
        st.update_job(&jid, |j| j.status = JobStatus::Running).await;

        let sources = st.sources_dir();
        let outputs = st.outputs_dir();
        let output_path = outputs.join(format!("{out_id}.mp4"));

        let outcome = async {
            let input = tools::find_source(&sources, &req.video_id).await?;
            let args = tools::build_ffmpeg_args(&input, &output_path, &req);
            tracing::info!("ffmpeg {}", args.join(" "));
            tools::run_ffmpeg(&args).await?;
            Ok::<Value, anyhow::Error>(json!({
                "id": out_id,
                "url": format!("/files/outputs/{out_id}.mp4"),
                "filename": format!("{out_id}.mp4"),
            }))
        }
        .await;

        match outcome {
            Ok(info) => {
                st.update_job(&jid, |j| {
                    j.status = JobStatus::Done;
                    j.result = Some(info);
                })
                .await
            }
            Err(e) => {
                st.update_job(&jid, |j| {
                    j.status = JobStatus::Error;
                    j.error = Some(e.to_string());
                })
                .await
            }
        }
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
