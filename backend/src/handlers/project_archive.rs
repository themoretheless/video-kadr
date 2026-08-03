use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, Response, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{Semaphore, SemaphorePermit};
use uuid::Uuid;

use crate::domain::portable_archive::{
    root_hash, ArchiveEntryKind, ArchiveLimits, PortableArchiveEntry, PortableArchiveManifest,
    PORTABLE_ARCHIVE_SCHEMA_VERSION,
};
use crate::error::{AppError, AppResult};
use crate::library::{now_secs, MediaEntry};
use crate::portable_container::{stage_vkadr, write_vkadr};
use crate::state::AppState;

pub const MAX_PROJECT_ARCHIVE_BODY: usize = 64 * 1024 * 1024;
static ARCHIVE_SEMAPHORE: Semaphore = Semaphore::const_new(1);

struct PermitBody {
    inner: Body,
    _permit: SemaphorePermit<'static>,
}

impl http_body::Body for PermitBody {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        Pin::new(&mut self.get_mut().inner).poll_frame(context)
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }
    fn size_hint(&self) -> http_body::SizeHint {
        self.inner.size_hint()
    }
}
const LIMITS: ArchiveLimits = ArchiveLimits {
    max_entries: 4096,
    max_entry_bytes: MAX_PROJECT_ARCHIVE_BODY as u64,
    max_total_bytes: MAX_PROJECT_ARCHIVE_BODY as u64,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportOptions {
    #[serde(default)]
    original_media: bool,
    #[serde(default)]
    proxies: bool,
}

pub async fn export_project_archive(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(options): Query<ExportOptions>,
) -> AppResult<Response<Body>> {
    let permit = ARCHIVE_SEMAPHORE
        .acquire()
        .await
        .map_err(|error| AppError::internal("acquire archive capacity", error))?;
    // Proxy export is intentionally fail-closed until the derived catalog can
    // provide provenance-complete artifacts for every requested entry.
    if options.proxies {
        return Err(AppError::bad_request(
            "Экспорт proxy в архив пока недоступен",
        ));
    }
    let envelope = state
        .db
        .get_project_document(&id)
        .await
        .map_err(|error| AppError::internal("read archived project", error))?
        .ok_or_else(|| AppError::not_found("Проект не найден"))?;
    let mut document = envelope.document;
    if references_lut(&document) {
        return Err(AppError::bad_request(
            "Проект использует LUT; перенос LUT пока не поддерживается",
        ));
    }
    let mut payloads: Vec<(PortableArchiveEntry, std::path::PathBuf)> = Vec::new();
    let mut archived_paths = BTreeSet::new();
    if options.original_media {
        for media in &mut document.media {
            let asset_id = media.asset_ref.as_deref().unwrap_or(&media.id);
            let entry = state.library.get(asset_id).await.ok_or_else(|| {
                AppError::bad_request(format!("Исходник {} недоступен", media.id))
            })?;
            let path = state.library.source_path(&entry).ok_or_else(|| {
                AppError::bad_request(format!("Медиа {} не является исходником", media.id))
            })?;
            let metadata = tokio::fs::symlink_metadata(&path)
                .await
                .map_err(|error| AppError::internal("stat archive media", error))?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(AppError::bad_request(format!(
                    "Исходник {} не является обычным файлом",
                    media.id
                )));
            }
            let sha = crate::artifacts::fingerprint_file(
                &state.cpu_pool,
                path.clone(),
                state.shutdown_token().child_token(),
            )
            .await
            .map_err(|error| AppError::internal("fingerprint archive media", error))?
            .sha256
            .to_string();
            media.content_fingerprint = Some(sha.clone());
            let extension = std::path::Path::new(&entry.filename)
                .extension()
                .and_then(|value| value.to_str())
                .filter(|value| {
                    !value.is_empty()
                        && value.len() <= 12
                        && value.bytes().all(|b| b.is_ascii_alphanumeric())
                })
                .unwrap_or("bin");
            let archive_path = format!("media/{sha}.{extension}");
            if archived_paths.insert(archive_path.clone()) {
                payloads.push((
                    PortableArchiveEntry {
                        path: archive_path,
                        kind: ArchiveEntryKind::Media,
                        size_bytes: metadata.len(),
                        sha256: sha.clone(),
                        proxy_provenance: None,
                    },
                    path,
                ));
            }
        }
    }
    let project = serde_json::to_value(&document).expect("project serializes");
    let project_bytes = serde_json::to_vec(&project).expect("project serializes");
    let project_sha = format!("{:x}", Sha256::digest(&project_bytes));
    let project_entry = PortableArchiveEntry {
        path: "project.json".into(),
        kind: ArchiveEntryKind::Project,
        size_bytes: project_bytes.len() as u64,
        sha256: project_sha,
        proxy_provenance: None,
    };
    let mut entries = vec![project_entry.clone()];
    entries.extend(payloads.iter().map(|(entry, _)| entry.clone()));
    let manifest = PortableArchiveManifest {
        schema_version: PORTABLE_ARCHIVE_SCHEMA_VERSION,
        root_hash: root_hash(PORTABLE_ARCHIVE_SCHEMA_VERSION, &project, &entries)
            .map_err(|error| AppError::internal("hash project archive", error))?,
        project,
        entries,
    };
    let stage = state
        .storage
        .join("archive-exports")
        .join(Uuid::new_v4().to_string());
    tokio::fs::create_dir_all(&stage)
        .await
        .map_err(|error| AppError::internal("stage archive export", error))?;
    let archive_result = async {
        tokio::fs::write(stage.join("project.json"), project_bytes).await?;
        for (entry, source) in payloads {
            let destination = stage.join(&entry.path);
            tokio::fs::create_dir_all(destination.parent().unwrap()).await?;
            tokio::fs::copy(source, destination).await?;
        }
        let mut archive = Vec::new();
        write_vkadr(&mut archive, &manifest, &stage, LIMITS)?;
        Ok::<_, anyhow::Error>(archive)
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&stage).await;
    let archive = archive_result
        .map_err(|error| AppError::internal("stage or encode project archive", error))?;
    let mut response = Response::new(Body::new(PermitBody {
        inner: Body::from(archive),
        _permit: permit,
    }));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/vnd.vkadr.project"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"project-{id}.vkadr\""))
            .map_err(|_| AppError::bad_request("Некорректный project id"))?,
    );
    Ok(response)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportArchiveResponse {
    project_id: String,
    revision: u64,
    missing_media: Vec<String>,
}

pub async fn import_project_archive(
    State(state): State<AppState>,
    body: Bytes,
) -> AppResult<(StatusCode, Json<ImportArchiveResponse>)> {
    let _permit = ARCHIVE_SEMAPHORE
        .acquire()
        .await
        .map_err(|error| AppError::internal("acquire archive capacity", error))?;
    if body.is_empty() || body.len() > MAX_PROJECT_ARCHIVE_BODY {
        return Err(AppError::bad_request("Некорректный размер архива"));
    }
    let import_root = state.storage.join("archive-imports");
    tokio::fs::create_dir_all(&import_root)
        .await
        .map_err(|error| AppError::internal("create archive import root", error))?;
    let stage = import_root.join(Uuid::new_v4().to_string());
    let stage_for_decode = stage.clone();
    let manifest = tokio::task::spawn_blocking(move || {
        stage_vkadr(Cursor::new(body), &stage_for_decode, LIMITS)
    })
    .await
    .map_err(|error| AppError::internal("join archive decoder", error))?
    .map_err(|error| AppError::bad_request(error.to_string()))?;
    let mut document = match manifest.validate(LIMITS) {
        Ok(document) => document,
        Err(error) => {
            let _ = tokio::fs::remove_dir_all(&stage).await;
            return Err(AppError::bad_request(error.to_string()));
        }
    };
    if manifest
        .entries
        .iter()
        .any(|entry| entry.kind == ArchiveEntryKind::Lut)
        || references_lut(&document)
    {
        let _ = tokio::fs::remove_dir_all(&stage).await;
        return Err(AppError::bad_request(
            "Импорт LUT из архива пока не поддерживается",
        ));
    }
    let embedded: BTreeMap<String, &PortableArchiveEntry> = manifest
        .entries
        .iter()
        .filter(|entry| entry.kind == ArchiveEntryKind::Media)
        .map(|entry| (entry.sha256.clone(), entry))
        .collect();
    let local_by_fingerprint: BTreeMap<String, String> = state
        .library
        .list()
        .await
        .into_iter()
        .filter(|entry| entry.kind == "source")
        .filter_map(|entry| entry.fingerprint.map(|sha| (sha, entry.id)))
        .collect();
    let mut missing = Vec::new();
    let mut added = Vec::new();
    let mut imported_by_fingerprint = local_by_fingerprint;
    for media in &mut document.media {
        let Some(sha) = media.content_fingerprint.clone() else {
            media.asset_ref = None;
            missing.push(media.id.clone());
            continue;
        };
        let Some(entry) = embedded.get(&sha) else {
            if let Some(asset_ref) = imported_by_fingerprint.get(&sha) {
                media.asset_ref = Some(asset_ref.clone());
            } else {
                media.asset_ref = None;
                missing.push(media.id.clone());
            }
            continue;
        };
        if let Some(asset_ref) = imported_by_fingerprint.get(&sha) {
            media.asset_ref = Some(asset_ref.clone());
            continue;
        }
        let asset_id = Uuid::new_v4().to_string();
        let extension = entry
            .path
            .rsplit_once('.')
            .map(|(_, ext)| ext)
            .unwrap_or("bin");
        let filename = format!("{asset_id}.{extension}");
        let destination = state.storage.join("sources").join(&filename);
        if let Err(error) = tokio::fs::copy(stage.join(&entry.path), &destination).await {
            let _ = tokio::fs::remove_file(&destination).await;
            rollback_media(&state, &added).await;
            let _ = tokio::fs::remove_dir_all(&stage).await;
            return Err(AppError::internal("publish imported media", error));
        }
        let library_entry = MediaEntry {
            id: asset_id.clone(),
            kind: "source".into(),
            filename: filename.clone(),
            url: format!("/files/sources/{filename}"),
            title: None,
            duration: None,
            width: None,
            height: None,
            fps: None,
            vcodec: None,
            acodec: None,
            media_kind: Some(media.kind.clone()),
            size_bytes: Some(entry.size_bytes),
            fingerprint: Some(sha.clone()),
            color_management: None,
            created_at: now_secs(),
        };
        if !state.library.add(library_entry).await {
            let _ = tokio::fs::remove_file(destination).await;
            rollback_media(&state, &added).await;
            let _ = tokio::fs::remove_dir_all(&stage).await;
            return Err(AppError::internal(
                "publish imported media catalog",
                anyhow::anyhow!("library add failed"),
            ));
        }
        media.asset_ref = Some(asset_id.clone());
        imported_by_fingerprint.insert(sha, asset_id.clone());
        added.push(asset_id);
    }
    let project_id = Uuid::new_v4().to_string();
    let envelope = match state
        .db
        .cas_upsert_project_document(&project_id, 0, &document)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            rollback_media(&state, &added).await;
            let _ = tokio::fs::remove_dir_all(&stage).await;
            return Err(AppError::internal("publish imported project", error));
        }
    };
    let _ = tokio::fs::remove_dir_all(&stage).await;
    Ok((
        StatusCode::CREATED,
        Json(ImportArchiveResponse {
            project_id: envelope.project_id,
            revision: envelope.revision,
            missing_media: missing,
        }),
    ))
}

async fn rollback_media(state: &AppState, ids: &[String]) {
    for id in ids {
        let _ = state.library.remove(id).await;
    }
}

fn references_lut(document: &crate::domain::project::ProjectDocument) -> bool {
    fn contains(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(object) => {
                object
                    .get("lutId")
                    .is_some_and(|value| value.as_str().is_some_and(|id| !id.trim().is_empty()))
                    || object.get("lut").is_some_and(|value| {
                        value
                            .as_object()
                            .and_then(|lut| lut.get("id"))
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|id| !id.trim().is_empty())
                    })
                    || object.values().any(contains)
            }
            serde_json::Value::Array(values) => values.iter().any(contains),
            _ => false,
        }
    }
    contains(&serde_json::to_value(document).expect("project serializes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::{ProjectDocument, ProjectEffect};
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn canonical_project_lut_reference_is_rejected() {
        let mut document = ProjectDocument::from_legacy(
            "lut",
            "video",
            json!({"id":"video","duration":1}),
            json!({}),
        )
        .unwrap();
        document.sequences[0].tracks[0].clips[0]
            .effects
            .push(ProjectEffect {
                id: "lut-effect".into(),
                kind: "color".into(),
                enabled: true,
                parameters: json!({"lutId":"private-lut"}),
                extra: BTreeMap::new(),
            });
        assert!(references_lut(&document));
        document.sequences[0].tracks[0].clips[0].effects[0].parameters =
            json!({"legacy":{"lut":{"id":"legacy-lut"}}});
        assert!(references_lut(&document));
    }

    #[tokio::test]
    async fn response_body_holds_aggregate_archive_permit_until_drop() {
        let permit = ARCHIVE_SEMAPHORE.acquire().await.unwrap();
        let body = Body::new(PermitBody {
            inner: Body::from(Bytes::from_static(b"archive")),
            _permit: permit,
        });
        assert!(ARCHIVE_SEMAPHORE.try_acquire().is_err());
        drop(body);
        assert!(ARCHIVE_SEMAPHORE.try_acquire().is_ok());
    }
}
