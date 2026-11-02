//! Owner/server-session mask tracking with real sequential work and retained partial snapshots.
use crate::{
    domain::{
        filmcraft_mask_track_batch::{Plan, Runner},
        filmcraft_project_file::SourceProjectDocument,
        filmcraft_project_index::ProjectIndex,
        group_surface::GroupSurface,
    },
    error::{AppError, AppResult},
    services::filmcraft_video_preview::{self, SourceVideoRevision},
    state::AppState,
};
use axum::{
    extract::{Path as AxPath, Query, State},
    http::HeaderMap,
};
use futures_util::FutureExt;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
pub(crate) type Stores = Arc<Mutex<Store>>;
#[derive(Default)]
pub(crate) struct Store {
    owners: HashMap<String, Session>,
    next: u64,
}
#[derive(Default)]
struct Session {
    jobs: BTreeMap<u64, Arc<Mutex<Job>>>,
}
struct Job {
    id: u64,
    instance: String,
    target: (u64, u64, usize, usize),
    label: String,
    total: usize,
    done: usize,
    inliers: usize,
    finished: bool,
    error: Option<String>,
    stopped: bool,
    cancel: CancellationToken,
    snapshot: Arc<str>,
    sources: Vec<String>,
    proof: Vec<SourceProof>,
    catalog: Option<(String, [u8; 32])>,
    started: Instant,
    seconds: f64,
}
struct SourceProof {
    id: String,
    kind: Option<String>,
    file: Option<(PathBuf, SourceVideoRevision)>,
}
async fn capture_proof(state: &AppState, ids: &[String]) -> AppResult<Vec<SourceProof>> {
    let mut proof = Vec::new();
    for id in ids {
        let entry = state
            .library
            .get(id)
            .await
            .ok_or_else(|| bad("tracking binding disappeared"))?;
        let file = match state.library.resolve_media_path(&entry).await {
            Ok(path) => {
                let revision = SourceVideoRevision::read(&path).await.map_err(bad)?;
                Some((path, revision))
            }
            Err(_) => None,
        };
        proof.push(SourceProof {
            id: id.clone(),
            kind: entry.media_type,
            file,
        });
    }
    Ok(proof)
}
async fn verify_proof(state: &AppState, proof: &[SourceProof]) -> AppResult<()> {
    for old in proof {
        let entry = state
            .library
            .get(&old.id)
            .await
            .ok_or_else(|| bad("tracking binding disappeared"))?;
        if entry.media_type != old.kind {
            return Err(bad("tracking binding changed"));
        }
        match (&old.file, state.library.resolve_media_path(&entry).await) {
            (Some((path, revision)), Ok(current)) if current == *path => {
                if SourceVideoRevision::read(&current).await.map_err(bad)? != *revision {
                    return Err(bad("tracking source changed since admission"));
                }
            }
            (None, Err(_)) => {}
            _ => return Err(bad("tracking source availability changed")),
        }
    }
    Ok(())
}
async fn verify_snapshot(state: &AppState, headers: &HeaderMap, job: &Job) -> AppResult<()> {
    verify_proof(state, &job.proof).await?;
    if let Some((id, hash)) = &job.catalog {
        let first = state
            .library
            .get(id)
            .await
            .ok_or_else(|| bad("tracking sequence binding disappeared"))?;
        let mut reader = super::filmcraft_image_sequence::catalog(
            state,
            headers,
            None,
            &first,
            state.shutdown_token(),
        )
        .await?;
        if reader.snapshot().await?.0 != *hash {
            return Err(bad("tracking sequence catalog changed since admission"));
        }
    }
    Ok(())
}
impl Job {
    fn view(&self, snapshot: bool) -> Value {
        let seconds = if self.finished {
            self.seconds
        } else {
            self.started.elapsed().as_secs_f64()
        };
        let result = if !self.finished {
            Value::Null
        } else if let Some(e) = &self.error {
            json!({"error":e})
        } else {
            json!({"path":"","frames":self.done,"seconds":seconds,"bytes":0,"render_fps":self.done as f64/seconds.max(1e-6)})
        };
        let status = if let Some(e) = &self.error {
            format!("{e} ({} frame(s) tracked)", self.done)
        } else if self.stopped {
            format!("stopped ({} frame(s) tracked)", self.done)
        } else if self.finished {
            format!("Tracked {} frame(s) in {:.1}s", self.done, seconds)
        } else {
            format!(
                "Frame {} of {} · {} features",
                self.done, self.total, self.inliers
            )
        };
        let mut v = json!({"id":if self.id>9007199254740991{json!(self.id.to_string())}else{json!(self.id)},"kind":"maskTracking","instance":self.instance,"target":{"sequence":self.target.0.to_string(),"clip":self.target.1.to_string(),"effect":self.target.2,"mask":self.target.3},"label":self.label,"progress":self.done as f64/self.total.max(1) as f64,"etaSeconds":if !self.finished&&self.done>0{Some(seconds/self.done as f64*(self.total-self.done) as f64)}else{None},"done":self.done,"total":self.total,"status":status,"finished":self.finished,"result":result});
        if snapshot {
            v["documentText"] = json!(self.snapshot.as_ref());
        }
        v
    }
}
fn bad(e: impl std::fmt::Display) -> AppError {
    AppError::bad_request(e.to_string())
}
fn num(v: Option<&Value>) -> Option<u64> {
    v.and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}
async fn records(state: &AppState, owner: &str) -> Vec<Arc<Mutex<Job>>> {
    state
        .filmcraft_tracking_jobs
        .lock()
        .await
        .owners
        .get(owner)
        .map(|s| s.jobs.values().cloned().collect())
        .unwrap_or_default()
}
async fn authorize(state: &AppState, headers: &HeaderMap, job: &Job) -> AppResult<()> {
    for id in &job.sources {
        super::library::authorize_source_access(state, id, headers).await?;
    }
    Ok(())
}

pub(super) async fn list(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    p: &Value,
) -> AppResult<Value> {
    let only = num(p.get("job"));
    if p.get("instance").is_some() && only.is_none() {
        return Err(bad("tracking instance requires a job ID"));
    }
    if p["snapshot"] == true && only.is_none() {
        return Err(bad("snapshot requires a tracking job ID"));
    }
    let mut out = Vec::new();
    for cell in records(state, owner).await {
        let job = cell.lock().await;
        if only.is_some_and(|id| id != job.id) {
            continue;
        }
        if p.get("instance")
            .is_some_and(|value| value.as_str() != Some(job.instance.as_str()))
        {
            continue;
        }
        if let Err(error) = authorize(state, headers, &job).await {
            if p["snapshot"] == true {
                return Err(error);
            }
            continue;
        }
        if p["snapshot"] == true {
            verify_snapshot(state, headers, &job).await?;
        }
        out.push(job.view(p["snapshot"] == true));
    }
    // Source export jobs share the listing, retaining the host's opaque export UUIDs.
    if only.is_none() {
        let mut cursor: Option<String> = None;
        loop {
            let query = serde_json::from_value(json!({"limit":50,"cursor":cursor})).map_err(bad)?;
            let page = super::filmcraft_video_jobs::list(
                State(state.clone()),
                headers.clone(),
                Query(query),
            )
            .await?
            .0;
            for row in page["jobs"]
                .as_array()
                .ok_or_else(|| bad("invalid export history"))?
            {
                let j = &row["job"];
                let status = j["status"].as_str().unwrap_or("pending");
                let finished = !matches!(status, "pending" | "running");
                let progress =
                    j["progress"]
                        .as_f64()
                        .unwrap_or(if status == "done" { 100. } else { 0. });
                let result = if let Some(error) = j["error"].as_str() {
                    json!({"error":error})
                } else {
                    j.get("result").cloned().unwrap_or(Value::Null)
                };
                out.push(json!({"id":row["jobId"],"kind":"export","label":"Filmcraft export","progress":progress/100.,"done":progress,"total":100,"etaSeconds":null,"status":status,"finished":finished,"result":result}));
            }
            cursor = page["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
            if out.len() > 10000 {
                return Err(bad("job listing budget exceeded"));
            }
        }
    }
    Ok(json!(out))
}
pub(super) async fn cancel(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    p: &Value,
) -> AppResult<Value> {
    if let Some(id) = num(p.get("job")) {
        let cells = records(state, owner).await;
        for cell in cells {
            let job = cell.lock().await;
            if job.id == id {
                if p.get("instance")
                    .is_some_and(|value| value.as_str() != Some(job.instance.as_str()))
                {
                    return Err(AppError::not_found("tracking job instance not found"));
                }
                if p["discard"] == true {
                    if !job.finished {
                        return Err(AppError::conflict(
                            "cancel and finish the tracking job before discarding it",
                        ));
                    }
                    drop(job);
                    let mut store = state.filmcraft_tracking_jobs.lock().await;
                    if let Some(session) = store.owners.get_mut(owner) {
                        session.jobs.remove(&id);
                    }
                    return Ok(Value::Null);
                }
                job.cancel.cancel();
                return Ok(Value::Null);
            }
        }
        return Err(AppError::not_found("tracking job not found"));
    }
    let id = p["job"].as_str().ok_or_else(|| bad("need job"))?;
    if p["discard"] == true {
        return Err(bad("discard applies only to finished tracking snapshots"));
    }
    uuid::Uuid::parse_str(id).map_err(bad)?;
    // Require actual membership in the owner's Filmcraft export history before generic cancellation.
    let mut cursor = None;
    let mut found = false;
    loop {
        let query = serde_json::from_value(json!({"limit":50,"cursor":cursor})).map_err(bad)?;
        let page =
            super::filmcraft_video_jobs::list(State(state.clone()), headers.clone(), Query(query))
                .await?
                .0;
        for row in page["jobs"]
            .as_array()
            .ok_or_else(|| bad("invalid export history"))?
        {
            if row["jobId"] == id {
                found = true;
                if !matches!(row["job"]["status"].as_str(), Some("pending" | "running")) {
                    return Ok(Value::Null);
                }
                break;
            }
        }
        if found {
            break;
        }
        cursor = page["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    if !found {
        return Err(AppError::not_found("export job not found"));
    }
    let _ = super::jobs::cancel_handler(State(state.clone()), AxPath(id.into()), headers.clone())
        .await?;
    Ok(Value::Null)
}

struct Provider<'a> {
    state: &'a AppState,
    headers: &'a HeaderMap,
    document: Arc<SourceProjectDocument>,
    source: u64,
    scale: f32,
    file: Option<(String, PathBuf, SourceVideoRevision, Option<String>)>,
    sequence: Option<(
        super::filmcraft_image_sequence::Reader<'a>,
        filmcraft_time::FrameRate,
    )>,
    still: Option<Arc<GroupSurface>>,
    sequence_id: Option<String>,
}
impl<'a> Provider<'a> {
    async fn open(
        state: &'a AppState,
        headers: &'a HeaderMap,
        plan: &Plan,
        sources: &BTreeMap<String, String>,
        decode_seed: bool,
    ) -> AppResult<Self> {
        let document = Arc::new(plan.document.clone());
        let index = ProjectIndex::new(&document).map_err(bad)?;
        let media = index
            .resolve_media(plan.source)
            .ok_or_else(|| bad("tracking source is not media"))?;
        let source = media.item_id;
        let generated = media.media.pointer("/media/Generator").is_some();
        let mut file = None;
        let mut sequence = None;
        let mut sequence_id = None;
        if !generated {
            let id = sources
                .get(&source.to_string())
                .ok_or_else(|| bad("tracking source is not bound to the library"))?;
            super::library::authorize_source_access(state, id, headers).await?;
            let entry = state
                .library
                .get(id)
                .await
                .filter(|e| e.kind == "source")
                .ok_or_else(|| AppError::not_found("tracking source not found"))?;
            if media.media["info"]["kind"] == "ImageSequence" {
                let rate = media
                    .media
                    .pointer("/interpret/frame_rate")
                    .filter(|v| v.is_object())
                    .or_else(|| media.media.pointer("/info/video/frame_rate"))
                    .map(|v| serde_json::from_value(v.clone()))
                    .transpose()
                    .map_err(bad)?
                    .unwrap_or_default();
                let mut reader = if decode_seed {
                    super::filmcraft_image_sequence::open(
                        state,
                        headers,
                        None,
                        &entry,
                        state.shutdown_token(),
                    )
                    .await?
                } else {
                    super::filmcraft_image_sequence::catalog(
                        state,
                        headers,
                        None,
                        &entry,
                        state.shutdown_token(),
                    )
                    .await?
                };
                reader.snapshot().await?;
                sequence = Some((reader, rate));
                sequence_id = Some(id.clone());
            } else {
                let path = state
                    .library
                    .resolve_media_path(&entry)
                    .await
                    .map_err(bad)?;
                let revision = SourceVideoRevision::read(&path).await.map_err(bad)?;
                file = Some((id.clone(), path, revision, entry.media_type));
            }
        }
        Ok(Self {
            state,
            headers,
            document,
            source,
            scale: (960. / plan.width.max(1) as f64).min(1.) as f32,
            file,
            sequence,
            still: None,
            sequence_id,
        })
    }
    async fn fingerprint(&mut self) -> AppResult<Vec<u8>> {
        self.verify().await?;
        if let Some((reader, _)) = &mut self.sequence {
            return Ok(reader.snapshot().await?.0.to_vec());
        }
        if let Some((id, path, revision, kind)) = &self.file {
            let mut bytes = revision.fingerprint();
            bytes.extend(id.as_bytes());
            bytes.extend(path.as_os_str().as_encoded_bytes());
            bytes.extend(kind.as_deref().unwrap_or("").as_bytes());
            return Ok(bytes);
        }
        Ok(Vec::new())
    }
    fn access_ids(&self) -> Vec<String> {
        if let Some((reader, _)) = &self.sequence {
            reader.member_ids().map(str::to_owned).collect()
        } else {
            self.file
                .as_ref()
                .map(|(id, _, _, _)| vec![id.clone()])
                .unwrap_or_default()
        }
    }
    async fn verify(&mut self) -> AppResult<()> {
        if let Some((reader, _)) = &mut self.sequence {
            reader.snapshot().await?;
            reader.verify().await?;
        }
        if let Some((id, path, revision, kind)) = &self.file {
            super::library::authorize_source_access(self.state, id, self.headers).await?;
            let entry = self
                .state
                .library
                .get(id)
                .await
                .ok_or_else(|| bad("tracking binding disappeared"))?;
            if self
                .state
                .library
                .resolve_media_path(&entry)
                .await
                .map_err(bad)?
                != *path
                || entry.media_type != *kind
                || SourceVideoRevision::read(path).await.map_err(bad)? != *revision
            {
                return Err(bad("tracking source changed before publication"));
            }
        }
        Ok(())
    }
    async fn frame(&mut self, t: i64) -> AppResult<Arc<GroupSurface>> {
        self.verify().await?;
        let output = if let Some((reader, rate)) = &mut self.sequence {
            reader.frame(*rate, t).await?
        } else if let Some((_, path, revision, kind)) = &self.file {
            if kind.as_deref() == Some("image") {
                if let Some(image) = &self.still {
                    image.clone()
                } else {
                    let path = path.clone();
                    let image = self
                        .state
                        .cpu_pool
                        .execute(self.state.shutdown_token(), move |_| {
                            anyhow::ensure!(
                                std::fs::metadata(&path)?.len() <= 64 * 1024 * 1024,
                                "tracking image budget exceeded"
                            );
                            crate::domain::group_media::decode_image_surface(&path).map(Arc::new)
                        })
                        .await
                        .map_err(bad)?;
                    self.still = Some(image.clone());
                    image
                }
            } else {
                let frame = filmcraft_video_preview::frame_tracking(
                    &self.state.filmcraft_video_previews,
                    &self.state.process_runtime,
                    path,
                    t,
                )
                .await
                .map_err(bad)?
                .ok_or_else(|| bad("tracking frame unavailable"))?;
                if frame.revision() != *revision {
                    return Err(bad("tracking source changed during decoding"));
                }
                frame.surface.clone()
            }
        } else {
            let doc = self.document.clone();
            let (source, scale) = (self.source, self.scale);
            Arc::new(
                self.state
                    .cpu_pool
                    .execute(self.state.shutdown_token(), move |cancel| {
                        crate::domain::filmcraft_generator_video::source_frame(
                            &doc,
                            source,
                            t,
                            scale,
                            || cancel.is_cancelled(),
                        )
                        .map(|f| f.surface)
                    })
                    .await
                    .map_err(bad)?,
            )
        };
        self.verify().await?;
        Ok(output)
    }
}

async fn worker(
    state: &AppState,
    headers: &HeaderMap,
    cell: &Arc<Mutex<Job>>,
    plan: Plan,
    sources: BTreeMap<String, String>,
    expected: Vec<u8>,
) -> AppResult<()> {
    let mut provider = Provider::open(state, headers, &plan, &sources, true).await?;
    if provider.fingerprint().await? != expected {
        return Err(bad("tracking source changed after admission"));
    }
    let frame = provider.frame(plan.from).await?;
    let (mut runner, text) = state
        .cpu_pool
        .execute(state.shutdown_token(), move |_| {
            let runner = Runner::start(plan, &frame)?;
            let text = String::from_utf8(runner.document.encode_preserved()?)?;
            anyhow::ensure!(
                text.len() <= 32 * 1024 * 1024,
                "tracking snapshot budget exceeded"
            );
            Ok((runner, text))
        })
        .await
        .map_err(bad)?;
    provider.verify().await?;
    cell.lock().await.snapshot = Arc::from(text);
    loop {
        if runner.done >= runner.plan.steps {
            break;
        }
        let cancelled = cell.lock().await.cancel.is_cancelled();
        if cancelled || state.is_shutting_down() {
            cell.lock().await.stopped = true;
            break;
        }
        let step = runner.done + 1;
        let t = runner.plan.time(step).map_err(bad)?;
        let frame = match provider.frame(t).await {
            Ok(frame) => frame,
            Err(e) => {
                cell.lock().await.error =
                    Some(format!("can't decode frame {step}: {}", e.public_message()));
                break;
            }
        };
        let (next, result, text) = state
            .cpu_pool
            .execute(state.shutdown_token(), move |_| {
                let result = runner.step(&frame);
                let text = String::from_utf8(runner.document.encode_preserved()?)?;
                anyhow::ensure!(
                    text.len() <= 32 * 1024 * 1024,
                    "tracking snapshot budget exceeded"
                );
                Ok((runner, result, text))
            })
            .await
            .map_err(bad)?;
        runner = next;
        provider.verify().await?;
        match result {
            Ok(true) => {
                let mut job = cell.lock().await;
                job.done = runner.done;
                job.inliers = runner.inliers;
                job.snapshot = Arc::from(text);
            }
            Ok(false) => {
                cell.lock().await.error = Some(format!(
                    "lost track after {} frame(s): not enough detail inside the mask",
                    runner.done
                ));
                break;
            }
            Err(e) => return Err(bad(e)),
        }
    }
    Ok(())
}
pub(super) async fn start(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    p: Value,
) -> AppResult<Value> {
    let (plan, text) = state
        .cpu_pool
        .execute(state.shutdown_token(), {
            let p = p.clone();
            move |_| {
                let plan = Plan::read(&p)?;
                let text = String::from_utf8(plan.document.encode_preserved()?)?;
                Ok((plan, text))
            }
        })
        .await
        .map_err(bad)?;
    let sources: BTreeMap<String, String> = p
        .get("sources")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(bad)?
        .unwrap_or_default();
    if sources.len() > 64 {
        return Err(bad("too many tracking bindings"));
    }
    let mut preflight = Provider::open(state, headers, &plan, &sources, false).await?;
    let expected = preflight.fingerprint().await?;
    let access = preflight.access_ids();
    let catalog = match preflight.sequence_id.clone() {
        Some(id) => Some((id, <[u8; 32]>::try_from(expected.as_slice()).map_err(bad)?)),
        None => None,
    };
    let proof = capture_proof(state, &access).await?;
    drop(preflight);
    if text.len() > 32 * 1024 * 1024 {
        return Err(bad("tracking snapshot budget exceeded"));
    }
    let target = (plan.sequence, plan.clip, plan.effect, plan.mask);
    let label = format!(
        "Track {} ({})",
        plan.name,
        if plan.delta < 0 {
            "backward"
        } else {
            "forward"
        }
    );
    let mut store = state.filmcraft_tracking_jobs.lock().await;
    if !store.owners.contains_key(owner) && store.owners.len() >= 256 {
        let old = store
            .owners
            .iter()
            .find(|(_, session)| session.jobs.is_empty())
            .map(|(key, _)| key.clone());
        if let Some(old) = old {
            store.owners.remove(&old);
        } else {
            return Err(AppError::too_many_requests(
                "tracking session capacity reached",
            ));
        }
    }
    // Reserve each live job's maximum published snapshot until its physical CPU scope drains.
    let mut retained = 32 * 1024 * 1024;
    for session in store.owners.values() {
        for cell in session.jobs.values() {
            let job = cell.lock().await;
            retained += if job.finished {
                job.snapshot.len()
            } else {
                32 * 1024 * 1024
            };
        }
    }
    if retained > 256 * 1024 * 1024 {
        return Err(AppError::too_many_requests("tracking snapshot capacity reached; explicitly discard finished tracking jobs to free space"));
    }
    let mut active = 0;
    for s in store.owners.values() {
        for c in s.jobs.values() {
            if !c.lock().await.finished {
                active += 1;
            }
        }
    }
    if active >= 4 {
        return Err(AppError::too_many_requests(
            "tracking worker capacity reached",
        ));
    }
    let id = store
        .next
        .checked_add(1)
        .ok_or_else(|| bad("tracking ID exhausted"))?;
    let session = store.owners.entry(owner.into()).or_default();
    for c in session.jobs.values() {
        let j = c.lock().await;
        if !j.finished && j.target == target {
            return Err(AppError::conflict("this mask is already being tracked"));
        }
    }
    let mut bytes = 32 * 1024 * 1024;
    for c in session.jobs.values() {
        let job = c.lock().await;
        bytes += if job.finished {
            job.snapshot.len()
        } else {
            32 * 1024 * 1024
        };
    }
    if session.jobs.len() >= 16 || bytes > 64 * 1024 * 1024 {
        return Err(AppError::too_many_requests("tracking snapshot capacity reached; explicitly discard finished tracking jobs to free space"));
    }
    let cell = Arc::new(Mutex::new(Job {
        id,
        instance: uuid::Uuid::new_v4().to_string(),
        target,
        label,
        total: plan.steps,
        done: 0,
        inliers: 0,
        finished: false,
        error: None,
        stopped: false,
        cancel: CancellationToken::new(),
        snapshot: Arc::from(text),
        sources: access,
        proof,
        catalog,
        started: Instant::now(),
        seconds: 0.,
    }));
    session.jobs.insert(id, cell.clone());
    store.next = id;
    drop(store);
    let mut scoped = state.clone();
    let (pool, scope) = state.cpu_pool.scoped();
    scoped.cpu_pool = pool;
    let task_cell = cell.clone();
    let task_headers = headers.clone();
    state.spawn_task(async move {
        let cancel=task_cell.lock().await.cancel.clone();let shutdown=scoped.shutdown_token();
        let permit=tokio::select! {permit=scoped.acquire_job_slot(crate::config::resource_classes::ResourceClass::Analysis)=>permit.ok(),_=cancel.cancelled()=>None,_=shutdown.cancelled()=>None};
        let result=if permit.is_some(){
            std::panic::AssertUnwindSafe(worker(&scoped,&task_headers,&task_cell,plan,sources,expected)).catch_unwind().await
        }else{task_cell.lock().await.stopped=true;Ok(Ok(()))};
        scope.wait_idle().await;
        drop(permit);
        let mut job = task_cell.lock().await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => job.error = Some(e.public_message().to_owned()),
            Err(_) => job.error = Some("tracking worker failed".into()),
        };
        job.seconds = job.started.elapsed().as_secs_f64();
        job.finished = true;
    });
    if p["wait"] == true {
        loop {
            if cell.lock().await.finished {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    let job = cell.lock().await;
    authorize(state, headers, &job).await?;
    if p["wait"] == true {
        verify_snapshot(state, headers, &job).await?;
    }
    let status = job.view(p["wait"] == true);
    Ok(json!({"job":status["id"],"frames":job.total,"status":status}))
}
