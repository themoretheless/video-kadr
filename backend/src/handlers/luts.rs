//! Upload and discovery endpoints for private immutable 3D CUBE LUT assets.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{multipart::MultipartError, Path as AxPath, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::error::{ApiJson, ApiMultipart, AppError, AppResult};
use crate::library::now_secs;
use crate::lut_baker::{bake, BakeRequest};
use crate::luts::{
    display_name, parse_cube, CubeError, FavoriteUpdate, LutAsset, MAX_LUT_FILE_BYTES,
};
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

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LutCatalogQuery {
    q: Option<String>,
    favorite: Option<bool>,
}

pub async fn lut_list_handler(
    State(state): State<AppState>,
    Query(query): Query<LutCatalogQuery>,
) -> AppResult<Json<Vec<LutAsset>>> {
    if query
        .q
        .as_ref()
        .is_some_and(|value| value.chars().count() > 128)
    {
        return Err(AppError::bad_request(
            "поисковый запрос LUT слишком длинный",
        ));
    }
    let assets = state
        .db
        .search_luts(query.q.as_deref(), query.favorite.unwrap_or(false))
        .await
        .map_err(|error| AppError::internal("list LUT metadata", error))?;
    Ok(Json(assets))
}

pub async fn lut_favorite_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    ApiJson(update): ApiJson<FavoriteUpdate>,
) -> AppResult<Json<LutAsset>> {
    if Uuid::parse_str(&id).is_err() {
        return Err(AppError::not_found("LUT не найден"));
    }
    state
        .db
        .set_lut_favorite(&id, update.favorite)
        .await
        .map_err(|error| AppError::internal("update LUT favorite", error))?
        .map(Json)
        .ok_or_else(|| AppError::not_found("LUT не найден"))
}

pub async fn lut_content_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Response> {
    if Uuid::parse_str(&id).is_err() {
        return Err(AppError::not_found("LUT не найден"));
    }
    let asset = state
        .db
        .get_lut(&id)
        .await
        .map_err(|error| AppError::internal("load LUT metadata", error))?
        .ok_or_else(|| AppError::not_found("LUT не найден"))?;
    let bytes = verified_lut_bytes(&state, &asset).await?;
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-cube"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{}.cube\"", asset.id))
            .expect("UUID filename is safe"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        "x-content-sha256",
        HeaderValue::from_str(&asset.sha256).expect("stored SHA-256 is HTTP-safe"),
    );
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&format!("\"{}\"", asset.sha256))
            .expect("stored SHA-256 is an HTTP-safe ETag"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

pub async fn lut_bake_handler(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<BakeRequest>,
) -> AppResult<Response> {
    let capability = crate::capabilities::Capabilities::from_tools(&state.tools);
    if !capability
        .filters
        .iter()
        .any(|option| option.id == "lut-baker-33-v1" && option.available)
    {
        return Err(AppError::conflict("Экспорт LUT 33×33×33 недоступен"));
    }
    let parsed = state
        .cpu_pool
        .execute(CancellationToken::new(), move |_| {
            bake(&request).map_err(anyhow::Error::msg)
        })
        .await
        .map_err(|error| match error {
            crate::runtime::cpu_pool::CpuTaskError::Saturated => {
                AppError::too_many_requests("очередь экспорта LUT заполнена, повторите позже")
            }
            crate::runtime::cpu_pool::CpuTaskError::Work(error) => {
                AppError::bad_request(error.to_string())
            }
            error => AppError::internal("bake LUT", anyhow::anyhow!(error.to_string())),
        })?;
    let mut response = Response::new(Body::from(parsed.canonical));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-cube"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"video-kadr-look-33.cube\""),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

async fn verified_lut_bytes(state: &AppState, asset: &LutAsset) -> AppResult<Vec<u8>> {
    let expected = format!("{}.cube", asset.id);
    if asset.filename != expected {
        return Err(AppError::internal(
            "validate LUT filename",
            anyhow::anyhow!("invalid LUT filename"),
        ));
    }
    let path = state.luts_dir().join(&asset.filename);
    let metadata = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| AppError::not_found("Файл LUT не найден"))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() != asset.size_bytes
        || metadata.len() > MAX_LUT_FILE_BYTES as u64
    {
        return Err(AppError::conflict("Файл LUT повреждён"));
    }
    let root = tokio::fs::canonicalize(state.luts_dir())
        .await
        .map_err(|error| AppError::internal("resolve LUT storage", error))?;
    let canonical = tokio::fs::canonicalize(&path)
        .await
        .map_err(|_| AppError::not_found("Файл LUT не найден"))?;
    if canonical.parent() != Some(root.as_path()) {
        return Err(AppError::conflict("Файл LUT повреждён"));
    }
    let bytes = tokio::fs::read(&canonical)
        .await
        .map_err(|_| AppError::not_found("Файл LUT не найден"))?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if hash != asset.sha256 {
        return Err(AppError::conflict("Файл LUT повреждён"));
    }
    let parsed = tokio::task::spawn_blocking({
        let bytes = bytes.clone();
        move || parse_cube(&bytes)
    })
    .await
    .map_err(|error| AppError::internal("join LUT verifier", error))?
    .map_err(|_| AppError::conflict("Файл LUT повреждён"))?;
    if parsed.canonical != bytes || parsed.cube_size != asset.cube_size {
        return Err(AppError::conflict("Файл LUT повреждён"));
    }
    Ok(bytes)
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
