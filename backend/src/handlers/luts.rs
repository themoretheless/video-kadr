//! Upload and discovery endpoints for private immutable 3D CUBE LUT assets.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::extract::{multipart::MultipartError, Path as AxPath, State};
use axum::http::StatusCode;
use axum::Json;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::time::timeout;
use uuid::Uuid;

use crate::error::{ApiMultipart, AppError, AppResult};
use crate::library::now_secs;
use crate::luts::{display_name, parse_cube, CubeError, LutAsset, MAX_LUT_FILE_BYTES};
use crate::state::AppState;

/// Includes bounded room for multipart headers and the closing boundary.
pub const MAX_LUT_BODY_BYTES: usize = MAX_LUT_FILE_BYTES + 64 * 1024;
const LUT_UPLOAD_RECEIVE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

struct ManagedFile {
    path: Option<PathBuf>,
}

impl ManagedFile {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn path(&self) -> &Path {
        self.path.as_deref().expect("managed file still active")
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
struct ReceivedLut {
    original_name: Option<String>,
    total_bytes: usize,
}

pub async fn lut_upload_handler(
    State(state): State<AppState>,
    ApiMultipart(multipart): ApiMultipart,
) -> AppResult<(StatusCode, Json<LutAsset>)> {
    let _upload_slot = state.try_acquire_upload_slot().ok_or_else(|| {
        AppError::too_many_requests("слишком много одновременных загрузок, повторите позже")
    })?;
    let id = Uuid::new_v4().to_string();
    let mut temporary = ManagedFile::new(state.staging_dir().join(format!("{id}.lut-upload")));
    let received = receive_with_timeout(
        receive_lut(multipart, temporary.path()),
        temporary.path(),
        LUT_UPLOAD_RECEIVE_TIMEOUT,
    )
    .await?;
    if received.total_bytes == 0 {
        return Err(AppError::bad_request("пустой LUT-файл"));
    }

    let bytes = tokio::fs::read(temporary.path())
        .await
        .map_err(|error| AppError::internal("read staged LUT", error))?;
    let parsed = tokio::task::spawn_blocking(move || parse_cube(&bytes))
        .await
        .map_err(|error| AppError::internal("join LUT parser", error))?
        .map_err(cube_error)?;
    let sha256 = format!("{:x}", Sha256::digest(&parsed.canonical));
    tokio::fs::write(temporary.path(), &parsed.canonical)
        .await
        .map_err(|error| AppError::internal("write canonical LUT", error))?;

    let filename = format!("{id}.cube");
    let destination = state.luts_dir().join(&filename);
    tokio::fs::rename(temporary.path(), &destination)
        .await
        .map_err(|error| AppError::internal("publish LUT", error))?;
    temporary.keep();
    let mut published = ManagedFile::new(destination);
    let asset = LutAsset::new(
        id,
        display_name(received.original_name.as_deref()),
        filename,
        parsed.cube_size,
        parsed.canonical.len() as u64,
        sha256,
        now_secs(),
    );
    let (resolved, created) = state.db.insert_or_get_lut(&asset).await.map_err(|error| {
        if crate::db::is_lut_quota_exceeded(&error) {
            AppError::conflict("хранилище LUT заполнено")
        } else {
            AppError::internal("persist LUT metadata", error)
        }
    })?;
    if created {
        published.keep();
    } else {
        repair_deduplicated_lut(&state, &mut published, &asset, &resolved).await?;
    }
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(resolved),
    ))
}

async fn repair_deduplicated_lut(
    state: &AppState,
    fresh: &mut ManagedFile,
    uploaded: &LutAsset,
    resolved: &LutAsset,
) -> AppResult<()> {
    let expected_filename = format!("{}.cube", resolved.id);
    if Uuid::parse_str(&resolved.id).is_err()
        || resolved.filename != expected_filename
        || resolved.sha256 != uploaded.sha256
        || resolved.size_bytes != uploaded.size_bytes
        || resolved.cube_size != uploaded.cube_size
    {
        return Err(AppError::internal(
            "validate deduplicated LUT metadata",
            anyhow::anyhow!("stored LUT metadata is inconsistent"),
        ));
    }

    let target = state.luts_dir().join(&resolved.filename);
    if lut_file_matches(&target, resolved).await {
        return Ok(());
    }

    tokio::fs::rename(fresh.path(), &target)
        .await
        .map_err(|error| AppError::internal("repair deduplicated LUT", error))?;
    fresh.keep();
    if !lut_file_matches(&target, resolved).await {
        return Err(AppError::internal(
            "verify repaired LUT",
            anyhow::anyhow!("repaired LUT content does not match metadata"),
        ));
    }
    Ok(())
}

async fn lut_file_matches(path: &Path, asset: &LutAsset) -> bool {
    let Ok(metadata) = tokio::fs::symlink_metadata(path).await else {
        return false;
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() != asset.size_bytes
        || metadata.len() > MAX_LUT_FILE_BYTES as u64
    {
        return false;
    }
    let Ok(bytes) = tokio::fs::read(path).await else {
        return false;
    };
    bytes.len() <= MAX_LUT_FILE_BYTES && format!("{:x}", Sha256::digest(&bytes)) == asset.sha256
}

pub async fn lut_list_handler(State(state): State<AppState>) -> AppResult<Json<Vec<LutAsset>>> {
    let assets = state
        .db
        .list_luts()
        .await
        .map_err(|error| AppError::internal("list LUT metadata", error))?;
    Ok(Json(assets))
}

pub async fn lut_get_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<LutAsset>> {
    if Uuid::parse_str(&id).is_err() {
        return Err(AppError::not_found("LUT не найден"));
    }
    state
        .db
        .get_lut(&id)
        .await
        .map_err(|error| AppError::internal("load LUT metadata", error))?
        .map(Json)
        .ok_or_else(|| AppError::not_found("LUT не найден"))
}

async fn receive_lut(
    mut multipart: axum::extract::Multipart,
    path: &Path,
) -> AppResult<ReceivedLut> {
    while let Some(mut field) = multipart.next_field().await.map_err(multipart_error)? {
        if field.name() != Some("file") {
            continue;
        }
        let original_name = field.file_name().map(str::to_owned);
        let mut file = tokio::fs::File::create(path)
            .await
            .map_err(|error| AppError::internal("create staged LUT", error))?;
        let mut total_bytes = 0_usize;
        while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
            total_bytes = total_bytes
                .checked_add(chunk.len())
                .ok_or_else(|| AppError::payload_too_large("LUT-файл превышает 16 МиБ"))?;
            if total_bytes > MAX_LUT_FILE_BYTES {
                return Err(AppError::payload_too_large("LUT-файл превышает 16 МиБ"));
            }
            file.write_all(&chunk)
                .await
                .map_err(|error| AppError::internal("write staged LUT", error))?;
        }
        file.flush()
            .await
            .map_err(|error| AppError::internal("flush staged LUT", error))?;
        return Ok(ReceivedLut {
            original_name,
            total_bytes,
        });
    }
    Err(AppError::bad_request("LUT-файл не найден в запросе"))
}

async fn receive_with_timeout<F>(receive: F, path: &Path, limit: Duration) -> AppResult<ReceivedLut>
where
    F: Future<Output = AppResult<ReceivedLut>>,
{
    match timeout(limit, receive).await {
        Ok(Ok(received)) => Ok(received),
        Ok(Err(error)) => Err(error),
        Err(_) => {
            let _ = tokio::fs::remove_file(path).await;
            Err(AppError::request_timeout(
                "загрузка LUT превысила лимит времени",
            ))
        }
    }
}

fn multipart_error(error: MultipartError) -> AppError {
    match error.status() {
        StatusCode::PAYLOAD_TOO_LARGE => AppError::payload_too_large("LUT-файл превышает 16 МиБ"),
        StatusCode::BAD_REQUEST => AppError::invalid_multipart(),
        _ => AppError::internal("read LUT multipart upload", error),
    }
}

fn cube_error(error: CubeError) -> AppError {
    match error {
        CubeError::TooLarge => AppError::payload_too_large("LUT-файл превышает 16 МиБ"),
        CubeError::UnsupportedOneDimensional => {
            AppError::unsupported_media_type("поддерживаются только трёхмерные CUBE LUT")
        }
        _ => AppError::bad_request("некорректный трёхмерный CUBE LUT"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn receive_timeout_removes_the_staged_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pending.lut-upload");
        tokio::fs::write(&path, b"partial").await.unwrap();

        let error = receive_with_timeout(std::future::pending(), &path, Duration::from_millis(10))
            .await
            .unwrap_err();

        assert_eq!(error.status(), StatusCode::REQUEST_TIMEOUT);
        assert_eq!(error.code(), "request_timeout");
        assert!(tokio::fs::metadata(path).await.is_err());
    }
}
