//! Upload, listing and deletion endpoints for private media assets.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::extract::{multipart::MultipartError, Path as AxPath, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;

use crate::assets::{
    is_valid_asset_id, new_asset_id, sniff_content, AssetKind, AssetRecord, SniffedContent,
    MAX_MEDIA_ASSET_BYTES, MAX_TEXT_ASSET_BYTES,
};
use crate::error::{ApiMultipart, AppError, AppResult};
use crate::library::now_secs;
use crate::state::AppState;
use crate::tools::{self, ProbeInfo};

/// Includes bounded room for multipart headers and the closing boundary.
pub const MAX_ASSET_BODY_BYTES: usize = MAX_MEDIA_ASSET_BYTES + 64 * 1024;
const ASSET_UPLOAD_RECEIVE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// The `kind` form field is a short enum token, never a payload.
const MAX_KIND_FIELD_BYTES: usize = 32;
/// Enough leading bytes for every signature `sniff_content` recognises.
const SNIFF_READ_BYTES: usize = 64;
const HASH_CHUNK_BYTES: usize = 64 * 1024;

/// Removes its file on drop unless `keep` was called, so a rejected upload
/// never leaves bytes behind (drop also runs when Axum cancels the handler).
struct ManagedFile {
    path: Option<PathBuf>,
}

impl ManagedFile {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn path(&self) -> &Path {
        self.path.as_deref().unwrap_or(Path::new(""))
    }

    fn keep(&mut self) {
        self.path = None;
    }
}

impl Drop for ManagedFile {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[derive(Debug)]
struct ReceivedAsset {
    kind: Option<AssetKind>,
    total_bytes: u64,
}

pub async fn asset_upload_handler(
    State(state): State<AppState>,
    ApiMultipart(multipart): ApiMultipart,
) -> AppResult<(StatusCode, Json<AssetRecord>)> {
    let _upload_slot = state.try_acquire_upload_slot().ok_or_else(|| {
        AppError::too_many_requests("слишком много одновременных загрузок, повторите позже")
    })?;
    let id = new_asset_id();
    tokio::fs::create_dir_all(state.staging_dir())
        .await
        .map_err(|error| AppError::internal("create staging directory", error))?;
    let mut temporary = ManagedFile::new(state.staging_dir().join(format!("{id}.asset-upload")));
    let received = receive_with_timeout(
        receive_asset(multipart, temporary.path()),
        temporary.path(),
        ASSET_UPLOAD_RECEIVE_TIMEOUT,
    )
    .await?;

    let kind = received
        .kind
        .ok_or_else(|| AppError::bad_request("не указан тип ассета"))?;
    if received.total_bytes == 0 {
        return Err(AppError::bad_request("пустой файл"));
    }
    if received.total_bytes > kind.max_bytes() as u64 {
        return Err(oversized_error(kind));
    }

    let sniffed = sniff_asset(temporary.path(), kind).await?;
    if !sniffed.supports(kind) {
        return Err(AppError::unsupported_media_type(
            "содержимое файла не соответствует выбранному типу",
        ));
    }
    let probe = if kind.needs_probe() {
        Some(probe_asset(&state, temporary.path(), kind).await?)
    } else {
        None
    };

    let sha256 = hash_file(temporary.path()).await?;
    let filename = format!("{id}.{}", sniffed.extension);
    let assets_dir = state.assets.assets_dir().to_path_buf();
    tokio::fs::create_dir_all(&assets_dir)
        .await
        .map_err(|error| AppError::internal("create assets directory", error))?;
    let destination = assets_dir.join(&filename);
    tokio::fs::rename(temporary.path(), &destination)
        .await
        .map_err(|error| AppError::internal("publish asset", error))?;
    temporary.keep();
    let mut published = ManagedFile::new(destination);

    let record = AssetRecord::new(
        id,
        kind,
        filename,
        sniffed.mime.to_owned(),
        received.total_bytes,
        sha256,
        now_secs(),
    )
    .with_media_metadata(
        probe.as_ref().map(|info| info.width),
        probe.as_ref().map(|info| info.height),
        probe.as_ref().map(|info| info.duration),
    );
    if !state.assets.insert(record.clone()).await {
        return Err(AppError::internal(
            "persist asset metadata",
            anyhow::anyhow!("asset index write failed"),
        ));
    }
    published.keep();
    Ok((StatusCode::CREATED, Json(record)))
}

pub async fn asset_list_handler(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "assets": state.assets.list() }))
}

pub async fn asset_get_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<AssetRecord>> {
    state
        .assets
        .get(&id)
        .map(Json)
        .ok_or_else(|| AppError::not_found("ассет не найден"))
}

pub async fn asset_delete_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<StatusCode> {
    if !is_valid_asset_id(&id) || !state.assets.remove(&id).await {
        return Err(AppError::not_found("ассет не найден"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Read only as much as the signature needs. Subtitles have no binary marker,
/// so their (already 4 MiB capped) text is validated in full.
async fn sniff_asset(path: &Path, kind: AssetKind) -> AppResult<SniffedContent> {
    let limit = if kind == AssetKind::Subtitle {
        MAX_TEXT_ASSET_BYTES
    } else {
        SNIFF_READ_BYTES
    };
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|error| AppError::internal("open staged asset", error))?;
    let mut head = Vec::new();
    file.take(limit as u64)
        .read_to_end(&mut head)
        .await
        .map_err(|error| AppError::internal("read staged asset", error))?;
    sniff_content(&head)
        .ok_or_else(|| AppError::unsupported_media_type("формат файла не поддерживается"))
}

/// Bounded `ffprobe` confirmation for audio/video. The client extension and the
/// declared MIME type never take part in this decision.
async fn probe_asset(state: &AppState, path: &Path, kind: AssetKind) -> AppResult<ProbeInfo> {
    let info = tools::probe_video(&state.process_runtime, path)
        .await
        .map_err(|error| {
            if tools::is_tool_timeout(&error) {
                AppError::gateway_timeout("анализ файла превысил лимит времени")
            } else {
                AppError::bad_request("не удалось распознать медиафайл")
            }
        })?;
    let accepted = match kind {
        AssetKind::Video => info.vcodec.is_some() && info.width > 0 && info.height > 0,
        AssetKind::Audio => info.acodec.is_some(),
        _ => false,
    };
    if !accepted {
        return Err(AppError::unsupported_media_type(
            "в файле нет подходящей дорожки",
        ));
    }
    Ok(info)
}

async fn hash_file(path: &Path) -> AppResult<String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| AppError::internal("open staged asset", error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; HASH_CHUNK_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| AppError::internal("hash staged asset", error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

async fn receive_asset(
    mut multipart: axum::extract::Multipart,
    path: &Path,
) -> AppResult<ReceivedAsset> {
    let mut kind = None;
    let mut total_bytes = None;
    while let Some(mut field) = multipart.next_field().await.map_err(multipart_error)? {
        match field.name() {
            Some("kind") => {
                let mut value = String::new();
                while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
                    if value.len() + chunk.len() > MAX_KIND_FIELD_BYTES {
                        return Err(AppError::bad_request("недопустимый тип ассета"));
                    }
                    value.push_str(&String::from_utf8_lossy(&chunk));
                }
                kind = Some(
                    AssetKind::parse(value.trim())
                        .ok_or_else(|| AppError::bad_request("недопустимый тип ассета"))?,
                );
            }
            Some("file") => {
                if total_bytes.is_some() {
                    return Err(AppError::bad_request("в запросе несколько файлов"));
                }
                total_bytes = Some(stream_field_to_file(&mut field, path).await?);
            }
            _ => continue,
        }
    }
    let total_bytes =
        total_bytes.ok_or_else(|| AppError::bad_request("файл не найден в запросе"))?;
    Ok(ReceivedAsset { kind, total_bytes })
}

/// Stream to disk with the widest per-kind cap. The exact per-kind cap is
/// enforced afterwards, because `kind` may arrive after the file part.
async fn stream_field_to_file(
    field: &mut axum::extract::multipart::Field<'_>,
    path: &Path,
) -> AppResult<u64> {
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|error| AppError::internal("create staged asset", error))?;
    let mut total = 0_u64;
    while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
        total = total
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| oversized_error(AssetKind::Video))?;
        if total > MAX_MEDIA_ASSET_BYTES as u64 {
            return Err(oversized_error(AssetKind::Video));
        }
        file.write_all(&chunk)
            .await
            .map_err(|error| AppError::internal("write staged asset", error))?;
    }
    file.flush()
        .await
        .map_err(|error| AppError::internal("flush staged asset", error))?;
    Ok(total)
}

async fn receive_with_timeout<F>(
    receive: F,
    path: &Path,
    limit: Duration,
) -> AppResult<ReceivedAsset>
where
    F: Future<Output = AppResult<ReceivedAsset>>,
{
    match timeout(limit, receive).await {
        Ok(Ok(received)) => Ok(received),
        Ok(Err(error)) => Err(error),
        Err(_) => {
            let _ = tokio::fs::remove_file(path).await;
            Err(AppError::request_timeout(
                "загрузка файла превысила лимит времени",
            ))
        }
    }
}

fn oversized_error(kind: AssetKind) -> AppError {
    if kind.needs_probe() || kind == AssetKind::Image {
        AppError::payload_too_large("файл превышает 64 МиБ")
    } else {
        AppError::payload_too_large("файл превышает 4 МиБ")
    }
}

fn multipart_error(error: MultipartError) -> AppError {
    match error.status() {
        StatusCode::PAYLOAD_TOO_LARGE => {
            AppError::payload_too_large("файл превышает допустимый размер")
        }
        StatusCode::BAD_REQUEST => AppError::invalid_multipart(),
        _ => AppError::internal("read asset multipart upload", error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn receive_timeout_removes_the_staged_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pending.asset-upload");
        tokio::fs::write(&path, b"partial").await.unwrap();

        let error = receive_with_timeout(std::future::pending(), &path, Duration::from_millis(10))
            .await
            .unwrap_err();

        assert_eq!(error.status(), StatusCode::REQUEST_TIMEOUT);
        assert_eq!(error.code(), "request_timeout");
        assert!(tokio::fs::metadata(path).await.is_err());
    }

    #[tokio::test]
    async fn subtitles_are_sniffed_from_their_full_text() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cues.bin");
        tokio::fs::write(&path, b"WEBVTT\n\n00:00.000 --> 00:01.000\nhi\n")
            .await
            .unwrap();

        let sniffed = sniff_asset(&path, AssetKind::Subtitle).await.unwrap();
        assert_eq!(sniffed.extension, "vtt");
        assert!(sniffed.supports(AssetKind::Subtitle));
    }

    #[tokio::test]
    async fn hashing_streams_the_whole_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("payload.bin");
        let bytes = vec![7_u8; HASH_CHUNK_BYTES * 2 + 13];
        tokio::fs::write(&path, &bytes).await.unwrap();

        assert_eq!(
            hash_file(&path).await.unwrap(),
            format!("{:x}", Sha256::digest(&bytes))
        );
    }
}
