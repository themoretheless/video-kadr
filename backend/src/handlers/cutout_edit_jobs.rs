//! Durable correction of a retained automatic mask; no learned model is loaded.
use super::jobs::{dispatch_job, JobLeaseHeartbeat};
use crate::{
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    jobs::{AcquirePlan, EnqueueOutcome, JobEvent, JobKind, JobService},
    services::{
        local_cutout,
        local_mask_edit::{self, Correction},
    },
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Work {
    schema_version: u32,
    owner: String,
    source_id: String,
    base_job_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_origin: Option<ProjectOrigin>,
    #[serde(default)]
    subject: Option<crate::domain::concat_cutout_import::Subject>,
    original: local_cutout::Metadata,
    engine_hash: String,
    correction: Correction,
}
impl Work {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema_version == 1
                && !self.owner.is_empty()
                && !self.source_id.is_empty()
                && (self.project_origin.is_some()
                    || uuid::Uuid::parse_str(&self.base_job_id)?.to_string() == self.base_job_id)
                && [3, 6, 8].contains(&self.original.schema_version)
                && self.original.mask_file_hash.is_some()
                && [
                    Some(&self.engine_hash),
                    Some(&self.original.source_hash),
                    Some(&self.original.mask_hash),
                    self.original.mask_file_hash.as_ref()
                ]
                .into_iter()
                .flatten()
                .all(|h| h.len() == 64 && h.bytes().all(|v| v.is_ascii_hexdigit())),
            "invalid correction snapshot"
        );
        if let Some(origin) = &self.project_origin {
            anyhow::ensure!(
                origin.snapshot.source_id == self.source_id
                    && origin.snapshot.subject == self.subject
                    && self.base_job_id == origin.project_id,
                "invalid project mask source"
            );
            origin.snapshot.validate()?;
        }
        self.correction.validate()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectOrigin {
    project_id: String,
    composition_id: Option<String>,
    clip_id: String,
    snapshot: crate::domain::cutout::Attachment,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectRequest {
    composition_id: Option<String>,
    clip_id: String,
    correction: Correction,
    expected_attachment: Option<crate::domain::cutout::Attachment>,
}
async fn project_snapshot(
    state: &AppState,
    owner: &str,
    project: &str,
    composition: Option<&str>,
    clip: &str,
) -> AppResult<(crate::domain::cutout::Attachment, u32, u32)> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM composition_project_members WHERE project_id=? AND actor=?",
    )
    .bind(project)
    .bind(owner)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| AppError::internal("read mask project role", e))?;
    if !matches!(role.as_deref(), Some("owner" | "editor")) {
        return Err(AppError::forbidden(
            "Для правки маски нужен доступ редактора к проекту",
        ));
    }
    let project = state
        .db
        .get_composition_project_for(project, owner)
        .await
        .map_err(|e| AppError::internal("read mask project", e))?
        .ok_or_else(|| AppError::not_found("Проект не найден"))?;
    snapshot_from_document(&project.document, composition, clip)
        .map_err(|_| AppError::conflict("Сохранённая маска клипа не найдена или изменилась"))
}
fn snapshot_from_document(
    document: &Value,
    composition: Option<&str>,
    clip: &str,
) -> anyhow::Result<(crate::domain::cutout::Attachment, u32, u32)> {
    let content = if let Some(nodes) = document.get("compositions") {
        &nodes[composition.ok_or_else(|| anyhow::anyhow!("timeline required"))?]["content"]
    } else {
        anyhow::ensure!(composition.is_none(), "unexpected timeline");
        document
    };
    let pair = &content["metadata"]["mediaVersions"][clip];
    let snapshot: crate::domain::cutout::Attachment =
        serde_json::from_value(pair["cutout"].clone())?;
    let tracks = content["tracks"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing tracks"))?;
    let selected = tracks
        .iter()
        .filter_map(|t| t["clips"].as_array())
        .flatten()
        .find(|c| c["id"].as_str() == Some(clip))
        .ok_or_else(|| anyhow::anyhow!("missing clip"))?;
    anyhow::ensure!(
        selected["sourceId"] == pair["originalSourceId"]
            || selected["sourceId"] == pair["copySourceId"],
        "stale media pair"
    );
    let source = &content["sources"][&snapshot.source_id];
    let mask = &content["sources"][&snapshot.mask_source_id];
    anyhow::ensure!(
        source["kind"] == "image"
            && mask["kind"] == "image"
,
        "invalid mask sources"
    );
    let width = u32::try_from(
        source["width"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("width"))?,
    )?;
    let height = u32::try_from(
        source["height"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("height"))?,
    )?;
    anyhow::ensure!(width > 0 && height > 0, "empty source");
    let (mw,mh)=crate::domain::cutout::mask_dimensions(&snapshot.model_hash,width,height)?;
    anyhow::ensure!(mask["width"]==mw&&mask["height"]==mh,"invalid saved mask grid");
    Ok((snapshot, width, height))
}
/// Correct a saved/imported project mask without its original inference job.
pub async fn submit_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<ProjectRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    request
        .correction
        .validate()
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let (snapshot, width, height) = project_snapshot(
        &state,
        &owner,
        &project_id,
        request.composition_id.as_deref(),
        &request.clip_id,
    )
    .await?;
    if request
        .expected_attachment
        .as_ref()
        .is_some_and(|expected| expected != &snapshot)
    {
        return Err(AppError::conflict(
            "Сохранённые правки маски изменились. Откройте полотно заново",
        ));
    }
    let executable = mask_executable(&state, &snapshot.model_hash)?;
    let engine_hash = local_mask_edit::identity(executable)
        .await
        .map_err(|_| AppError::service_unavailable("Движок обработки маски недоступен"))?;
    let mut original = local_cutout::Metadata {
        schema_version: crate::domain::cutout::still_mask_schema(&snapshot.model_hash),
        width,
        height,
        mask_width: crate::domain::cutout::mask_dimensions(&snapshot.model_hash,width,height).map_err(|_|AppError::conflict("Неверный размер маски"))?.0,
        mask_height: crate::domain::cutout::mask_dimensions(&snapshot.model_hash,width,height).map_err(|_|AppError::conflict("Неверный размер маски"))?.1,
        blank_mask: false,
        mask_hash: snapshot.mask_hash.clone(),
        source_hash: snapshot.source_hash.clone(),
        model_hash: snapshot.model_hash.clone(),
        runtime_hash: snapshot.runtime_hash.clone(),
        output_hash: String::new(),
        mask_file_hash: Some(snapshot.mask_file_hash.clone()),
    };
    let mask = super::cutout_jobs::source(&state, &snapshot.mask_source_id, &owner)
        .await
        .map_err(|_| AppError::forbidden("Маска недоступна"))?;
    // The portable attachment carries the mask bytes/hash, not a redundant blank flag.
    if local_cutout::read_verified_mask(&mask, &original)
        .await
        .is_err()
    {
        original.blank_mask = true;
        local_cutout::read_verified_mask(&mask, &original)
            .await
            .map_err(|_| AppError::conflict("Сохранённая маска изменилась"))?;
    }
    let work = Work {
        schema_version: 1,
        owner,
        source_id: snapshot.source_id.clone(),
        base_job_id: project_id.clone(),
        subject: snapshot.subject,
        project_origin: Some(ProjectOrigin {
            project_id,
            composition_id: request.composition_id,
            clip_id: request.clip_id,
            snapshot,
        }),
        original,
        engine_hash,
        correction: request.correction,
    };
    work.validate()
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    inputs(&state, &work)
        .await
        .map_err(|_| AppError::conflict("Исходник или маска изменились"))?;
    enqueue(&state, work).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectQuery {
    composition_id: Option<String>,
}
pub async fn project_settings(
    State(state): State<AppState>,
    Path((project, clip)): Path<(String, String)>,
    Query(query): Query<ProjectQuery>,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let (snapshot, width, height) = project_snapshot(
        &state,
        &owner,
        &project,
        query.composition_id.as_deref(),
        &clip,
    )
    .await?;
    // Resolve both resources under the caller's current media permissions.
    for source in [&snapshot.source_id, &snapshot.mask_source_id] {
        super::cutout_jobs::source(&state, source, &owner)
            .await
            .map_err(|_| AppError::forbidden("Исходник или маска недоступны"))?;
    }
    Ok(Json(
        json!({"snapshot":snapshot,"width":width,"height":height}),
    ))
}
pub async fn project_original(
    State(state): State<AppState>,
    Path((project, clip)): Path<(String, String)>,
    Query(query): Query<ProjectQuery>,
    headers: HeaderMap,
) -> AppResult<axum::response::Response> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let (snapshot, width, height) = project_snapshot(
        &state,
        &owner,
        &project,
        query.composition_id.as_deref(),
        &clip,
    )
    .await?;
    let path = super::cutout_jobs::source(&state, &snapshot.source_id, &owner)
        .await
        .map_err(|_| AppError::forbidden("Исходник недоступен"))?;
    let png =
        super::cutout_jobs::original_preview(&state, path, &snapshot.source_hash, (width, height))
            .await?;
    let current = project_snapshot(
        &state,
        &owner,
        &project,
        query.composition_id.as_deref(),
        &clip,
    )
    .await?
    .0;
    if current != snapshot {
        return Err(AppError::conflict("Сохранённая маска изменилась"));
    }
    super::cutout_jobs::source(&state, &snapshot.source_id, &owner)
        .await
        .map_err(|_| AppError::forbidden("Исходник недоступен"))?;
    axum::response::Response::builder()
        .header("content-type", "image/png")
        .header("cache-control", "no-store")
        .body(axum::body::Body::from(png))
        .map_err(|e| AppError::internal("project mask original", e))
}

#[derive(Debug)]
enum Violation {
    Access,
    Source,
    Mask,
    Engine,
}
impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Access => "mask correction source access revoked",
            Self::Source => "mask correction source changed",
            Self::Mask => "mask correction snapshot changed",
            Self::Engine => "mask correction executable changed",
        })
    }
}
impl std::error::Error for Violation {}
pub(super) async fn region_paths(
    state: &AppState,
    owner: &str,
    source_id: &str,
    original: &local_cutout::Metadata,
    correction: &Correction,
) -> anyhow::Result<Vec<Option<std::path::PathBuf>>> {
    correction.validate()?;
    if correction.strokes.iter().all(|s| s.region.is_none()) {
        return Ok(vec![]);
    }
    let mut paths = Vec::with_capacity(correction.strokes.len());
    for stroke in &correction.strokes {
        if let Some(region) = &stroke.region {
            anyhow::ensure!(
                region.source_id == source_id && region.source_hash == original.source_hash,
                Violation::Source
            );
            let path = super::cutout_jobs::source(state, &region.mask_source_id, owner)
                .await
                .map_err(|_| Violation::Access)?;
            local_mask_edit::verify_region_file(
                original,
                &local_mask_edit::RegionInput {
                    path: &path,
                    attachment: region,
                },
            )
            .await
            .map_err(|_| Violation::Mask)?;
            paths.push(Some(path));
        } else {
            paths.push(None);
        }
    }
    Ok(paths)
}
async fn inputs(state: &AppState, work: &Work) -> anyhow::Result<(std::path::PathBuf, Vec<u8>)> {
    if let Some(origin) = &work.project_origin {
        let (snapshot, _, _) = project_snapshot(
            state,
            &work.owner,
            &origin.project_id,
            origin.composition_id.as_deref(),
            &origin.clip_id,
        )
        .await
        .map_err(|_| Violation::Access)?;
        anyhow::ensure!(snapshot == origin.snapshot, Violation::Mask);
        let source = super::cutout_jobs::source(state, &work.source_id, &work.owner)
            .await
            .map_err(|_| Violation::Access)?;
        anyhow::ensure!(
            crate::project_archive::fingerprint_file_with_limit(&source, 256 * 1024 * 1024)
                .await?
                .1
                == work.original.source_hash,
            Violation::Source
        );
        let mask = super::cutout_jobs::source(state, &snapshot.mask_source_id, &work.owner)
            .await
            .map_err(|_| Violation::Access)?;
        let bytes = local_cutout::read_verified_mask(&mask, &work.original)
            .await
            .map_err(|_| Violation::Mask)?;
        region_paths(
            state,
            &work.owner,
            &work.source_id,
            &work.original,
            &work.correction,
        )
        .await?;
        return Ok((source, bytes));
    }
    let access = super::cutout_assets::job_access(state, &work.base_job_id)
        .await
        .map_err(|_| Violation::Access)?;
    anyhow::ensure!(
        access == Some((work.owner.clone(), work.source_id.clone())),
        Violation::Access
    );
    let source = super::cutout_jobs::source(state, &work.source_id, &work.owner)
        .await
        .map_err(|_| Violation::Access)?;
    anyhow::ensure!(
        crate::project_archive::fingerprint_file_with_limit(&source, 256 * 1024 * 1024)
            .await?
            .1
            == work.original.source_hash,
        Violation::Source
    );
    let mask = super::cutout_assets::job_mask_output(state, &work.base_job_id).await?;
    let bytes = local_cutout::read_verified_mask(&mask, &work.original)
        .await
        .map_err(|_| Violation::Mask)?;
    region_paths(
        state,
        &work.owner,
        &work.source_id,
        &work.original,
        &work.correction,
    )
    .await?;
    Ok((source, bytes))
}
pub async fn submit(
    State(state): State<AppState>,
    Path(base): Path<String>,
    headers: HeaderMap,
    ApiJson(correction): ApiJson<Correction>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let (base_owner, source_id) = super::cutout_assets::job_access(&state, &base)
        .await?
        .ok_or_else(|| AppError::not_found("Маска не найдена"))?;
    if owner != base_owner {
        return Err(AppError::forbidden("Это маска другого пользователя"));
    }
    super::cutout_assets::authorize_job(&state, &base, &headers, true).await?;
    correction
        .validate()
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let job = state
        .get_job(&base)
        .await
        .map_err(|e| AppError::internal("read base mask job", e))?
        .ok_or_else(|| AppError::not_found("Задание не найдено"))?;
    if job.status != crate::model::JobStatus::Done {
        return Err(AppError::conflict("Исходная маска ещё не готова"));
    }
    let original: local_cutout::Metadata = serde_json::from_value(
        job.result
            .ok_or_else(|| AppError::conflict("Нет исходной маски"))?,
    )
    .map_err(|_| AppError::conflict("Неверная исходная маска"))?;
    if original.mask_file_hash.is_none() {
        return Err(AppError::conflict("В этом задании маска не сохранена"));
    }
    let executable = mask_executable(&state, &original.model_hash)?;
    let engine_hash = local_mask_edit::identity(executable)
        .await
        .map_err(|_| AppError::service_unavailable("Движок обработки маски недоступен"))?;
    let work = Work {
        schema_version: 1,
        owner,
        source_id,
        subject: super::cutout_assets::job_subject(&state,&base).await?,
        base_job_id: base,
        project_origin: None,
        original,
        engine_hash,
        correction,
    };
    work.validate()
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    inputs(&state, &work)
        .await
        .map_err(|_| AppError::conflict("Исходник или сохранённая маска изменились"))?;
    enqueue(&state, work).await
}
async fn enqueue(state: &AppState, work: Work) -> AppResult<(StatusCode, Json<Value>)> {
    let id = uuid::Uuid::new_v4().to_string();
    let mut tx = state
        .db
        .pool()
        .begin()
        .await
        .map_err(|e| AppError::internal("begin mask edit", e))?;
    sqlx::query("INSERT INTO cutout_job_access(job_id,owner,source_id) VALUES(?,?,?)")
        .bind(&id)
        .bind(&work.owner)
        .bind(&work.source_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal("save mask edit access", e))?;
    sqlx::query("INSERT INTO cutout_job_edits(job_id,base_job_id,settings_json) VALUES(?,?,?)")
        .bind(&id)
        .bind(&work.base_job_id)
        .bind(
            serde_json::to_string(&work.correction)
                .map_err(|e| AppError::internal("serialize mask strokes", e))?,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal("save mask edit settings", e))?;
    if let Some(subject)=work.subject {
        sqlx::query("INSERT INTO cutout_job_subjects(job_id,subject_json) VALUES(?,?)").bind(&id).bind(serde_json::to_string(&subject).map_err(|e|AppError::internal("encode cutout subject",e))?).execute(&mut *tx).await.map_err(|e|AppError::internal("save cutout subject",e))?;
    }
    tx.commit()
        .await
        .map_err(|e| AppError::internal("commit mask edit", e))?;
    let payload =
        serde_json::to_value(work).map_err(|e| AppError::internal("serialize mask edit", e))?;
    let outcome = state
        .enqueue_job(
            id.clone(),
            JobKind::CutoutEdit,
            &payload,
            &format!("cutout-edit:{id}"),
        )
        .await
        .map_err(|e| AppError::internal("enqueue mask edit", e))?;
    if matches!(outcome, EnqueueOutcome::RateLimited) {
        return Err(AppError::too_many_requests("Слишком много заданий"));
    }
    dispatch_job(state, &id).await;
    Ok((StatusCode::ACCEPTED, Json(json!({"jobId":id}))))
}
pub async fn settings(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    if super::cutout_assets::job_access(&state, &id)
        .await?
        .is_none()
    {
        return Err(AppError::not_found("Правка маски не найдена"));
    }
    super::cutout_assets::authorize_job(&state, &id, &headers, true).await?;
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT base_job_id,settings_json FROM cutout_job_edits WHERE job_id=?")
            .bind(&id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read mask edit settings", e))?;
    let (base, settings) =
        row.ok_or_else(|| AppError::not_found("Это исходная маска без правок"))?;
    let correction: Correction = serde_json::from_str(&settings)
        .map_err(|e| AppError::internal("decode mask settings", e))?;
    super::cutout_assets::authorize_job(&state, &id, &headers, true).await?;
    Ok(Json(json!({"baseJobId":base,"correction":correction})))
}
pub(super) fn spawn(
    state: AppState,
    id: String,
    work: Work,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let st = state.clone();
    state.spawn_task(async move {
        let _lease = lease;
        if !JobService::mark_queued(&st, &id).await {
            st.clear_cancel(&id).await;
            return;
        }
        let Some(_guards) = JobService::acquire_execution(
            &st,
            &id,
            &token,
            None,
            AcquirePlan {
                resource_class: ResourceClass::Analysis,
                acquire_render_permit: false,
            },
        )
        .await
        else {
            return;
        };
        if !JobService::mark_running(&st, &id, "cutout_edit", attempt).await {
            st.clear_cancel(&id).await;
            return;
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let drain = JobService::spawn_progress_drain(st.clone(), id.clone(), rx);
        let outcome: anyhow::Result<Value> = async {
            let (source, mask_bytes) = inputs(&st, &work).await?;
            let executable =
                mask_executable(&st, &work.original.model_hash).map_err(|_| Violation::Engine)?;
            anyhow::ensure!(
                local_mask_edit::identity(executable)
                    .await
                    .map_err(|_| Violation::Engine)?
                    == work.engine_hash,
                Violation::Engine
            );
            let target = super::cutout_jobs::output(&st, &id)?;
            let mask = super::cutout_jobs::mask_output(&st, &id)?;
            crate::services::tts_model_download::directory(
                &st.storage,
                std::path::Path::new("cutout-jobs"),
            )
            .await?;
            for path in [&target, &mask] {
                if tokio::fs::symlink_metadata(path).await.is_ok() {
                    anyhow::ensure!(
                        tokio::fs::symlink_metadata(path)
                            .await?
                            .file_type()
                            .is_file(),
                        "unexpected correction output"
                    );
                    tokio::fs::remove_file(path).await?;
                }
            }
            let mut options = tokio::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                options.mode(0o600);
            }
            let mut file = options.open(&mask).await?;
            file.write_all(&mask_bytes).await?;
            file.sync_all().await?;
            drop(file);
            let paths = region_paths(
                &st,
                &work.owner,
                &work.source_id,
                &work.original,
                &work.correction,
            )
            .await?;
            let regions: Vec<_> = paths
                .iter()
                .zip(&work.correction.strokes)
                .map(|(path, stroke)| {
                    path.as_deref()
                        .zip(stroke.region.as_ref())
                        .map(|(path, attachment)| local_mask_edit::RegionInput { path, attachment })
                })
                .collect();
            let result = local_mask_edit::correct_with_regions(
                &st.process_runtime,
                executable,
                local_mask_edit::Input {
                    source: &source,
                    mask: &mask,
                    original: &work.original,
                    correction: &work.correction,
                },
                &regions,
                &target,
                &token,
                std::time::Duration::from_secs(1800),
                &tx,
            )
            .await?;
            inputs(&st, &work).await?;
            let mut metadata = work.original.clone();
            metadata.output_hash = result.output_hash;
            Ok(serde_json::to_value(metadata)?)
        }
        .await;
        drop(tx);
        let _ = drain.await;
        if token.is_cancelled() {
            if !st.is_shutting_down() {
                cleanup(&st, &id).await;
            }
            JobService::mark_cancelled(&st, &id).await;
        } else {
            let event = match outcome {
                Ok(result) => JobEvent::Succeeded { result },
                Err(error) => {
                    cleanup(&st, &id).await;
                    JobEvent::Failed {
                        kind: match error.downcast_ref::<Violation>() {
                            Some(Violation::Access) => crate::jobs::ErrorKind::Security,
                            Some(_) => crate::jobs::ErrorKind::Validation,
                            None => crate::jobs::classify_job_error(&error),
                        },
                        message: crate::privacy::redact_text(&error.to_string()),
                    }
                }
            };
            JobService::transition(&st, &id, event).await;
            st.clear_cancel(&id).await;
        }
    });
}
async fn cleanup(state: &AppState, id: &str) {
    for path in [
        super::cutout_jobs::output(state, id),
        super::cutout_jobs::mask_output(state, id),
    ]
    .into_iter()
    .flatten()
    {
        let _ = tokio::fs::remove_file(path).await;
    }
}

#[cfg(test)]
mod project_tests {
    use super::*;
    fn document() -> Value {
        json!({"sources":{"input":{"kind":"image","width":37,"height":23},"mask":{"kind":"image","width":512,"height":512}},"tracks":[{"clips":[{"id":"picture","sourceId":"copy"}]}],"metadata":{"mediaVersions":{"picture":{"originalSourceId":"input","copySourceId":"copy","cutout":{"sourceId":"input","maskSourceId":"mask","sourceHash":"a".repeat(64),"maskHash":"b".repeat(64),"maskFileHash":"c".repeat(64),"modelHash":"d".repeat(64),"runtimeHash":"e".repeat(64),"correction":{"custom":false,"strokes":[],"feather":0}}}}}})
    }
    #[test]
    fn saved_mask_requires_exact_clip_and_explicit_nested_timeline() {
        let plain = document();
        let (snapshot, w, h) = snapshot_from_document(&plain, None, "picture").unwrap();
        assert_eq!((w, h), (37, 23));
        assert_eq!(snapshot.mask_source_id, "mask");
        assert!(snapshot_from_document(&plain, None, "other").is_err());
        assert!(snapshot_from_document(&plain, Some("root"), "picture").is_err());
        let nested = json!({"compositions":{"root":{"content":plain}}});
        assert!(snapshot_from_document(&nested, None, "picture").is_err());
        assert!(snapshot_from_document(&nested, Some("wrong"), "picture").is_err());
        assert_eq!(
            snapshot_from_document(&nested, Some("root"), "picture")
                .unwrap()
                .0,
            snapshot
        );
    }
    #[test]
    fn stale_clip_missing_sources_and_invalid_mask_identity_refuse() {
        for (pointer, replacement) in [
            ("/tracks/0/clips/0/sourceId", json!("unrelated")),
            ("/sources/mask/width", json!(511)),
            ("/sources/input/kind", json!("video")),
            (
                "/metadata/mediaVersions/picture/cutout/maskFileHash",
                json!("bad"),
            ),
        ] {
            let mut doc = document();
            *doc.pointer_mut(pointer).unwrap() = replacement;
            assert!(
                snapshot_from_document(&doc, None, "picture").is_err(),
                "{pointer}"
            );
        }
    }
}

fn mask_executable<'a>(state: &'a AppState, hash: &str) -> AppResult<&'a std::path::PathBuf> {
    if hash == crate::domain::cutout::SELFIE_MODEL_HASH {
        state.selfie.as_ref().as_ref().map(|c| &c.executable)
    } else if hash == crate::domain::cutout::PERSON_MODEL_HASH {
        state.person.as_ref().as_ref().map(|c| &c.executable)
    } else {
        state.cutout.as_ref().as_ref().map(|c| &c.executable)
    }
    .ok_or_else(|| AppError::service_unavailable("Обработка маски не настроена"))
}
