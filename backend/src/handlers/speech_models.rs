//! Owner-scoped immutable Whisper model imports and persisted selection.
use crate::{
    error::{ApiJson, AppError, AppResult},
    services::local_speech::SpeechConfig,
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Query, State},
    http::HeaderMap,
    Json,
};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
const MAX_MODEL_BYTES: u64 = 4_000_000_000;
struct Staged(std::path::PathBuf);
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) async fn optional_owner(
    state: &AppState,
    headers: &HeaderMap,
) -> AppResult<Option<String>> {
    let session = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .any(|v| v.trim().starts_with("video_kadr_session="))
        });
    if headers.contains_key("authorization") || session {
        Ok(Some(
            super::publish::authenticated_actor(state, headers).await?,
        ))
    } else {
        Ok(None)
    }
}
pub(super) async fn config_for_snapshot(
    state: &AppState,
    owner: &str,
    id: Option<&str>,
) -> AppResult<SpeechConfig> {
    let mut config = (*state.speech).clone();
    let Some(id) = id else { return Ok(config) };
    if !valid_id(id) {
        return Err(AppError::bad_request("Некорректная модель"));
    }
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT bytes FROM speech_models WHERE owner=? AND id=?")
            .bind(owner)
            .bind(id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("load speech model", e))?;
    let expected = exists.ok_or_else(|| AppError::not_found("Модель не найдена"))?;
    let path = state
        .storage
        .join("models/whisper")
        .join(format!("{id}.bin"));
    let (size, hash) = crate::services::local_speech::fingerprint_model(&path)
        .await
        .map_err(|_| AppError::not_found("Файл модели недоступен"))?;
    if size != expected as u64 || hash != id {
        return Err(AppError::bad_request(
            "Файл модели изменился. Импортируйте модель заново.",
        ));
    }
    config.model = Some(path);
    Ok(config)
}
pub(super) async fn config_for_owner(
    state: &AppState,
    owner: Option<&str>,
) -> AppResult<(SpeechConfig, Option<String>)> {
    let id: Option<String> = if let Some(owner) = owner {
        sqlx::query_scalar("SELECT s.model_id FROM speech_model_selection s WHERE s.owner=? AND NOT EXISTS(SELECT 1 FROM speech_model_removals r WHERE r.owner=s.owner AND r.id=s.model_id)")
            .bind(owner)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("load model selection", e))?
    } else {
        None
    };
    let config = if let Some(id) = &id {
        config_for_snapshot(state, owner.expect("owner selection"), Some(id)).await?
    } else {
        (*state.speech).clone()
    };
    Ok((config, id))
}
pub async fn list(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    if let Err(error) = gc_removed(&state).await {
        tracing::warn!(%error,"removed model cleanup will retry");
    }
    let selected: Option<String> =
        sqlx::query_scalar("SELECT s.model_id FROM speech_model_selection s WHERE s.owner=? AND NOT EXISTS(SELECT 1 FROM speech_model_removals r WHERE r.owner=s.owner AND r.id=s.model_id)")
            .bind(&owner)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("load model selection", e))?;
    let rows: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT m.id,m.name,m.bytes FROM speech_models m WHERE m.owner=? AND NOT EXISTS(SELECT 1 FROM speech_model_removals r WHERE r.owner=m.owner AND r.id=m.id) ORDER BY m.name,m.id")
            .bind(&owner)
            .fetch_all(state.db.pool())
            .await
            .map_err(|e| AppError::internal("list speech models", e))?;
    let mut models = vec![];
    for (id, name, bytes) in rows {
        let path = state
            .storage
            .join("models/whisper")
            .join(format!("{id}.bin"));
        let available = tokio::fs::metadata(path)
            .await
            .is_ok_and(|m| m.is_file() && m.len() == bytes as u64);
        let in_use = in_use(&state, &id)
            .await
            .map_err(|e| AppError::internal("read model usage", e))?;
        models
            .push(json!({"id":id,"name":name,"bytes":bytes,"available":available,"inUse":in_use}));
    }
    let configured = if let Some(path) = &state.speech.model {
        tokio::fs::metadata(path).await.ok().filter(|m|m.is_file()).map(|m|json!({"id":"configured","name":path.file_name().map(|v|v.to_string_lossy().into_owned()),"bytes":m.len(),"available":true}))
    } else {
        None
    };
    Ok(Json(
        json!({"models":models,"configured":configured,"selectedId":selected.unwrap_or_else(||"configured".into()),"maxBytes":MAX_MODEL_BYTES}),
    ))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Select {
    model_id: String,
}
pub async fn select(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Select>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let _operation = state.speech_model_ops.lock().await;
    if request.model_id == "configured" {
        sqlx::query("DELETE FROM speech_model_selection WHERE owner=?")
            .bind(&owner)
            .execute(state.db.pool())
            .await
            .map_err(|e| AppError::internal("reset speech model", e))?;
    } else {
        let removed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM speech_model_removals WHERE owner=? AND id=?)",
        )
        .bind(&owner)
        .bind(&request.model_id)
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| AppError::internal("read model removal", e))?;
        if removed {
            return Err(AppError::not_found("Модель удалена"));
        }
        config_for_snapshot(&state, &owner, Some(&request.model_id)).await?;
        sqlx::query("INSERT INTO speech_model_selection(owner,model_id) VALUES(?,?) ON CONFLICT(owner) DO UPDATE SET model_id=excluded.model_id").bind(&owner).bind(&request.model_id).execute(state.db.pool()).await.map_err(|e|AppError::internal("select speech model",e))?;
    }
    Ok(Json(json!({"selectedId":request.model_id})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportQuery {
    name: String,
}
pub async fn import(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ImportQuery>,
    body: Body,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    if headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .is_some_and(|n| n > MAX_MODEL_BYTES)
    {
        return Err(AppError::bad_request("Модель превышает 4 ГБ"));
    }
    let _permit = state
        .try_acquire_upload_slot()
        .ok_or_else(|| AppError::too_many_requests("Импорт уже занят"))?;
    install_model(&state, owner, query.name, body, None, None).await
}
pub(super) async fn install_model(
    state: &AppState,
    owner: String,
    name: String,
    body: Body,
    expected: Option<(String, u64)>,
    progress: Option<tokio::sync::mpsc::UnboundedSender<f64>>,
) -> AppResult<Json<Value>> {
    if name.trim().is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
        return Err(AppError::bad_request("Укажите название модели до 256 байт"));
    }
    let root = state.storage.join("models/whisper");
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|e| AppError::internal("create model directory", e))?;
    let staged = Staged(root.join(format!("{}.upload", uuid::Uuid::new_v4())));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged.0)
        .await
        .map_err(|e| AppError::internal("create model import", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&staged.0, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|e| AppError::internal("protect model import", e))?;
    }
    let mut size = 0u64;
    let mut hash = Sha256::new();
    let mut prefix = Vec::new();
    let mut stream = body.into_data_stream();
    let mut progress_at = std::time::Instant::now();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| AppError::bad_request("Не удалось прочитать модель"))?;
        size = size
            .checked_add(chunk.len() as u64)
            .filter(|s| *s <= MAX_MODEL_BYTES)
            .ok_or_else(|| AppError::bad_request("Модель превышает 4 ГБ"))?;
        if let Some((_, expected_bytes)) = &expected {
            if size > *expected_bytes {
                return Err(AppError::bad_request(
                    "Размер скачанной модели не совпадает с каталогом",
                ));
            }
            if progress_at.elapsed() >= std::time::Duration::from_millis(200) {
                if let Some(progress) = &progress {
                    let _ = progress.send((size as f64 / *expected_bytes as f64 * 90.).min(90.));
                }
                progress_at = std::time::Instant::now();
            }
        }
        hash.update(&chunk);
        if prefix.len() < 48 {
            prefix.extend_from_slice(&chunk[..chunk.len().min(48 - prefix.len())]);
        }
        file.write_all(&chunk)
            .await
            .map_err(|e| AppError::internal("write model import", e))?;
    }
    if size < 1_000_000 || prefix.len() < 48 || &prefix[..4] != b"lmgg" {
        return Err(AppError::bad_request("Нужна модель Whisper GGML"));
    }
    let vocab = u32::from_le_bytes(prefix[4..8].try_into().expect("header length"));
    if !(10_000..=100_000).contains(&vocab) {
        return Err(AppError::bad_request(
            "Некорректный заголовок модели Whisper",
        ));
    }
    file.sync_all()
        .await
        .map_err(|e| AppError::internal("sync model import", e))?;
    drop(file);
    let id = hash
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if let Some((expected_hash, expected_bytes)) = &expected {
        if id != *expected_hash || size != *expected_bytes {
            return Err(AppError::bad_request(
                "Контрольная сумма скачанной модели не совпадает с каталогом",
            ));
        }
    }
    if let Some(progress) = &progress {
        let _ = progress.send(95.);
    }
    let destination = root.join(format!("{id}.bin"));
    let _operation = state.speech_model_ops.lock().await;
    match tokio::fs::hard_link(&staged.0, &destination).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if crate::services::local_speech::fingerprint_model(&destination)
                .await
                .map_err(|e| AppError::internal("verify existing model", e))?
                .1
                != id
            {
                return Err(AppError::conflict("Сохранённая модель повреждена"));
            }
        }
        Err(e) => return Err(AppError::internal("publish model", e)),
    }
    #[cfg(unix)]
    {
        let root = root.clone();
        tokio::task::spawn_blocking(move || std::fs::File::open(root)?.sync_all())
            .await
            .map_err(|e| AppError::internal("join model sync", e))?
            .map_err(|e| AppError::internal("sync model directory", e))?;
    }
    let mut transaction = state
        .db
        .pool()
        .begin()
        .await
        .map_err(|e| AppError::internal("begin model registration", e))?;
    sqlx::query("INSERT INTO speech_models(owner,id,name,bytes) VALUES(?,?,?,?) ON CONFLICT(owner,id) DO UPDATE SET name=excluded.name").bind(&owner).bind(&id).bind(name.trim()).bind(size as i64).execute(&mut *transaction).await.map_err(|e|AppError::internal("register speech model",e))?;
    sqlx::query("DELETE FROM speech_model_removals WHERE owner=? AND id=?")
        .bind(&owner)
        .bind(&id)
        .execute(&mut *transaction)
        .await
        .map_err(|e| AppError::internal("restore model registration", e))?;
    transaction
        .commit()
        .await
        .map_err(|e| AppError::internal("commit model registration", e))?;
    Ok(Json(
        json!({"id":id,"name":name.trim(),"bytes":size,"available":true}),
    ))
}

/// Pin synchronous work; durable work is pinned by its persisted pending/running request.
pub(super) struct ModelUse {
    state: AppState,
    id: Option<String>,
    pin: bool,
}
impl ModelUse {
    pub(super) fn new(state: &AppState, id: Option<String>, pin: bool) -> Self {
        if pin {
            if let Some(id) = &id {
                *state
                    .speech_model_pins
                    .lock()
                    .expect("model pins poisoned")
                    .entry(id.clone())
                    .or_default() += 1;
            }
        }
        Self {
            state: state.clone(),
            id,
            pin,
        }
    }
}
impl Drop for ModelUse {
    fn drop(&mut self) {
        let Some(id) = self.id.take() else { return };
        if self.pin {
            if let Ok(mut pins) = self.state.speech_model_pins.lock() {
                if let Some(count) = pins.get_mut(&id) {
                    *count -= 1;
                    if *count == 0 {
                        pins.remove(&id);
                    }
                }
            }
        }
        let state = self.state.clone();
        self.state.spawn_task(async move {
            if let Err(error) = release(&state, &id).await {
                tracing::warn!(%error,"removed model release deferred");
            }
        });
    }
}
async fn in_use(state: &AppState, id: &str) -> anyhow::Result<bool> {
    if state
        .speech_model_pins
        .lock()
        .map_err(|_| anyhow::anyhow!("model pins poisoned"))?
        .get(id)
        .copied()
        .unwrap_or(0)
        > 0
    {
        return Ok(true);
    }
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM job_requests r JOIN jobs j ON j.id=r.job_id WHERE r.kind='speech' AND json_extract(r.payload_json,'$.model_id')=? AND j.status IN ('pending','running'))").bind(id).fetch_one(state.db.pool()).await?)
}
async fn release_locked(state: &AppState, id: &str) -> anyhow::Result<bool> {
    anyhow::ensure!(valid_id(id), "invalid removed model ID");
    let owned:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM speech_models m WHERE m.id=? AND NOT EXISTS(SELECT 1 FROM speech_model_removals r WHERE r.owner=m.owner AND r.id=m.id))").bind(id).fetch_one(state.db.pool()).await?;
    if owned || in_use(state, id).await? {
        return Ok(false);
    }
    let path = state
        .storage
        .join("models/whisper")
        .join(format!("{id}.bin"));
    if let Some(configured) = &state.speech.model {
        if let (Ok(a), Ok(b)) = (
            tokio::fs::canonicalize(configured).await,
            tokio::fs::canonicalize(&path).await,
        ) {
            if a == b {
                return Ok(false);
            }
        }
    }
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let mut tx = state.db.pool().begin().await?;
    sqlx::query("DELETE FROM speech_models WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM speech_model_removals WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}
async fn release(state: &AppState, id: &str) -> anyhow::Result<bool> {
    let _operation = state.speech_model_ops.lock().await;
    release_locked(state, id).await
}
async fn gc_removed(state: &AppState) -> anyhow::Result<()> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT DISTINCT id FROM speech_model_removals")
        .fetch_all(state.db.pool())
        .await?;
    for id in ids {
        release(state, &id).await?;
    }
    Ok(())
}
pub async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Select>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    if !valid_id(&request.model_id) {
        return Err(AppError::bad_request("Некорректная модель"));
    }
    let _operation = state.speech_model_ops.lock().await;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM speech_models WHERE owner=? AND id=?)")
            .bind(&owner)
            .bind(&request.model_id)
            .fetch_one(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read removed model", e))?;
    if !exists {
        return Err(AppError::not_found("Модель не найдена"));
    }
    let mut tx = state
        .db
        .pool()
        .begin()
        .await
        .map_err(|e| AppError::internal("begin model removal", e))?;
    sqlx::query("INSERT OR IGNORE INTO speech_model_removals(owner,id) VALUES(?,?)")
        .bind(&owner)
        .bind(&request.model_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal("remove model registration", e))?;
    sqlx::query("DELETE FROM speech_model_selection WHERE owner=? AND model_id=?")
        .bind(&owner)
        .bind(&request.model_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal("reset removed selection", e))?;
    tx.commit()
        .await
        .map_err(|e| AppError::internal("commit model removal", e))?;
    let released = match release_locked(&state, &request.model_id).await {
        Ok(released) => released,
        Err(error) => {
            tracing::warn!(%error,"removed model file retained for retry");
            false
        }
    };
    Ok(Json(json!({"removed":true,"fileReleased":released})))
}
