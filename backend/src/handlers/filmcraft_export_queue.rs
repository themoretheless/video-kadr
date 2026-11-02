//! Owner/server-session queue. Ready payloads share an immutable project and
//! captured source identity; only one item creates an execution job at a time.
use super::filmcraft_video_jobs::{enqueue_prepared, prepare, PreparedExport};
use crate::{
    domain::{
        filmcraft_export_pace::{format_eta, Pace},
        filmcraft_export_queue::{Outcome, Queue, Status},
        filmcraft_export_range::SourceRange,
        filmcraft_project_file::SourceProjectDocument,
        filmcraft_project_index::ProjectIndex,
    },
    error::{ApiJson, AppError, AppResult},
    model::JobStatus,
    state::AppState,
};
use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::Mutex;

pub(crate) type Stores = Arc<Mutex<HashMap<String, Arc<Mutex<Session>>>>>;
struct Payload {
    prepared: PreparedExport,
    document: Arc<Value>,
    sequence_name: String,
    preset_name: String,
}
pub(crate) struct Session {
    queue: Queue<Payload>,
    observer: bool,
    managed: HashMap<String, u64>,
    cancel_waiters: Arc<AtomicUsize>,
    paces: HashMap<String, Pace>,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            queue: Queue::default(),
            observer: false,
            managed: HashMap::new(),
            cancel_waiters: Arc::new(AtomicUsize::new(0)),
            paces: HashMap::new(),
        }
    }
}

async fn store(state: &AppState, owner: &str) -> Arc<Mutex<Session>> {
    state
        .filmcraft_export_queues
        .lock()
        .await
        .entry(owner.into())
        .or_insert_with(|| Arc::new(Mutex::new(Session::default())))
        .clone()
}
fn bad(error: impl std::fmt::Display) -> AppError {
    AppError::bad_request(error.to_string())
}
fn id(value: &str) -> AppResult<u64> {
    value.parse().map_err(bad)
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum Range {
    Named(String),
    Exact(SourceRange),
    Source(crate::domain::filmcraft_export_command_range::CommandRange),
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum Command {
    Add {
        input: Value,
        sequences: Option<Vec<String>>,
        ranges: Option<Vec<Range>>,
        #[serde(default)]
        start: bool,
        #[serde(default)]
        wait: bool,
    },
    Start {
        #[serde(default)]
        wait: bool,
    },
    Stop {},
    Cancel {
        id: Option<String>,
        #[serde(default)]
        wait: bool,
    },
    Retry {
        id: String,
        #[serde(default)]
        start: bool,
        #[serde(default)]
        wait: bool,
    },
    Remove {
        id: String,
    },
    Move {
        id: String,
        to: Option<i64>,
        by: Option<i64>,
    },
    Clear {
        #[serde(default)]
        all: bool,
    },
}

async fn view(state: &AppState, session: &mut Session) -> Value {
    let mut items = Vec::new();
    session.paces.retain(|job_id, _| {
        session
            .queue
            .items()
            .iter()
            .any(|item| item.status == Status::Encoding && item.job.as_ref() == Some(job_id))
    });
    for item in session.queue.items() {
        let job = if let Some(job) = &item.job {
            state.get_job(job).await.ok().flatten()
        } else {
            None
        };
        let progress = job
            .as_ref()
            .and_then(|job| job.progress)
            .map(|percent| percent / 100.0);
        let eta = if item.status == Status::Encoding {
            if let Some(job) = &job {
                let finished = matches!(
                    job.status,
                    JobStatus::Done
                        | JobStatus::Error
                        | JobStatus::Cancelled
                        | JobStatus::Interrupted
                );
                session
                    .paces
                    .entry(job.id.clone())
                    .or_default()
                    .fraction_eta(std::time::Instant::now(), progress, finished)
            } else {
                None
            }
        } else {
            None
        };
        items.push(json!({"id":item.id.to_string(),"sequenceId":item.payload.prepared.request.sequence_id,"sequenceName":item.payload.sequence_name,"preset":item.payload.preset_name,"container":item.payload.prepared.request.extension(),"status":item.status,"jobId":item.job,"error":item.error,"progress":progress.unwrap_or(if item.status == Status::Done { 1.0 } else { 0.0 }),"statusText":job.as_ref().and_then(|job|job.stage.as_ref()),"etaSeconds":eta.map(|eta|eta.as_secs_f64()),"etaText":eta.map(format_eta),"sourceRange":item.payload.prepared.request.source_range}));
    }
    json!({"running":session.queue.running(),"items":items})
}

pub async fn list(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let store = store(&state, &owner).await;
    let mut session = store.lock().await;
    Ok(Json(view(&state, &mut session).await))
}

async fn admission(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    mut input: Value,
    sequences: Option<Vec<String>>,
    ranges: Option<Vec<Range>>,
) -> AppResult<Vec<Payload>> {
    let object = input
        .as_object_mut()
        .ok_or_else(|| bad("queue input must be an object"))?;
    let document = Arc::new(
        object
            .remove("document")
            .ok_or_else(|| bad("queue document is required"))?,
    );
    let bytes = match document.as_str() {
        Some(text) => text.as_bytes().to_vec(),
        None => serde_json::to_vec(document.as_ref()).map_err(bad)?,
    };
    let parsed = SourceProjectDocument::read(&bytes).map_err(bad)?;
    let index = ProjectIndex::new(&parsed).map_err(bad)?;
    let sequences = match sequences {
        Some(sequences) => sequences,
        None => vec![input["sequenceId"]
            .as_str()
            .ok_or_else(|| bad("sequenceId is required"))?
            .to_owned()],
    };
    if sequences.is_empty() {
        return Err(bad("no sequences"));
    }
    let mut preset_name = input["preset"].as_str().unwrap_or("").to_owned();
    let mut base = if let Some(name) = input
        .as_object_mut()
        .and_then(|input| input.remove("libraryPreset"))
    {
        let name = name
            .as_str()
            .ok_or_else(|| bad("libraryPreset must be a name"))?;
        let library = state
            .db
            .filmcraft_export_presets(owner)
            .await
            .map_err(|error| AppError::internal("read queued presets", error))?;
        let preset = library
            .find(name)
            .ok_or_else(|| bad("unknown export preset"))?;
        preset_name = preset.name;
        crate::domain::filmcraft_export_preset_resolve::request_patch(&preset.settings)
            .map_err(bad)?
    } else if input.get("preset").is_none() {
        let settings = serde_json::to_value(
            crate::domain::filmcraft_export_preset_settings::ExportSettings::default(),
        )
        .map_err(bad)?;
        crate::domain::filmcraft_export_preset_resolve::request_patch(&settings).map_err(bad)?
    } else {
        json!({})
    };
    for (key, value) in input
        .as_object()
        .ok_or_else(|| bad("invalid queue input"))?
    {
        base[key] = value.clone();
    }
    let ranges = ranges.unwrap_or_default();
    let count = sequences
        .len()
        .checked_mul(ranges.len().max(1))
        .ok_or_else(|| bad("queue batch overflow"))?;
    let mut payloads = Vec::new();
    for sequence_id in sequences {
        let sequence = id(&sequence_id)?;
        let raw = index
            .item(sequence)
            .filter(|item| item.pointer("/kind/Sequence").is_some())
            .ok_or_else(|| bad("queue item is not a sequence"))?;
        let sequence_name = raw["name"].as_str().unwrap_or("").to_owned();
        for range_index in 0..ranges.len().max(1) {
            let mut body = base.clone();
            body["document"] = document.as_ref().clone();
            body["sequenceId"] = json!(sequence_id);
            let range = match ranges.get(range_index) {
                Some(Range::Exact(range)) => {
                    range.bounds().map_err(bad)?;
                    Some(range.clone())
                }
                Some(Range::Source(range)) => Some(range.resolve(&parsed, sequence).map_err(bad)?),
                Some(Range::Named(mode)) => Some(
                    crate::domain::filmcraft_quick_export_range::resolve(
                        &parsed,
                        sequence,
                        Some(mode),
                        None,
                    )
                    .map_err(bad)?,
                ),
                None if body.get("sourceRange").is_none()
                    && body.get("frames").is_none()
                    && body.get("startTicks").is_none()
                    && body.get("startFrames").is_none() =>
                {
                    Some(
                        crate::domain::filmcraft_quick_export_range::resolve(
                            &parsed, sequence, None, None,
                        )
                        .map_err(bad)?,
                    )
                }
                None => None,
            };
            if let Some(range) = range {
                let object = body
                    .as_object_mut()
                    .ok_or_else(|| bad("invalid queue body"))?;
                for key in ["frames", "startTicks", "startFrames"] {
                    object.remove(key);
                }
                object.insert("sourceRange".into(), json!(range));
            }
            let mut prepared = prepare(state, headers, owner, body).await?;
            prepared.request.document = Value::Null;
            payloads.push(Payload {
                prepared,
                document: document.clone(),
                sequence_name: sequence_name.clone(),
                preset_name: preset_name.clone(),
            });
        }
    }
    debug_assert_eq!(payloads.len(), count);
    Ok(payloads)
}

// A cancelled HTTP request must release its pause without an async lock.
struct CancelWait(Arc<AtomicUsize>);
impl CancelWait {
    fn new(counter: Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::AcqRel);
        Self(counter)
    }
}
impl Drop for CancelWait {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
#[derive(Clone, Copy)]
enum Wait {
    None,
    Drain,
    Cancel,
}

pub async fn command(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(command): ApiJson<Command>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let mut wait = match &command {
        Command::Add {
            start: true,
            wait: true,
            ..
        }
        | Command::Start { wait: true }
        | Command::Retry {
            start: true,
            wait: true,
            ..
        } => Wait::Drain,
        Command::Cancel { wait: true, .. } => Wait::Cancel,
        _ => Wait::None,
    };
    // Complete admission before mutating the queue; no jobs are created here.
    let (admission, control) = match command {
        Command::Add {
            input,
            sequences,
            ranges,
            start,
            ..
        } => {
            let payloads = admission(&state, &headers, &owner, input, sequences, ranges).await?;
            (Some((payloads, start)), None)
        }
        command => (None, Some(command)),
    };
    let store = store(&state, &owner).await;
    let mut session = store.lock().await;
    // Source pump_queue(wait) never blocks on an encode that was already
    // running asynchronously; only work started by this pump is synchronous.
    if matches!(wait, Wait::Drain)
        && session
            .queue
            .items()
            .iter()
            .any(|item| item.status == Status::Encoding)
    {
        wait = Wait::None;
    }
    let pause =
        matches!(wait, Wait::Cancel).then(|| CancelWait::new(session.cancel_waiters.clone()));
    let mut added = Vec::new();
    let mut cancel = Vec::new();
    if let Some((payloads, start)) = admission {
        added = session.queue.add(payloads).map_err(bad)?;
        if start {
            session.queue.start().map_err(bad)?;
        }
    } else if let Some(command) = control {
        match command {
            Command::Start { .. } => session.queue.start().map_err(bad)?,
            Command::Stop {} => session.queue.stop(),
            Command::Cancel { id: target, .. } => {
                cancel = session
                    .queue
                    .cancel(target.as_deref().map(id).transpose()?)
                    .map_err(bad)?;
            }
            Command::Retry {
                id: target, start, ..
            } => session.queue.retry(id(&target)?, start).map_err(bad)?,
            Command::Remove { id: target } => {
                session.queue.remove(id(&target)?).map_err(bad)?;
            }
            Command::Move { id: target, to, by } => {
                session.queue.move_item(id(&target)?, to, by).map_err(bad)?
            }
            Command::Clear { all } => session.queue.clear(all).map_err(bad)?,
            Command::Add { .. } => return Err(bad("invalid queue control")),
        }
    }
    let remaining = session
        .queue
        .items()
        .iter()
        .map(|item| item.id)
        .collect::<std::collections::HashSet<_>>();
    session.managed.retain(|_, item| remaining.contains(item));
    for job in cancel {
        state
            .cancel_open_job(&job)
            .await
            .map_err(|error| AppError::internal("cancel queued export", error))?;
    }
    drop(session);
    ensure_observer(&state, owner, store.clone()).await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let mut session = store.lock().await;
        let pending = match wait {
            Wait::None => false,
            Wait::Drain => session.queue.is_active(),
            Wait::Cancel => {
                session
                    .queue
                    .items()
                    .iter()
                    .any(|item| item.status == Status::Encoding)
                    && tokio::time::Instant::now() < deadline
            }
        };
        if !pending || state.is_shutting_down() {
            let mut result = view(&state, &mut session).await;
            if !added.is_empty() {
                result["added"] = json!(added.iter().map(u64::to_string).collect::<Vec<_>>());
            }
            drop(session);
            drop(pause);
            return Ok(Json(result));
        }
        drop(session);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn ensure_observer(state: &AppState, owner: String, store: Arc<Mutex<Session>>) {
    let mut session = store.lock().await;
    if session.observer || !session.queue.is_active() {
        return;
    }
    session.observer = true;
    drop(session);
    let worker = state.clone();
    state.spawn_task(async move {
        loop {
            let mut session = store.lock().await;
            if worker.is_shutting_down() {
                session.observer = false;
                return;
            }
            if let Some((item_id, job_id)) = session
                .queue
                .items()
                .iter()
                .find(|item| item.status == Status::Encoding)
                .and_then(|item| item.job.as_ref().map(|job| (item.id, job.clone())))
            {
                if let Ok(Some(job)) = worker.get_job(&job_id).await {
                    let outcome = match job.status {
                        JobStatus::Done => Some(Outcome::Done),
                        JobStatus::Cancelled => Some(Outcome::Cancelled),
                        JobStatus::Error | JobStatus::Interrupted => Some(Outcome::Failed(
                            job.error.unwrap_or_else(|| "export failed".into()),
                        )),
                        _ => None,
                    };
                    if let Some(outcome) = outcome
                        .filter(|_| !super::filmcraft_video_jobs::worker_active(&worker, &job_id))
                    {
                        if let Err(error) = session.queue.settle(item_id, outcome) {
                            tracing::error!(%error,"queue settlement failed");
                            session.observer = false;
                            return;
                        }
                    }
                }
            }
            let next = if session.cancel_waiters.load(Ordering::Acquire) == 0 {
                session.queue.next_ready()
            } else {
                None
            }
            .map(|item| {
                let mut prepared = item.payload.prepared.clone();
                prepared.request.document = item.payload.document.as_ref().clone();
                (item.id, prepared)
            });
            if let Some((item_id, prepared)) = next {
                match enqueue_prepared(&worker, &owner, prepared).await {
                    Ok(accepted) => {
                        let job = accepted.job_id;
                        if let Err(error) = session.queue.begin(item_id, job.clone()) {
                            let _ = worker.cancel_open_job(&job).await;
                            tracing::error!(%error,"queue start invariant failed");
                            session.observer = false;
                            return;
                        }
                        session.managed.insert(job, item_id);
                    }
                    Err(error) => {
                        if let Err(invariant) = session
                            .queue
                            .fail_ready(item_id, error.public_message().to_owned())
                        {
                            tracing::error!(%invariant,"queue admission invariant failed");
                            session.observer = false;
                            return;
                        }
                    }
                }
            }
            if !session.queue.is_active() {
                session.observer = false;
                return;
            }
            drop(session);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
}

/// Retrying a managed job directly would bypass queue ordering. Queue retry
/// creates a fresh job from the item's captured payload instead.
pub(super) async fn authorize_external_retry(
    state: &AppState,
    job: &str,
    headers: &HeaderMap,
) -> AppResult<()> {
    let job_owner: Option<String> =
        sqlx::query_scalar("SELECT owner FROM filmcraft_video_job_access WHERE job_id=?")
            .bind(job)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|error| AppError::internal("resolve queued retry", error))?;
    let Some(job_owner) = job_owner else {
        return Ok(());
    };
    let owner = super::publish::authenticated_actor(state, headers).await?;
    if owner != job_owner {
        return Err(AppError::forbidden("Это задание другого пользователя"));
    }
    let store = state
        .filmcraft_export_queues
        .lock()
        .await
        .get(&owner)
        .cloned();
    if let Some(store) = store {
        if store.lock().await.managed.contains_key(job) {
            return Err(bad(
                "Повторите элемент через очередь или сначала удалите его из очереди",
            ));
        }
    }
    Ok(())
}
