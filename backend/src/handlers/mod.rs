use axum::extract::State;
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;

use crate::config::resource_classes::ResourceClass;
use crate::jobs::{JobEvent, JobPermit, JobService};
use crate::state::AppState;

mod composition;
mod edit;
mod http_helpers;
mod import;
mod jobs;
mod library;
mod luts;
mod project_archive;
mod proxy;
mod publish;
mod render_cache;
mod stock;
mod upload;

pub use composition::composition_render_handler;

pub use edit::edit_handler;

pub use http_helpers::{api_not_found_handler, method_not_allowed_handler};
pub use import::import_handler;

pub use jobs::{
    cancel_handler, discard_job_handler, failed_jobs_handler, job_registry_handler,
    job_status_handler, resume_pending_jobs, retry_job_handler, start_job_dispatcher,
};

pub use library::{
    library_delete_handler, library_filmstrip_handler, library_filmstrip_version_handler,
    library_list_handler, library_metadata_patch_handler, library_metadata_put_handler,
    library_search_handler, library_thumbnail_handler, library_thumbnail_version_handler,
    source_file_handler,
};
pub use luts::{lut_get_handler, lut_list_handler, lut_upload_handler, MAX_LUT_BODY_BYTES};
pub use project_archive::{
    composition_project_archive_export_handler, composition_project_archive_import_handler,
};
pub use proxy::{
    proxy_content_handler, proxy_create_handler, proxy_delete_handler, proxy_list_handler,
};

pub use publish::{
    youtube_callback_handler, youtube_connect_handler, youtube_disconnect_handler,
    youtube_publish_handler, youtube_status_handler,
};
pub use render_cache::{
    plan_render_cache_key, render_cache_key, render_cache_key_for_tools,
    render_runtime_fingerprint, RENDER_CACHE_PIPELINE_VERSION,
};
pub use stock::stock_search_handler;
pub use upload::upload_handler;

pub async fn metrics_handler(State(state): State<AppState>) -> Response {
    match state.prometheus_metrics() {
        Some(body) => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(
                    "application/openmetrics-text; version=1.0.0; charset=utf-8",
                ),
            )],
            body,
        )
            .into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

/// Drain numeric progress updates from a worker into the job record.
pub(super) fn spawn_progress_drain(
    st: AppState,
    jid: String,
    rx: mpsc::UnboundedReceiver<f64>,
) -> tokio::task::JoinHandle<()> {
    JobService::spawn_progress_drain(st, jid, rx)
}

pub(super) async fn mark_running(st: &AppState, jid: &str, stage: &str, attempt: u32) -> bool {
    JobService::mark_running(st, jid, stage, attempt).await
}

pub(super) async fn mark_queued(st: &AppState, jid: &str) -> bool {
    JobService::mark_queued(st, jid).await
}

pub(super) async fn apply_job_event(st: &AppState, jid: &str, event: JobEvent) -> bool {
    JobService::transition(st, jid, event).await
}

pub(super) async fn finish_from_render_cache(
    st: &AppState,
    jid: &str,
    cache_key: &str,
    actor: Option<&str>,
) -> bool {
    JobService::finish_from_render_cache(st, jid, cache_key, actor).await
}

pub(super) async fn acquire_render_lock_or_cancelled(
    st: &AppState,
    jid: &str,
    token: &CancellationToken,
    lock: Arc<Mutex<()>>,
) -> Option<OwnedMutexGuard<()>> {
    JobService::acquire_render_lock_or_cancelled(st, jid, token, lock).await
}

pub(super) async fn acquire_job_permit_or_cancelled(
    st: &AppState,
    jid: &str,
    token: &CancellationToken,
    class: ResourceClass,
) -> Option<JobPermit> {
    JobService::acquire_job_permit_or_cancelled(st, jid, token, class).await
}

pub(super) async fn acquire_render_permit_or_cancelled(
    st: &AppState,
    jid: &str,
    token: &CancellationToken,
) -> Option<JobPermit> {
    JobService::acquire_render_permit_or_cancelled(st, jid, token).await
}

pub(super) async fn mark_cancelled(st: &AppState, jid: &str) {
    JobService::mark_cancelled(st, jid).await
}

/// Apply the terminal outcome of a worker to the job and clear its cancel token.
pub(super) async fn finish_job(
    st: &AppState,
    jid: &str,
    outcome: anyhow::Result<Option<Value>>,
    kind: &str,
) -> bool {
    JobService::finish(st, jid, outcome, kind).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::jobs::{EnqueueOutcome, JobKind};
    use crate::library::Library;
    use crate::model::{Job, JobStatus};
    use crate::state::{CancelJobOutcome, ToolInfo};
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
        let job = st.get_job("j1").await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.result.is_none());
        assert!(st.library.list().await.is_empty());
    }

    #[tokio::test]
    async fn shutdown_cancellation_leaves_durable_work_recoverable() {
        let (st, _dir) = state().await;
        let outcome = st
            .enqueue_job(
                "restartable".into(),
                JobKind::Import,
                &json!({"schemaVersion": 1}),
                "restartable-key",
            )
            .await
            .unwrap();
        assert!(matches!(outcome, EnqueueOutcome::Created(_)));

        st.begin_shutdown();
        mark_cancelled(&st, "restartable").await;
        assert!(!finish_job(&st, "restartable", Ok(None), "source").await);
        assert_eq!(
            st.get_job("restartable").await.unwrap().unwrap().status,
            JobStatus::Pending
        );
        assert_eq!(
            st.job_store.deliverable_ids().await.unwrap(),
            vec!["restartable"]
        );
    }

    #[tokio::test]
    async fn mark_running_does_not_revive_a_cancelled_job() {
        let (st, _dir) = state().await;
        st.set_job(Job::pending("cancelled-before-start".into()))
            .await;
        assert_eq!(
            st.cancel_open_job("cancelled-before-start").await.unwrap(),
            CancelJobOutcome::Cancelled
        );

        assert!(!mark_running(&st, "cancelled-before-start", "processing", 1).await);
        assert!(!mark_queued(&st, "cancelled-before-start").await);
        let job = st.get_job("cancelled-before-start").await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.stage.is_none());
        assert!(job.progress.is_none());
    }

    #[test]
    fn cache_filenames_must_be_plain_output_names() {
        assert!(crate::jobs::is_plain_filename("output.mp4"));
        assert!(crate::jobs::is_plain_filename("b5b2b5b2-clip.webm"));

        for filename in ["", ".", "..", "../x.mp4", "dir/x.mp4", "dir\\x.mp4"] {
            assert!(
                !crate::jobs::is_plain_filename(filename),
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

        assert!(finish_from_render_cache(&st, "valid", "valid-key", Some("publisher")).await);
        assert!(st.db.can_access_output("out", "publisher").await.unwrap());
        let job = st.get_job("valid").await.unwrap().unwrap();
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
        assert!(!finish_from_render_cache(&st, "unsafe", "unsafe-key", None).await);
        assert!(st.db.cache_get("unsafe-key").await.unwrap().is_none());
        assert_eq!(
            st.get_job("unsafe").await.unwrap().unwrap().status,
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
        assert!(!finish_from_render_cache(&st, "missing", "missing-key", None).await);
        assert!(st.db.cache_get("missing-key").await.unwrap().is_none());
        assert_eq!(
            st.get_job("missing").await.unwrap().unwrap().status,
            JobStatus::Pending
        );
    }
}
