//! Persist an explicitly accepted narration as private project media.
use crate::{
    domain::filmcraft_wav_audio::WavSource,
    error::{AppError, AppResult},
    library::MediaEntry,
    state::AppState,
};
use axum::{
    body::{to_bytes, Body},
    extract::{Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tokio::io::AsyncWriteExt;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    project_id: String,
}
struct OwnedFile(std::path::PathBuf, bool);
impl Drop for OwnedFile {
    fn drop(&mut self) {
        if !self.1 {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}
pub async fn store(
    State(state): State<AppState>,
    Query(target): Query<Target>,
    headers: HeaderMap,
    body: Body,
) -> AppResult<Json<MediaEntry>> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM composition_project_members WHERE project_id=? AND actor=?",
    )
    .bind(&target.project_id)
    .bind(&actor)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| AppError::internal("authorize narration project", e))?;
    if !matches!(role.as_deref(), Some("owner" | "editor")) {
        return Err(AppError::forbidden(
            "Для добавления озвучки нужен доступ редактора к проекту",
        ));
    }
    let _slot = state
        .try_acquire_upload_slot()
        .ok_or_else(|| AppError::too_many_requests("Повторите сохранение озвучки позже"))?;
    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        to_bytes(body, 64 * 1024 * 1024),
    )
    .await
    .map_err(|_| AppError::bad_request("Сохранение озвучки превысило время ожидания"))?
    .map_err(|_| AppError::bad_request("Озвучка превышает допустимый размер"))?;
    let audio = WavSource::parse(bytes.to_vec().into())
        .map_err(|_| AppError::bad_request("Нужен звуковой файл WAV"))?;
    if audio.channels() != 1
        || audio.sample_rate() != 24000
        || audio.frames() == 0
        || audio.frames() > 24_000 * 600
    {
        return Err(AppError::bad_request(
            "Нужна монофоническая озвучка WAV 24 кГц длительностью до десяти минут",
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let filename = format!("{id}.wav");
    let space = state
        .db
        .claim_project_narration(&target.project_id, &actor, &id)
        .await
        .map_err(|_| AppError::forbidden("Доступ к проекту изменился"))?;
    let operation=async{
  let directory=state.sources_dir().join("spaces").join(&space);tokio::fs::create_dir_all(&directory).await.map_err(|e|AppError::internal("create narration media directory",e))?;
  let mut staged=OwnedFile(directory.join(format!(".{id}.part")),true);let mut file=tokio::fs::OpenOptions::new().write(true).create_new(true).open(&staged.0).await.map_err(|e|AppError::internal("stage narration audio",e))?;staged.1=false;
  #[cfg(unix)] {use std::os::unix::fs::PermissionsExt;file.set_permissions(std::fs::Permissions::from_mode(0o600)).await.map_err(|e|AppError::internal("protect narration audio",e))?;}
  file.write_all(&bytes).await.map_err(|e|AppError::internal("write narration audio",e))?;file.sync_all().await.map_err(|e|AppError::internal("sync narration audio",e))?;drop(file);
  let mut published=OwnedFile(directory.join(&filename),true);tokio::fs::hard_link(&staged.0,&published.0).await.map_err(|e|AppError::internal("publish narration media",e))?;published.1=false;
  let role:Option<String>=sqlx::query_scalar("SELECT role FROM composition_project_members WHERE project_id=? AND actor=?").bind(&target.project_id).bind(&actor).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("recheck narration project",e))?;
  if !matches!(role.as_deref(),Some("owner"|"editor")){return Err(AppError::forbidden("Доступ к проекту изменился"));}
  let entry=MediaEntry::from_result("source",&json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"storageKey":format!("spaces/{space}/{filename}"),"mediaType":"audio","duration":audio.frames() as f64/24000.,"title":"Озвучка","sizeBytes":bytes.len()}));
  if !state.library.add(entry.clone()).await{return Err(AppError::service_unavailable("Не удалось сохранить озвучку в медиатеке"));}
  published.1=true;state.index_media(&entry).await;Ok(Json(entry))
 }.await;
    if operation.is_err() {
        let _ = state.db.release_space_source_claim(&id).await;
    }
    operation
}
