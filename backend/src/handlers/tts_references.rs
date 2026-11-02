//! Private bounded voice samples and encrypted narration reference snapshots.
use crate::{
    error::{AppError, AppResult},
    services::local_tts::{self, SynthesisRequest},
    state::AppState,
};
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Submission {
    text: String,
    voice_id: u32,
    speed: f32,
    #[serde(default)]
    num_steps: Option<u32>,
    #[serde(default)]
    pauses: Option<f32>,
    #[serde(default)]
    reference_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReferenceSnapshot {
    pub id: String,
    pub hash: String,
    pub bytes: u64,
    #[serde(default)]
    pub source_id: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PinnedInput {
    pub request: SynthesisRequest,
    #[serde(default)]
    pub reference: Option<ReferenceSnapshot>,
}
impl PinnedInput {
    pub fn decode(schema: u32, bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(if schema == 1 {
            Self {
                request: serde_json::from_slice(bytes)?,
                reference: None,
            }
        } else {
            anyhow::ensure!(schema == 2, "unsupported narration schema");
            serde_json::from_slice(bytes)?
        })
    }
}
impl Submission {
    pub(crate) async fn pin(
        self,
        state: &AppState,
        owner: &str,
        headers: &HeaderMap,
    ) -> AppResult<PinnedInput> {
        let request = SynthesisRequest {
            pauses: self.pauses,
            num_steps: self.num_steps,
            text: self.text,
            voice_id: self.voice_id,
            speed: self.speed,
        };
        request
            .validate()
            .map_err(|_| AppError::bad_request("Проверьте текст, голос и скорость озвучивания"))?;
        if request.pauses.is_some() && state.kokoro.is_none() {
            return Err(AppError::service_unavailable(
                "Для пауз настройте нативный движок Kokoro",
            ));
        }
        if request.voice_id == 2000 {
            if self.reference_id.is_some() {
                return Err(AppError::bad_request(
                    "Готовый голос не принимает пользовательский образец",
                ));
            }
            let config = super::tts_models::config_for_owner(state, owner).await?.0;
            request
                .validate_for_model(&config)
                .map_err(|_| AppError::bad_request("Голос не принадлежит выбранной модели"))?;
            let reference = stock_bria(state, owner, headers).await?;
            return Ok(PinnedInput {
                request,
                reference: Some(reference),
            });
        }
        if (matches!(request.voice_id, 1099 | 2099)) != self.reference_id.is_some() {
            return Err(AppError::bad_request(
                "Для голоса по образцу нужен загруженный образец; штатные голоса его не используют",
            ));
        }
        let reference = if let Some(id) = self.reference_id {
            validate_id(&id)?;
            let row: Option<(String, i64)> =
                sqlx::query_as("SELECT sha256,bytes FROM tts_references WHERE id=? AND owner=? AND NOT EXISTS(SELECT 1 FROM tts_reference_removals r WHERE r.reference_id=tts_references.id)")
                    .bind(&id)
                    .bind(owner)
                    .fetch_optional(state.db.pool())
                    .await
                    .map_err(|e| AppError::internal("read voice reference", e))?;
            let (hash, bytes) =
                row.ok_or_else(|| AppError::not_found("Образец голоса недоступен"))?;
            let source_id: Option<String> = sqlx::query_scalar(
                "SELECT source_id FROM tts_reference_sources WHERE reference_id=?",
            )
            .bind(&id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read reference origin", e))?;
            let snapshot = ReferenceSnapshot {
                id,
                hash,
                bytes: bytes as u64,
                source_id,
            };
            snapshot.resolve(state, owner).await?;
            Some(snapshot)
        } else {
            None
        };
        Ok(PinnedInput { request, reference })
    }
}
fn validate_id(id: &str) -> AppResult<()> {
    if uuid::Uuid::parse_str(id)
        .ok()
        .is_none_or(|v| v.to_string() != id)
    {
        return Err(AppError::bad_request("Некорректный образец голоса"));
    }
    Ok(())
}
impl ReferenceSnapshot {
    pub(crate) async fn authorize_origin(&self, state: &AppState, owner: &str) -> AppResult<()> {
        if let Some(source) = &self.source_id {
            if !state
                .library
                .get(source)
                .await
                .is_some_and(|e| e.kind == "source")
            {
                return Err(AppError::not_found("Исходник образца недоступен"));
            }
            if let Some(space) = state
                .db
                .source_space_id(source)
                .await
                .map_err(|e| AppError::internal("read reference source scope", e))?
            {
                if !state
                    .db
                    .is_space_member(&space, owner)
                    .await
                    .map_err(|e| AppError::internal("authorize reference source", e))?
                {
                    return Err(AppError::forbidden("Доступ к исходнику образца отозван"));
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn resolve(
        &self,
        state: &AppState,
        owner: &str,
    ) -> AppResult<std::path::PathBuf> {
        validate_id(&self.id)?;
        if self.hash.len() != 64
            || !self
                .hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !(24044..=480044).contains(&self.bytes)
        {
            return Err(AppError::bad_request("Повреждён снимок образца голоса"));
        }
        let owned:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tts_references WHERE id=? AND owner=? AND sha256=? AND bytes=?)").bind(&self.id).bind(owner).bind(&self.hash).bind(self.bytes as i64).fetch_one(state.db.pool()).await.map_err(|e|AppError::internal("authorize voice reference",e))?;
        if !owned {
            return Err(AppError::not_found("Образец голоса недоступен"));
        }
        self.authorize_origin(state, owner).await?;
        let root = crate::services::tts_model_download::directory(
            &state.storage,
            std::path::Path::new("tts-references"),
        )
        .await
        .map_err(|_| AppError::service_unavailable("Хранилище образцов недоступно"))?;
        let path = root.join(format!("{}.wav", self.id));
        let meta = tokio::fs::symlink_metadata(&path)
            .await
            .map_err(|_| AppError::service_unavailable("Образец голоса отсутствует"))?;
        if !meta.file_type().is_file() || meta.len() != self.bytes {
            return Err(AppError::service_unavailable("Образец голоса изменён"));
        }
        let (bytes, hash) = crate::services::local_speech::fingerprint_model(&path)
            .await
            .map_err(|_| AppError::service_unavailable("Не удалось проверить образец"))?;
        if bytes != self.bytes || hash != self.hash {
            return Err(AppError::service_unavailable("Образец голоса изменён"));
        }
        Ok(path)
    }
}
struct OwnedFile(std::path::PathBuf, bool);
impl Drop for OwnedFile {
    fn drop(&mut self) {
        if !self.1 {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}
pub async fn upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Body,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let _slot = state
        .try_acquire_upload_slot()
        .ok_or_else(|| AppError::too_many_requests("Повторите загрузку образца позже"))?;
    let input = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        to_bytes(body, 8 * 1024 * 1024),
    )
    .await
    .map_err(|_| AppError::bad_request("Загрузка образца превысила время ожидания"))?
    .map_err(|_| AppError::bad_request("Образец превышает 8 МиБ"))?;
    let (wav, duration_us) =
        tokio::task::spawn_blocking(move || local_tts::normalize_reference(input.to_vec()))
            .await
            .map_err(|e| AppError::internal("join reference preparation", e))?
            .map_err(|_| {
                AppError::bad_request(
            "Нужен WAV: минимум 0,5 секунды, 1–2 канала, 8–48 кГц. Используются первые 10 секунд.",
        )
            })?;
    persist(&state, &headers, &owner, wav, duration_us, None, false).await
}
pub(crate) struct ReferenceOrigin {
    pub source_id: String,
    pub source_hash: String,
    pub start_us: u64,
    pub end_us: u64,
}
pub(crate) async fn persist(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    wav: Vec<u8>,
    duration_us: u64,
    origin: Option<ReferenceOrigin>,
    hidden: bool,
) -> AppResult<(StatusCode, Json<Value>)> {
    let hash = format!("{:x}", Sha256::digest(&wav));
    let size = wav.len();
    let id = uuid::Uuid::new_v4().to_string();
    let root = crate::services::tts_model_download::directory(
        &state.storage,
        std::path::Path::new("tts-references"),
    )
    .await
    .map_err(|e| AppError::internal("prepare reference storage", e))?;
    let path = root.join(format!("{id}.wav"));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
        .map_err(|e| AppError::internal("create voice reference", e))?;
    let mut owned = OwnedFile(path, false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|e| AppError::internal("protect voice reference", e))?;
    }
    file.write_all(&wav)
        .await
        .map_err(|e| AppError::internal("write voice reference", e))?;
    file.sync_all()
        .await
        .map_err(|e| AppError::internal("sync voice reference", e))?;
    drop(file);
    if super::publish::authenticated_actor(&state, &headers).await? != owner {
        return Err(AppError::unauthorized("Сессия изменилась"));
    }
    if let Some(origin) = &origin {
        super::library::authorize_source_access(state, &origin.source_id, headers).await?;
    }
    let mut tx = state
        .db
        .pool()
        .begin()
        .await
        .map_err(|e| AppError::internal("begin reference registration", e))?;
    sqlx::query("INSERT INTO tts_references(id,owner,sha256,bytes) VALUES(?,?,?,?)")
        .bind(&id)
        .bind(&owner)
        .bind(&hash)
        .bind(size as i64)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal("register voice reference", e))?;
    if let Some(origin) = origin {
        sqlx::query("INSERT INTO tts_reference_sources(reference_id,source_id,source_hash,start_us,end_us) VALUES(?,?,?,?,?)").bind(&id).bind(origin.source_id).bind(origin.source_hash).bind(origin.start_us as i64).bind(origin.end_us as i64).execute(&mut *tx).await.map_err(|e|AppError::internal("register reference origin",e))?;
    }
    if hidden {
        sqlx::query("INSERT INTO tts_reference_removals(reference_id) VALUES(?)")
            .bind(&id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal("hide stock voice reference", e))?;
    }
    tx.commit()
        .await
        .map_err(|e| AppError::internal("commit reference registration", e))?;
    owned.1 = true;
    Ok((
        StatusCode::CREATED,
        Json(json!({"id":id,"durationUs":duration_us,"sampleRate":24000})),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_requests_and_pinned_snapshots_decode_without_plaintext_shape_confusion() {
        let request = SynthesisRequest {
            pauses: None,
            num_steps: None,
            text: "legacy".into(),
            voice_id: 3,
            speed: 1.,
        };
        let bytes = serde_json::to_vec(&request).unwrap();
        assert!(PinnedInput::decode(1, &bytes).unwrap().reference.is_none());
        assert!(PinnedInput::decode(2, &bytes).is_err());
        let pinned = PinnedInput {
            request,
            reference: Some(ReferenceSnapshot {
                id: uuid::Uuid::new_v4().to_string(),
                hash: "a".repeat(64),
                bytes: 48044,
                source_id: None,
            }),
        };
        let bytes = serde_json::to_vec(&pinned).unwrap();
        assert!(PinnedInput::decode(1, &bytes).is_err());
        assert!(PinnedInput::decode(2, &bytes).unwrap().reference.is_some());
        let mut quality = pinned;
        quality.request.voice_id = 1099;
        quality.request.num_steps = Some(8);
        let restored = PinnedInput::decode(2, &serde_json::to_vec(&quality).unwrap()).unwrap();
        assert_eq!(restored.request.num_steps, Some(8));
        assert_eq!(
            restored.reference.unwrap().id,
            quality.reference.unwrap().id
        );
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferencePage {
    cursor: Option<String>,
    limit: Option<u32>,
}
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(page): axum::extract::Query<ReferencePage>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    {
        let _operation = state.tts_model_ops.lock().await;
        collect_removed(&state, &owner).await;
    }
    let limit = page.limit.unwrap_or(20);
    if !(1..=50).contains(&limit) {
        return Err(AppError::bad_request("Лимит должен быть от 1 до 50"));
    }
    if let Some(cursor) = &page.cursor {
        validate_id(cursor)?;
    }
    let rows:Vec<(String,i64,Option<String>)>=sqlx::query_as("SELECT r.id,r.bytes,s.source_id FROM tts_references r LEFT JOIN tts_reference_sources s ON s.reference_id=r.id WHERE r.owner=? AND (? IS NULL OR r.id>?) AND NOT EXISTS(SELECT 1 FROM tts_reference_removals d WHERE d.reference_id=r.id) ORDER BY r.id LIMIT ?").bind(&owner).bind(&page.cursor).bind(&page.cursor).bind(limit as i64+1).fetch_all(state.db.pool()).await.map_err(|e|AppError::internal("list private references",e))?;
    let more = rows.len() > limit as usize;
    let mut references = Vec::new();
    for (id, bytes, source_id) in rows.into_iter().take(limit as usize) {
        let snapshot = ReferenceSnapshot {
            id: id.clone(),
            hash: String::new(),
            bytes: bytes as u64,
            source_id: source_id.clone(),
        };
        let available = snapshot.authorize_origin(&state, &owner).await.is_ok();
        references.push(json!({"id":id,"durationUs":(bytes.saturating_sub(44) as u64)*1_000_000/48000,"sampleRate":24000,"origin":if source_id.is_some(){"clip"}else{"file"},"available":available}));
    }
    let cursor = if more {
        references.last().and_then(|r| r.get("id")).cloned()
    } else {
        None
    };
    Ok(Json(json!({"references":references,"nextCursor":cursor})))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoveReference {
    reference_id: String,
}
pub async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    crate::error::ApiJson(request): crate::error::ApiJson<RemoveReference>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    validate_id(&request.reference_id)?;
    let _operation = state.tts_model_ops.lock().await;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tts_references WHERE id=? AND owner=?)")
            .bind(&request.reference_id)
            .bind(&owner)
            .fetch_one(state.db.pool())
            .await
            .map_err(|e| AppError::internal("authorize reference removal", e))?;
    if !exists {
        return Err(AppError::not_found("Образец голоса недоступен"));
    }
    sqlx::query("INSERT OR IGNORE INTO tts_reference_removals(reference_id) VALUES(?)")
        .bind(&request.reference_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| AppError::internal("hide private reference", e))?;
    collect_removed(&state, &owner).await;
    let retained = match tokio::fs::symlink_metadata(
        state
            .storage
            .join("tts-references")
            .join(format!("{}.wav", request.reference_id)),
    )
    .await
    {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => true,
    };
    Ok(Json(json!({"removed":true,"filesRetained":retained})))
}

/// Logical removal commits first. Collection failures retain files and retry on
/// the next owner refresh; malformed retained work pins all removed references.
async fn collect_removed(state: &AppState, owner: &str) {
    let Ok(pinned) = super::tts_jobs::retained_references(state).await else {
        return;
    };
    let rows:Result<Vec<(String,String,i64)>,_>=sqlx::query_as("SELECT r.id,r.sha256,r.bytes FROM tts_references r JOIN tts_reference_removals d ON d.reference_id=r.id WHERE r.owner=?").bind(owner).fetch_all(state.db.pool()).await;
    let Ok(rows) = rows else { return };
    for (id, hash, bytes) in rows {
        if pinned.contains(&id) {
            continue;
        }
        let _ = release_reference(state, &id, &hash, bytes).await;
    }
}
async fn release_reference(
    state: &AppState,
    id: &str,
    hash: &str,
    bytes: i64,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        uuid::Uuid::parse_str(id)?.to_string() == id,
        "invalid reference UUID"
    );
    anyhow::ensure!((24044..=480044).contains(&bytes), "invalid reference size");
    let root = crate::services::tts_model_download::directory(
        &state.storage,
        std::path::Path::new("tts-references"),
    )
    .await?;
    let path = root.join(format!("{id}.wav"));
    let meta = match tokio::fs::symlink_metadata(&path).await {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    anyhow::ensure!(
        meta.file_type().is_file() && meta.len() == bytes as u64,
        "reference file changed"
    );
    let wav = tokio::fs::read(&path).await?;
    anyhow::ensure!(
        wav.len() == bytes as usize && format!("{:x}", Sha256::digest(&wav)) == hash,
        "reference checksum changed"
    );
    tokio::fs::remove_file(path).await?;
    Ok(())
}

/// Called under admission/removal lock. Frozen owned bytes keep accepted jobs
/// independent of later Pocket removal; internal samples never enter the UI list.
async fn stock_bria(
    state: &AppState,
    owner: &str,
    headers: &HeaderMap,
) -> AppResult<ReferenceSnapshot> {
    if !super::tts_models::chatterbox_stock_available(state, owner).await? {
        return Err(AppError::bad_request(
            "Для готового голоса Bria установите Pocket TTS в свою библиотеку",
        ));
    }
    collect_removed(state, owner).await;
    let catalog =
        crate::services::tts_model_catalog::ModelCatalog::find(super::tts_models::POCKET_ID)
            .map_err(|e| AppError::internal("read stock voice catalog", e))?
            .ok_or_else(|| AppError::service_unavailable("Каталог Pocket недоступен"))?;
    let file = catalog
        .files
        .iter()
        .find(|f| f.path == "test_wavs/bria.wav")
        .ok_or_else(|| AppError::service_unavailable("Запись Bria недоступна"))?;
    let path = state
        .storage
        .join("models/tts")
        .join(&catalog.id)
        .join(&file.path);
    let meta = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| AppError::service_unavailable("Запись Bria недоступна"))?;
    if !meta.file_type().is_file() || meta.len() != file.bytes || file.bytes > 8 * 1024 * 1024 {
        return Err(AppError::service_unavailable("Запись Bria повреждена"));
    }
    let raw = tokio::fs::read(&path)
        .await
        .map_err(|e| AppError::internal("read stock voice", e))?;
    if raw.len() as u64 != file.bytes || format!("{:x}", Sha256::digest(&raw)) != file.sha256 {
        return Err(AppError::service_unavailable("Запись Bria изменилась"));
    }
    let (wav, duration) = local_tts::normalize_reference(raw)
        .map_err(|_| AppError::service_unavailable("Некорректная запись Bria"))?;
    if duration < 5000000 {
        return Err(AppError::service_unavailable(
            "Запись Bria слишком короткая",
        ));
    }
    let hash = format!("{:x}", Sha256::digest(&wav));
    let bytes = wav.len() as u64;
    let (_, Json(saved)) = persist(state, headers, owner, wav, duration, None, true).await?;
    let id = saved["id"]
        .as_str()
        .ok_or_else(|| AppError::service_unavailable("Образец Bria не сохранён"))?
        .to_owned();
    Ok(ReferenceSnapshot {
        id,
        hash,
        bytes,
        source_id: None,
    })
}
