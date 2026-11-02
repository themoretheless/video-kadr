//! Owner model selection. A job resolves its captured ID, never the later selection.
use crate::{
    error::{ApiJson, AppError, AppResult},
    services::{local_tts::TtsConfig, tts_model_catalog::ModelCatalog},
    state::AppState,
};
use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;
use serde_json::{json, Value};

pub(crate) async fn config_for_model(
    state: &AppState,
    owner: &str,
    id: Option<&str>,
) -> AppResult<TtsConfig> {
    let mut config = state.tts.as_ref().clone();
    if let Some(id) = id {
        let catalog = ModelCatalog::find(id)
            .map_err(|e| AppError::internal("read TTS catalog", e))?
            .ok_or_else(|| AppError::bad_request("Неизвестная модель озвучивания"))?;
        let owned: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tts_models WHERE owner=? AND id=?)")
                .bind(owner)
                .bind(id)
                .fetch_one(state.db.pool())
                .await
                .map_err(|e| AppError::internal("read TTS model owner", e))?;
        if !owned {
            return Err(AppError::forbidden("Модель недоступна этому пользователю"));
        }
        let root = state.storage.join("models").join("tts").join(id);
        let checked = root.clone();
        tokio::task::spawn_blocking(move || catalog.verify_directory(&checked))
            .await
            .map_err(|e| AppError::internal("join TTS verification", e))?
            .map_err(|_| {
                AppError::service_unavailable("Комплект модели отсутствует или повреждён")
            })?;
        config.model_dir = Some(root);
    }
    if crate::services::local_tts::family(&config)
        .map_err(|_| AppError::service_unavailable("Некорректная модель"))?
        == crate::services::local_tts::TtsFamily::Chatterbox
    {
        config.executable = state
            .chatterbox
            .as_ref()
            .as_ref()
            .ok_or_else(|| {
                AppError::service_unavailable("Настройте движок Chatterbox и ONNX Runtime")
            })?
            .executable
            .clone();
    }
    if crate::services::local_tts::family(&config).map_err(|_|AppError::service_unavailable("Некорректная модель"))?==crate::services::local_tts::TtsFamily::Kokoro{
        if let Some(runtime)=state.kokoro.as_ref(){config.executable=runtime.executable.clone();}
    }
    Ok(config)
}
pub(crate) async fn config_for_owner(
    state: &AppState,
    owner: &str,
) -> AppResult<(TtsConfig, Option<String>)> {
    let id: Option<String> =
        sqlx::query_scalar("SELECT model_id FROM tts_model_selection WHERE owner=?")
            .bind(owner)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read TTS selection", e))?;
    Ok((config_for_model(state, owner, id.as_deref()).await?, id))
}
pub async fn list(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let catalogs = ModelCatalog::all().map_err(|e| AppError::internal("read TTS catalog", e))?;
    let _operation = state.tts_model_ops.lock().await;
    for model in &catalogs {
        let _ = release_locked(&state, &model.id).await;
    }
    let rows: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT m.id,m.name,m.bytes FROM tts_models m WHERE m.owner=? AND NOT EXISTS(SELECT 1 FROM tts_model_removals r WHERE r.owner=m.owner AND r.id=m.id) ORDER BY m.name,m.id")
            .bind(&owner)
            .fetch_all(state.db.pool())
            .await
            .map_err(|e| AppError::internal("list TTS models", e))?;
    let selected: Option<String> =
        sqlx::query_scalar("SELECT model_id FROM tts_model_selection WHERE owner=?")
            .bind(owner)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read TTS selection", e))?;
    Ok(Json(
        json!({"selectedId":selected.unwrap_or_else(||"configured".into()),"models":rows.into_iter().map(|(id,name,bytes)|json!({"id":id,"name":name,"bytes":bytes})).collect::<Vec<_>>(),"catalog":catalogs.into_iter().map(|m|json!({"id":m.id,"name":m.name,"bytes":m.bytes,"revision":m.revision,"family":m.family})).collect::<Vec<_>>()}),
    ))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    model_id: String,
}
pub async fn select(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Selection>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let _operation = state.tts_model_ops.lock().await;
    if request.model_id == "configured" {
        sqlx::query("DELETE FROM tts_model_selection WHERE owner=?")
            .bind(&owner)
            .execute(state.db.pool())
            .await
            .map_err(|e| AppError::internal("reset TTS selection", e))?;
    } else {
        let removed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM tts_model_removals WHERE owner=? AND id=?)",
        )
        .bind(&owner)
        .bind(&request.model_id)
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| AppError::internal("read removed TTS model", e))?;
        if removed {
            return Err(AppError::not_found("Модель удалена из вашей библиотеки"));
        }
        config_for_model(&state, &owner, Some(&request.model_id)).await?;
        sqlx::query("INSERT INTO tts_model_selection(owner,model_id) VALUES(?,?) ON CONFLICT(owner) DO UPDATE SET model_id=excluded.model_id").bind(&owner).bind(&request.model_id).execute(state.db.pool()).await.map_err(|e| AppError::internal("select TTS model", e))?;
    }
    Ok(Json(json!({"selectedId":request.model_id})))
}

/// Caller holds tts_model_ops: serialize admission, synchronous inference and publication.
async fn release_locked(state: &AppState, id: &str) -> anyhow::Result<bool> {
    let removed: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tts_model_removals WHERE id=?)")
            .bind(id)
            .fetch_one(state.db.pool())
            .await?;
    if !removed {
        return Ok(false);
    }
    let owners: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tts_models m WHERE m.id=? AND NOT EXISTS(SELECT 1 FROM tts_model_removals r WHERE r.id=m.id AND r.owner=m.owner))").bind(id).fetch_one(state.db.pool()).await?;
    // Retriable requests retain their payload. Malformed retained requests pin
    // conservatively instead of allowing collection through a failed JSON parse.
    let pinned: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM job_requests WHERE kind='tts' AND payload_json<>'null' AND CASE WHEN json_valid(payload_json) THEN json_extract(payload_json,'$.model_id')=? ELSE 1 END)").bind(id).fetch_one(state.db.pool()).await?;
    if owners || pinned {
        return Ok(false);
    }
    let parent = crate::services::tts_model_download::directory(
        &state.storage,
        std::path::Path::new("models/tts"),
    )
    .await?;
    let root = parent.join(id);
    if let Some(configured) = &state.tts.model_dir {
        if let (Ok(default), Ok(target)) = (
            tokio::fs::canonicalize(configured).await,
            tokio::fs::canonicalize(&root).await,
        ) {
            if default.starts_with(&target) {
                return Ok(false);
            }
        }
    }
    match tokio::fs::symlink_metadata(&root).await {
        Ok(_) => {
            let checked = root.clone();
            let id_owned = id.to_owned();
            tokio::task::spawn_blocking(move || {
                ModelCatalog::find(&id_owned)?
                    .ok_or_else(|| anyhow::anyhow!("unknown TTS catalog"))?
                    .verify_directory(&checked)
            })
            .await??;
            let trash = crate::services::tts_model_download::directory(
                &state.storage,
                std::path::Path::new("tts-model-trash"),
            )
            .await?
            .join(uuid::Uuid::new_v4().to_string());
            tokio::fs::create_dir(&trash).await?;
            tokio::fs::rename(&root, trash.join("bundle")).await?;
            tokio::fs::remove_dir_all(&trash).await?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    sqlx::query("DELETE FROM tts_models WHERE id=? AND EXISTS(SELECT 1 FROM tts_model_removals r WHERE r.id=tts_models.id AND r.owner=tts_models.owner)").bind(id).execute(state.db.pool()).await?;
    Ok(true)
}
pub async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Selection>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    if ModelCatalog::find(&request.model_id)
        .map_err(|e| AppError::internal("read TTS catalog", e))?
        .is_none()
    {
        return Err(AppError::bad_request("Неизвестная модель озвучивания"));
    }
    let _operation = state.tts_model_ops.lock().await;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tts_models WHERE owner=? AND id=?)")
            .bind(&owner)
            .bind(&request.model_id)
            .fetch_one(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read removed TTS model", e))?;
    if !exists {
        return Err(AppError::not_found("Модель не найдена"));
    }
    let mut tx = state
        .db
        .pool()
        .begin()
        .await
        .map_err(|e| AppError::internal("begin TTS removal", e))?;
    sqlx::query("INSERT INTO tts_model_epochs(owner,id,epoch) VALUES(?,?,1) ON CONFLICT(owner,id) DO UPDATE SET epoch=epoch+1").bind(&owner).bind(&request.model_id).execute(&mut *tx).await.map_err(|e|AppError::internal("advance TTS removal epoch",e))?;
    sqlx::query("INSERT OR IGNORE INTO tts_model_removals(owner,id) VALUES(?,?)")
        .bind(&owner)
        .bind(&request.model_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal("remove TTS registration", e))?;
    sqlx::query("DELETE FROM tts_model_selection WHERE owner=? AND model_id=?")
        .bind(&owner)
        .bind(&request.model_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal("reset TTS selection", e))?;
    tx.commit()
        .await
        .map_err(|e| AppError::internal("commit TTS removal", e))?;
    let file_released = match release_locked(&state, &request.model_id).await {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error,"removed TTS bundle retained");
            false
        }
    };
    Ok(Json(json!({"removed":true,"fileReleased":file_released})))
}

pub(crate) fn kokoro_config(state:&AppState,config:&TtsConfig)->anyhow::Result<crate::services::local_kokoro::Config>{
    let runtime=state.kokoro.as_ref().as_ref().ok_or_else(||anyhow::anyhow!("native Kokoro is not configured"))?;
    Ok(crate::services::local_kokoro::Config{executable:runtime.executable.clone(),model_dir:config.model_dir.clone().ok_or_else(||anyhow::anyhow!("Kokoro model missing"))?,runtime_library:runtime.library.clone(),ort_library:runtime.ort_library.clone(),threads:config.threads})
}
pub(crate) fn chatterbox_config(
    state: &AppState,
    config: &TtsConfig,
) -> anyhow::Result<crate::services::local_chatterbox::Config> {
    let runtime = state
        .chatterbox
        .as_ref()
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Chatterbox runtime is not configured"))?;
    Ok(crate::services::local_chatterbox::Config {
        executable: runtime.executable.clone(),
        runtime_library: runtime.library.clone(),
        model_dir: config
            .model_dir
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Chatterbox model missing"))?,
    })
}
pub(crate) async fn identity(state: &AppState, config: &TtsConfig) -> anyhow::Result<String> {
    if state.kokoro.is_some()&&crate::services::local_tts::family(config)?==crate::services::local_tts::TtsFamily::Kokoro{return crate::services::local_kokoro::identity(&kokoro_config(state,config)?).await}
    if crate::services::local_tts::family(config)?
        == crate::services::local_tts::TtsFamily::Chatterbox
    {
        crate::services::local_chatterbox::identity(&chatterbox_config(state, config)?).await
    } else {
        crate::services::tts_job_identity::fingerprint(config).await
    }
}
pub(crate) fn validate_reference(
    config: &TtsConfig,
    pinned: &super::tts_references::PinnedInput,
) -> AppResult<()> {
    if crate::services::local_tts::family(config)
        .map_err(|_| AppError::bad_request("Некорректная модель"))?
        == crate::services::local_tts::TtsFamily::Chatterbox
        && !pinned.reference.as_ref().is_some_and(|r| r.bytes >= 240044)
    {
        return Err(AppError::bad_request(
            "Для Chatterbox нужен образец от 5 до 10 секунд",
        ));
    }
    Ok(())
}
pub(crate) async fn synthesize(
    state: &AppState,
    config: &TtsConfig,
    request: &crate::services::local_tts::SynthesisRequest,
    output: &std::path::Path,
    cancel: &tokio_util::sync::CancellationToken,
    limit: std::time::Duration,
    reference: Option<&std::path::Path>,
    progress: Option<&tokio::sync::mpsc::UnboundedSender<f64>>,
) -> anyhow::Result<crate::services::local_tts::SynthesizedAudio> {
    request.validate_for_model(config)?;
    if state.kokoro.is_some()&&crate::services::local_tts::family(config)?==crate::services::local_tts::TtsFamily::Kokoro{
        let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();
        return crate::services::local_kokoro::synthesize(&state.process_runtime,&kokoro_config(state,config)?,crate::services::local_kokoro::Input{request,pauses:request.pauses.unwrap_or(0.2)},output,cancel,limit,progress.unwrap_or(&tx)).await
    }
    if crate::services::local_tts::family(config)?
        == crate::services::local_tts::TtsFamily::Chatterbox
    {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut audio = crate::services::local_chatterbox::synthesize(
            &state.process_runtime,
            &chatterbox_config(state, config)?,
            crate::services::local_chatterbox::Input {
                text: &request.text,
                reference: reference
                    .ok_or_else(|| anyhow::anyhow!("missing custom voice reference"))?,
            },
            output,
            cancel,
            limit,
            progress.unwrap_or(&tx),
        )
        .await?;
        audio.voice_id = request.voice_id;
        Ok(audio)
    } else {
        crate::services::local_tts::synthesize_with_reference(
            &state.process_runtime,
            config,
            request,
            output,
            cancel,
            limit,
            reference,
        )
        .await
    }
}

#[cfg(test)]
mod chatterbox_tests {
    use super::*;
    #[test]
    fn chatterbox_reference_minimum_is_enforced_before_admission() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("onnx")).unwrap();
        std::fs::write(
            root.path().join("onnx/language_model_quantized.onnx"),
            b"layout marker",
        )
        .unwrap();
        let config = TtsConfig {
            executable: "/host/chatterbox".into(),
            model_dir: Some(root.path().to_owned()),
            threads: 2,
        };
        let mut pinned = super::super::tts_references::PinnedInput {
            request: crate::services::local_tts::SynthesisRequest {
                pauses: None,
                num_steps: None,
                text: "Hello".into(),
                voice_id: 2099,
                speed: 1.,
            },
            reference: None,
        };
        assert!(validate_reference(&config, &pinned).is_err());
        pinned.reference = Some(super::super::tts_references::ReferenceSnapshot {
            id: uuid::Uuid::new_v4().to_string(),
            hash: "a".repeat(64),
            bytes: 48044,
            source_id: None,
        });
        assert!(validate_reference(&config, &pinned).is_err());
        pinned.reference.as_mut().unwrap().bytes = 240044;
        validate_reference(&config, &pinned).unwrap();
    }
}

pub(crate) const POCKET_ID: &str =
    "f85f04c7acf76154571a1e6b398534c257574cece490d2bb7991eae5e8426d54";
pub(crate) async fn chatterbox_stock_available(state: &AppState, owner: &str) -> AppResult<bool> {
    let visible:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tts_models m WHERE owner=? AND id=? AND NOT EXISTS(SELECT 1 FROM tts_model_removals r WHERE r.owner=m.owner AND r.id=m.id))").bind(owner).bind(POCKET_ID).fetch_one(state.db.pool()).await.map_err(|e|AppError::internal("read stock voice access",e))?;
    if !visible {
        return Ok(false);
    }
    Ok(config_for_model(state, owner, Some(POCKET_ID))
        .await
        .is_ok())
}
