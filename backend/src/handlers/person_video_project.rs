//! Saved/imported video-mask correction origins; no inference-job dependency.
use crate::{
    domain::video_cutout::{Attachment, FrameCorrection},
    error::{ApiJson, AppError, AppResult},
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
#[derive(Debug)]
pub(super) enum Invariant {
    Access,
    Snapshot,
}
impl std::fmt::Display for Invariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Access => "project video-mask access revoked",
            Self::Snapshot => "project video-mask snapshot/resources changed",
        })
    }
}
impl std::error::Error for Invariant {}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Origin {
    pub project_id: String,
    pub composition_id: Option<String>,
    pub clip_id: String,
    pub snapshot: Attachment,
    /// Immutable persisted baseline when session tracking adds resource inventory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_snapshot: Option<Attachment>,
}
impl Origin {
    pub(super) fn validate(&self) -> anyhow::Result<()> {
        self.snapshot.validate()?;
        if let Some(saved) = &self.saved_snapshot {
            saved.validate()?;
            let mut comparable = self.snapshot.clone();
            comparable.tracked_regions = saved.tracked_regions.clone();
            anyhow::ensure!(
                comparable == *saved,
                "session region changed baseline fields"
            );
            for region in &saved.tracked_regions {
                anyhow::ensure!(
                    self.snapshot.tracked_regions.iter().any(|r| r == region),
                    "session region removed baseline inventory"
                );
            }
        }
        Ok(())
    }
    fn matches_saved(&self, current: &Attachment) -> bool {
        current == &self.snapshot || self.saved_snapshot.as_ref() == Some(current)
    }
    fn refresh(&mut self, current: Attachment) -> anyhow::Result<()> {
        self.validate()?;
        let mut effective = self.snapshot.clone();
        effective.corrections = current.corrections.clone();
        if current == effective {
            self.snapshot = current;
            self.saved_snapshot = None;
        } else {
            let mut baseline = self.saved_snapshot.clone().ok_or(Invariant::Snapshot)?;
            baseline.corrections = current.corrections.clone();
            anyhow::ensure!(current == baseline, Invariant::Snapshot);
            self.saved_snapshot = Some(current.clone());
            self.snapshot.corrections = current.corrections;
        }
        self.validate()
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectRequest {
    composition_id: Option<String>,
    clip_id: String,
    expected_attachment: Attachment,
    corrections: Vec<FrameCorrection>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectQuery {
    composition_id: Option<String>,
}
async fn snapshot(
    state: &AppState,
    actor: &str,
    project: &str,
    composition: Option<&str>,
    clip: &str,
) -> AppResult<Attachment> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM composition_project_members WHERE project_id=? AND actor=?",
    )
    .bind(project)
    .bind(actor)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| AppError::internal("read video-mask project role", e))?;
    if !matches!(role.as_deref(), Some("owner" | "editor")) {
        return Err(AppError::forbidden(
            "Для правки видеомаски нужен доступ редактора к проекту",
        ));
    }
    let project = state
        .db
        .get_composition_project_for(project, actor)
        .await
        .map_err(|e| AppError::internal("read video-mask project", e))?
        .ok_or_else(|| AppError::not_found("Проект не найден"))?;
    attachment(&project.document, composition, clip)
        .map_err(|_| AppError::conflict("Сохранённая видеомаска не найдена или изменилась"))
}
fn attachment(
    document: &Value,
    composition: Option<&str>,
    clip: &str,
) -> anyhow::Result<Attachment> {
    let content = if document.get("compositions").is_some() {
        &document["compositions"]
            [composition.ok_or_else(|| anyhow::anyhow!("timeline required"))?]["content"]
    } else {
        anyhow::ensure!(composition.is_none(), "unexpected timeline");
        document
    };
    let pair = &content["metadata"]["mediaVersions"][clip];
    let snapshot: Attachment = serde_json::from_value(pair["videoCutout"].clone())?;
    let tracks = content["tracks"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("tracks missing"))?;
    let selected = tracks
        .iter()
        .filter_map(|t| t["clips"].as_array())
        .flatten()
        .find(|c| c["id"].as_str() == Some(clip))
        .ok_or_else(|| anyhow::anyhow!("clip missing"))?;
    anyhow::ensure!(
        selected["sourceId"] == pair["originalSourceId"]
            || selected["sourceId"] == pair["copySourceId"],
        "stale pair"
    );
    anyhow::ensure!(
        pair["originalSourceId"] == snapshot.source_id
            && pair["copySourceId"]
                .as_str()
                .is_some_and(|v| v != snapshot.source_id && v != snapshot.mask_source_id),
        "invalid source pair"
    );
    let source = &content["sources"][&snapshot.source_id];
    let mask = &content["sources"][&snapshot.mask_source_id];
    let copy = &content["sources"][pair["copySourceId"].as_str().unwrap()];
    anyhow::ensure!(
        source["kind"] == "video"
            && copy["kind"] == "video"
            && mask["kind"] == "video"
            && mask["hasAudio"] == false
            && mask["width"] == snapshot.mask_width
            && mask["height"] == snapshot.mask_height
            && copy["width"] == snapshot.width
            && copy["height"] == snapshot.height,
        "invalid video resources"
    );
    for region in &snapshot.tracked_regions {
        let input = &content["sources"][&region.source_id];
        let mask = &content["sources"][&region.mask_source_id];
        anyhow::ensure!(
            input["kind"] == "video"
                && [
                    (json!(snapshot.width), json!(snapshot.height)),
                    (json!(snapshot.height), json!(snapshot.width))
                ]
                .contains(&(input["width"].clone(), input["height"].clone()))
                && mask["kind"] == "video"
                && mask["hasAudio"] == false
                && mask["width"] == 256
                && mask["height"] == 256
                && region.source_id != pair["copySourceId"].as_str().unwrap()
                && region.mask_source_id != pair["copySourceId"].as_str().unwrap(),
            "invalid retained video region resources"
        );
    }
    Ok(snapshot)
}
pub(super) async fn inputs(
    state: &AppState,
    actor: &str,
    origin: &Origin,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    origin.validate()?;
    let resources = resources(state, actor, origin).await?;
    let saved = snapshot(
        state,
        actor,
        &origin.project_id,
        origin.composition_id.as_deref(),
        &origin.clip_id,
    )
    .await
    .map_err(|_| Invariant::Snapshot)?;
    anyhow::ensure!(origin.matches_saved(&saved), Invariant::Snapshot);
    Ok(resources)
}
/// Permission-only checks for status/cancellation-independent result observation.
pub(super) async fn authorize_resources(
    state: &AppState,
    actor: &str,
    origin: &Origin,
) -> anyhow::Result<()> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM composition_project_members WHERE project_id=? AND actor=?",
    )
    .bind(&origin.project_id)
    .bind(actor)
    .fetch_optional(state.db.pool())
    .await?;
    anyhow::ensure!(
        matches!(role.as_deref(), Some("owner" | "editor")),
        Invariant::Access
    );
    let mut ids = std::collections::BTreeSet::from([
        origin.snapshot.source_id.as_str(),
        origin.snapshot.mask_source_id.as_str(),
    ]);
    for region in &origin.snapshot.tracked_regions {
        ids.insert(&region.source_id);
        ids.insert(&region.mask_source_id);
    }
    for id in ids {
        if let Some(space) = state.db.source_space_id(id).await? {
            anyhow::ensure!(
                state.db.is_space_member(&space, actor).await?,
                Invariant::Access
            );
        }
        anyhow::ensure!(
            state
                .library
                .get(id)
                .await
                .is_some_and(|e| e.kind == "source"),
            Invariant::Access
        );
    }
    Ok(())
}
/// Resolve just the USED region videos after the complete saved inventory was
/// authorized and fingerprinted. No public request may supply paths.
pub(super) async fn used_regions(
    state: &AppState,
    actor: &str,
    origin: &Origin,
    corrections: &[FrameCorrection],
) -> anyhow::Result<Vec<(PathBuf, crate::domain::video_region::Attachment)>> {
    crate::domain::video_cutout::validate_corrections_with_regions(
        corrections,
        origin.snapshot.frames,
        &origin.snapshot.tracked_regions,
    )?;
    inputs(state, actor, origin).await?;
    let ids = corrections
        .iter()
        .flat_map(|e| {
            e.correction
                .strokes
                .iter()
                .filter_map(|s| s.region.as_ref().map(|r| r.mask_source_id.as_str()))
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut result = Vec::new();
    for region in &origin.snapshot.tracked_regions {
        if ids.contains(region.mask_source_id.as_str()) {
            result.push((
                super::person_video_jobs::source(state, &region.mask_source_id, actor).await?,
                region.clone(),
            ));
        }
    }
    Ok(result)
}
pub(super) async fn resources(
    state: &AppState,
    actor: &str,
    origin: &Origin,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM composition_project_members WHERE project_id=? AND actor=?",
    )
    .bind(&origin.project_id)
    .bind(actor)
    .fetch_optional(state.db.pool())
    .await?;
    anyhow::ensure!(
        matches!(role.as_deref(), Some("owner" | "editor")),
        Invariant::Access
    );
    authorize_resources(state, actor, origin).await?;
    let saved = &origin.snapshot;
    let source = super::person_video_jobs::source(state, &saved.source_id, actor).await?;
    let mask = super::person_video_jobs::source(state, &saved.mask_source_id, actor).await?;
    for (path, hash) in [
        (&source, &saved.source_hash),
        (&mask, &saved.mask_file_hash),
    ] {
        let info = tokio::fs::symlink_metadata(path).await?;
        anyhow::ensure!(info.file_type().is_file(), Invariant::Snapshot);
        anyhow::ensure!(
            crate::project_archive::fingerprint_file_with_limit(path, 64 * 1024 * 1024 * 1024)
                .await?
                .1
                == *hash,
            Invariant::Snapshot
        );
    }
    for region in &saved.tracked_regions {
        for (id, hash) in [
            (&region.source_id, &region.source_hash),
            (&region.mask_source_id, &region.mask_file_hash),
        ] {
            let path = super::person_video_jobs::source(state, id, actor)
                .await
                .map_err(|_| Invariant::Access)?;
            anyhow::ensure!(
                tokio::fs::symlink_metadata(&path)
                    .await?
                    .file_type()
                    .is_file()
                    && crate::project_archive::fingerprint_file_with_limit(
                        &path,
                        64 * 1024 * 1024 * 1024
                    )
                    .await?
                    .1 == *hash,
                Invariant::Snapshot
            );
        }
    }
    Ok((source, mask))
}
pub async fn settings(
    State(state): State<AppState>,
    Path((project, clip)): Path<(String, String)>,
    Query(query): Query<ProjectQuery>,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    let snapshot = snapshot(
        &state,
        &actor,
        &project,
        query.composition_id.as_deref(),
        &clip,
    )
    .await?;
    let origin = Origin {
        project_id: project,
        composition_id: query.composition_id,
        clip_id: clip,
        snapshot,
        saved_snapshot: None,
    };
    inputs(&state, &actor, &origin)
        .await
        .map_err(|_| AppError::conflict("Ресурсы видеомаски изменились"))?;
    Ok(Json(json!({"snapshot":origin.snapshot})))
}
pub async fn submit(
    State(state): State<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<ProjectRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    crate::domain::video_cutout::validate_corrections_with_regions(
        &request.corrections,
        request.expected_attachment.frames,
        &request.expected_attachment.tracked_regions,
    )
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    let saved = snapshot(
        &state,
        &actor,
        &project,
        request.composition_id.as_deref(),
        &request.clip_id,
    )
    .await?;
    if saved != request.expected_attachment {
        return Err(AppError::conflict(
            "Сохранённые видеомаски изменились. Откройте полотно заново",
        ));
    }
    let origin = Origin {
        project_id: project,
        composition_id: request.composition_id,
        clip_id: request.clip_id,
        snapshot: saved,
        saved_snapshot: None,
    };
    enqueue_origin(&state, actor, origin, request.corrections).await
}
pub(super) async fn continue_job(
    state: &AppState,
    actor: String,
    mut origin: Origin,
    corrections: Vec<FrameCorrection>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let current = snapshot(
        state,
        &actor,
        &origin.project_id,
        origin.composition_id.as_deref(),
        &origin.clip_id,
    )
    .await?;
    origin
        .refresh(current)
        .map_err(|_| AppError::conflict("Исходные видеомаски проекта изменились"))?;
    crate::domain::video_cutout::validate_corrections_with_regions(
        &corrections,
        origin.snapshot.frames,
        &origin.snapshot.tracked_regions,
    )
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    enqueue_origin(state, actor, origin, corrections).await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegionRequest {
    sam_job_id: String,
    corrections: Vec<FrameCorrection>,
}
/// Fork a verified project correction with extra session inventory; never mutate its saved baseline.
pub async fn add_region(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<RegionRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    super::person_video_jobs::checked_result(&state, &id, &headers).await?;
    let mut origin = super::person_video_jobs::project_origin(&state, &id)
        .await?
        .ok_or_else(|| AppError::conflict("Нужна сохранённая исходная видеомаска проекта"))?;
    // Admission budgets and current inventory bindings are checked before resource copying.
    crate::domain::video_cutout::validate_corrections_with_regions(
        &request.corrections,
        origin.snapshot.frames,
        &origin.snapshot.tracked_regions,
    )
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    inputs(&state, &actor, &origin)
        .await
        .map_err(|_| AppError::conflict("Исходные видеомаски проекта изменились"))?;
    let region = super::sam_video_assets::store(
        State(state.clone()),
        Path(request.sam_job_id),
        headers,
        ApiJson(super::sam_video_assets::Target {
            project_id: origin.project_id.clone(),
        }),
    )
    .await?
    .0
    .snapshot;
    if let Some(old) = origin
        .snapshot
        .tracked_regions
        .iter()
        .find(|r| r.mask_source_id == region.mask_source_id)
    {
        if old != &region {
            return Err(AppError::conflict("Сохранённая область изменилась"));
        }
    } else {
        if origin.saved_snapshot.is_none() {
            origin.saved_snapshot = Some(origin.snapshot.clone());
        }
        origin.snapshot.tracked_regions.push(region);
    }
    origin
        .validate()
        .map_err(|_| AppError::conflict("Область не соответствует исходному видео"))?;
    enqueue_origin(&state, actor, origin, request.corrections).await
}
async fn enqueue_origin(
    state: &AppState,
    actor: String,
    origin: Origin,
    corrections: Vec<FrameCorrection>,
) -> AppResult<(StatusCode, Json<Value>)> {
    inputs(&state, &actor, &origin)
        .await
        .map_err(|_| AppError::conflict("Ресурсы видеомаски изменились"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let edit = super::person_video_jobs::CorrectionWork {
        base_job_id: id.clone(),
        mask_hash: origin.snapshot.mask_hash.clone(),
        project_origin: Some(origin.clone()),
        corrections,
    };
    super::person_video_jobs::enqueue_correction(
        &state,
        id,
        actor,
        origin.snapshot.source_id,
        origin.snapshot.source_hash,
        edit,
    )
    .await
}

#[cfg(test)]
mod session_region_tests {
    use super::*;
    fn region() -> crate::domain::video_region::Attachment {
        serde_json::from_value(json!({"sourceId":"original","maskSourceId":"tracked","sourceHash":"a".repeat(64),"maskHash":"d".repeat(64),"maskFileHash":"e".repeat(64),"encoderHash":"f".repeat(64),"decoderHash":"0".repeat(64),"runtimeHash":"1".repeat(64),"promptHash":"8e0f6ce7be723f24b418232b01f7fdb4ddac66f10dcab24c069812ec3aa39e62","points":[[0.5,0.5]],"width":37,"height":23,"frames":4,"frameRate":{"numerator":4,"denominator":1},"seedFrame":1,"trackedFrames":4,"forwardLostAt":null,"backwardLostAt":null})).unwrap()
    }
    fn original() -> Origin {
        serde_json::from_value(json!({"projectId":"project","compositionId":"root","clipId":"clip","snapshot":{"sourceId":"original","maskSourceId":"automatic","sourceHash":"a".repeat(64),"maskHash":"b".repeat(64),"maskFileHash":"c".repeat(64),"width":37,"height":23,"maskWidth":512,"maskHeight":320,"frames":4,"frameRate":{"numerator":4,"denominator":1},"corrections":[]}})).unwrap()
    }
    fn extended() -> Origin {
        let mut origin = original();
        origin.saved_snapshot = Some(origin.snapshot.clone());
        origin.snapshot.tracked_regions.push(region());
        origin
    }
    #[test]
    fn legacy_wire_and_exact_saved_baselines() {
        let legacy = original();
        legacy.validate().unwrap();
        assert!(serde_json::to_value(&legacy)
            .unwrap()
            .get("savedSnapshot")
            .is_none());
        let origin = extended();
        origin.validate().unwrap();
        assert!(origin.matches_saved(&legacy.snapshot));
        assert!(origin.matches_saved(&origin.snapshot));
        let mut changed = legacy.snapshot.clone();
        changed.mask_hash = "9".repeat(64);
        assert!(!origin.matches_saved(&changed));
    }
    #[test]
    fn extension_cannot_rewrite_source_or_remove_prior_regions() {
        let mut origin = extended();
        origin.snapshot.source_hash = "9".repeat(64);
        origin.snapshot.tracked_regions[0].source_hash = "9".repeat(64);
        assert!(origin.validate().is_err());
        let mut origin = extended();
        origin.saved_snapshot = Some(origin.snapshot.clone());
        origin.snapshot.tracked_regions.clear();
        assert!(origin.validate().is_err());
        let mut origin = extended();
        origin.saved_snapshot = Some(origin.snapshot.clone());
        origin.snapshot.tracked_regions[0].runtime_hash = "9".repeat(64);
        assert!(origin.validate().is_err());
    }
    #[test]
    fn refreshed_corrections_keep_session_inventory_and_reject_stale_access() {
        let mut origin = extended();
        let old = origin.saved_snapshot.clone().unwrap();
        let mut current = old.clone();
        current.corrections = serde_json::from_value(
            json!([{"frame":0,"correction":{"custom":false,"strokes":[],"feather":0.01}}]),
        )
        .unwrap();
        assert!(!origin.matches_saved(&current));
        origin.refresh(current.clone()).unwrap();
        assert!(origin.matches_saved(&current));
        assert!(!origin.matches_saved(&old));
        assert_eq!(origin.snapshot.corrections, current.corrections);
        assert_eq!(origin.snapshot.tracked_regions, vec![region()]);
    }
    #[test]
    fn persisted_inventory_collapses_baseline_and_rejects_unrelated_changes() {
        let mut origin = extended();
        let current = origin.snapshot.clone();
        origin.refresh(current.clone()).unwrap();
        assert!(origin.saved_snapshot.is_none());
        assert!(origin.matches_saved(&current));
        assert!(!origin.matches_saved(&original().snapshot));
        let before = serde_json::to_value(&origin).unwrap();
        let mut unrelated = current;
        unrelated.mask_hash = "9".repeat(64);
        assert!(origin.refresh(unrelated).is_err());
        assert_eq!(serde_json::to_value(&origin).unwrap(), before);
    }
}
