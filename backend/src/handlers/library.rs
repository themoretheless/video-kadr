//! Media library endpoints: list, annotate, and delete persisted sources/outputs.

use axum::body::Body;
use axum::extract::{Path as AxPath, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::de::{Deserializer, Visitor};
use serde::{Deserialize, Serialize};

use crate::analysis::thumbnail::{
    infer_thumbnail_kind, ThumbnailKind, ThumbnailSource, FILMSTRIP_CELL_COUNT,
    FILMSTRIP_CELL_HEIGHT, FILMSTRIP_CELL_WIDTH,
};
use crate::db::{
    normalize_library_tags, normalize_library_title, valid_composition_source_id, LibraryMetadata,
    LibraryMetadataChanges, MAX_LIBRARY_TAGS, MAX_LIBRARY_TAG_CHARS, MAX_LIBRARY_TITLE_CHARS,
};
use crate::error::{AppError, AppResult};
use crate::library::MediaEntry;
use crate::state::AppState;

const IMMUTABLE_THUMBNAIL_CACHE: &str = "public, max-age=31536000, immutable";
const MAX_THUMBNAIL_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_FILMSTRIP_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntryResponse {
    #[serde(flatten)]
    entry: MediaEntry,
    favorite: bool,
    tags: Vec<String>,
}

impl LibraryEntryResponse {
    fn new(mut entry: MediaEntry, metadata: Option<LibraryMetadata>) -> Self {
        let (favorite, tags) = if let Some(metadata) = metadata {
            if metadata.title.is_some() {
                entry.title = metadata.title;
            }
            (metadata.favorite, metadata.tags)
        } else {
            (false, Vec::new())
        };
        Self {
            entry,
            favorite,
            tags,
        }
    }
}

/// `GET /api/library` — list persisted sources and outputs, newest first.
pub async fn library_list_handler(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<LibraryEntryResponse>>> {
    let entries = state.library.list().await;
    let mut metadata = state
        .db
        .list_library_metadata()
        .await
        .map_err(|error| AppError::internal("list library metadata", error))?;
    Ok(Json(
        entries
            .into_iter()
            .map(|entry| {
                let item_metadata = metadata.remove(&entry.id);
                LibraryEntryResponse::new(entry, item_metadata)
            })
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
pub struct LibrarySearchQuery {
    q: String,
    #[serde(default = "default_search_limit")]
    limit: u32,
}

pub async fn library_search_handler(
    State(state): State<AppState>,
    Query(query): Query<LibrarySearchQuery>,
) -> AppResult<Json<Vec<crate::ports::SearchHit>>> {
    let hits = state
        .media_search
        .search(&query.q, query.limit)
        .await
        .map_err(|error| AppError::internal("search media library", error))?;
    Ok(Json(hits))
}

/// Resolve the current media identity to a fingerprinted thumbnail URL. The
/// stable URL itself is never cached as immutable, so source replacement
/// cannot leave a stale preview pinned in the browser.
pub async fn library_thumbnail_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Response> {
    let source = thumbnail_source(&state, &id).await?;
    ensure_thumbnail_support(&state, source.kind)?;
    let key = state
        .thumbnail_service
        .current_key(&source)
        .await
        .map_err(|error| {
            tracing::warn!(media.id = %id, %error, "resolve thumbnail identity");
            AppError::thumbnail_unavailable("Не удалось подготовить предпросмотр")
        })?;
    Response::builder()
        .status(StatusCode::TEMPORARY_REDIRECT)
        .header(
            header::LOCATION,
            format!("/api/library/{id}/thumbnail/{key}"),
        )
        .header(header::CACHE_CONTROL, "private, no-cache")
        .body(Body::empty())
        .map_err(|error| AppError::internal("build thumbnail redirect", error))
}

/// Generate or serve a bounded current PNG. A stale fingerprint deliberately
/// returns 404 rather than redirecting, preserving immutable URL semantics.
pub async fn library_thumbnail_version_handler(
    State(state): State<AppState>,
    AxPath((id, key)): AxPath<(String, String)>,
    headers: HeaderMap,
) -> AppResult<Response> {
    if !is_sha256(&key) {
        return Err(AppError::bad_request("Некорректный ключ предпросмотра"));
    }
    let source = thumbnail_source(&state, &id).await?;
    ensure_thumbnail_support(&state, source.kind)?;
    let current_key = state
        .thumbnail_service
        .current_key(&source)
        .await
        .map_err(|error| {
            tracing::warn!(media.id = %id, %error, "resolve thumbnail identity");
            AppError::thumbnail_unavailable("Не удалось подготовить предпросмотр")
        })?;
    if current_key != key {
        return Err(AppError::not_found("Версия предпросмотра устарела"));
    }
    let etag = format!("\"{key}\"");
    if etag_matches(headers.get(header::IF_NONE_MATCH), &etag) {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::CACHE_CONTROL, IMMUTABLE_THUMBNAIL_CACHE)
            .header(header::ETAG, etag)
            .body(Body::empty())
            .map_err(|error| AppError::internal("build thumbnail response", error));
    }

    let cancellation = RequestCancellation::new();
    let artifact = state
        .thumbnail_service
        .get_or_create(&source, cancellation.token())
        .await
        .map_err(|error| {
            tracing::warn!(media.id = %id, %error, "generate thumbnail");
            AppError::thumbnail_unavailable("Не удалось создать предпросмотр")
        })?;
    if artifact.key != key || artifact.size_bytes as usize > MAX_THUMBNAIL_RESPONSE_BYTES {
        return Err(AppError::thumbnail_unavailable(
            "Некорректный кэш предпросмотра",
        ));
    }
    let bytes = tokio::fs::read(&artifact.path)
        .await
        .map_err(|error| AppError::internal("read generated thumbnail", error))?;
    if bytes.len() != artifact.size_bytes as usize {
        return Err(AppError::thumbnail_unavailable(
            "Предпросмотр изменился во время чтения",
        ));
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CONTENT_LENGTH, bytes.len())
        .header(header::CACHE_CONTROL, IMMUTABLE_THUMBNAIL_CACHE)
        .header(header::ETAG, etag)
        .body(Body::from(bytes))
        .map_err(|error| AppError::internal("build thumbnail response", error))
}

/// Resolve a video to an immutable, content-addressed eight-cell filmstrip.
pub async fn library_filmstrip_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Response> {
    let source = filmstrip_source(&state, &id).await?;
    ensure_filmstrip_support(&state)?;
    let key = state
        .thumbnail_service
        .current_filmstrip_key(&source)
        .await
        .map_err(|error| {
            tracing::warn!(media.id = %id, %error, "resolve filmstrip identity");
            AppError::filmstrip_unavailable("Не удалось подготовить раскадровку")
        })?;
    Response::builder()
        .status(StatusCode::TEMPORARY_REDIRECT)
        .header(
            header::LOCATION,
            format!("/api/library/{id}/filmstrip/{key}"),
        )
        .header(header::CACHE_CONTROL, "private, no-cache")
        .body(Body::empty())
        .map_err(|error| AppError::internal("build filmstrip redirect", error))
}

pub async fn library_filmstrip_version_handler(
    State(state): State<AppState>,
    AxPath((id, key)): AxPath<(String, String)>,
    headers: HeaderMap,
) -> AppResult<Response> {
    if !is_sha256(&key) {
        return Err(AppError::bad_request("Некорректный ключ раскадровки"));
    }
    let source = filmstrip_source(&state, &id).await?;
    ensure_filmstrip_support(&state)?;
    let current_key = state
        .thumbnail_service
        .current_filmstrip_key(&source)
        .await
        .map_err(|error| {
            tracing::warn!(media.id = %id, %error, "resolve filmstrip identity");
            AppError::filmstrip_unavailable("Не удалось подготовить раскадровку")
        })?;
    if current_key != key {
        return Err(AppError::not_found("Версия раскадровки устарела"));
    }
    let etag = format!("\"{key}\"");
    if etag_matches(headers.get(header::IF_NONE_MATCH), &etag) {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::CACHE_CONTROL, IMMUTABLE_THUMBNAIL_CACHE)
            .header(header::ETAG, etag)
            .body(Body::empty())
            .map_err(|error| AppError::internal("build filmstrip response", error));
    }

    let cancellation = RequestCancellation::new();
    let artifact = state
        .thumbnail_service
        .get_or_create_filmstrip(&source, cancellation.token())
        .await
        .map_err(|error| {
            tracing::warn!(media.id = %id, %error, "generate filmstrip");
            AppError::filmstrip_unavailable("Не удалось создать раскадровку")
        })?;
    if artifact.key != key || artifact.size_bytes as usize > MAX_FILMSTRIP_RESPONSE_BYTES {
        return Err(AppError::filmstrip_unavailable(
            "Некорректный кэш раскадровки",
        ));
    }
    let bytes = tokio::fs::read(&artifact.path)
        .await
        .map_err(|error| AppError::internal("read generated filmstrip", error))?;
    if bytes.len() != artifact.size_bytes as usize {
        return Err(AppError::filmstrip_unavailable(
            "Раскадровка изменилась во время чтения",
        ));
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CONTENT_LENGTH, bytes.len())
        .header(header::CACHE_CONTROL, IMMUTABLE_THUMBNAIL_CACHE)
        .header(header::ETAG, etag)
        .header("x-filmstrip-cells", FILMSTRIP_CELL_COUNT)
        .header("x-filmstrip-cell-width", FILMSTRIP_CELL_WIDTH)
        .header("x-filmstrip-cell-height", FILMSTRIP_CELL_HEIGHT)
        .body(Body::from(bytes))
        .map_err(|error| AppError::internal("build filmstrip response", error))
}

async fn thumbnail_source(state: &AppState, id: &str) -> AppResult<ThumbnailSource> {
    if !valid_composition_source_id(id) {
        return Err(AppError::bad_request("Некорректный ID медиафайла"));
    }
    let entry = state
        .library
        .get(id)
        .await
        .ok_or_else(|| AppError::not_found("Медиафайл не найден"))?;
    let kind = infer_thumbnail_kind(&entry).ok_or_else(|| {
        AppError::unsupported_media_type("Для этого формата предпросмотр не поддерживается")
    })?;
    let path = state
        .library
        .resolve_media_path(&entry)
        .await
        .map_err(|error| {
            tracing::warn!(media.id = %id, %error, "resolve thumbnail source");
            AppError::not_found("Медиафайл не найден")
        })?;
    Ok(ThumbnailSource {
        id: entry.id,
        path,
        kind,
        duration_seconds: entry.duration,
    })
}

async fn filmstrip_source(state: &AppState, id: &str) -> AppResult<ThumbnailSource> {
    let source = thumbnail_source(state, id).await?;
    if source.kind != ThumbnailKind::Video {
        return Err(AppError::unsupported_media_type(
            "Раскадровка доступна только для видео",
        ));
    }
    if !source
        .duration_seconds
        .is_some_and(|value| value.is_finite() && value > 0.0 && value <= 24.0 * 60.0 * 60.0)
    {
        return Err(AppError::filmstrip_unavailable(
            "Для раскадровки нужна длительность видео",
        ));
    }
    Ok(source)
}

fn ensure_thumbnail_support(state: &AppState, kind: ThumbnailKind) -> AppResult<()> {
    let tools = &state.tools;
    let base = tools.ffmpeg
        && tools.ffmpeg_encoders.iter().any(|value| value == "png")
        && tools.ffmpeg_muxers.iter().any(|value| value == "image2");
    let required_filters: &[&str] = match kind {
        ThumbnailKind::Video | ThumbnailKind::Image => &["scale", "pad", "setsar"],
        ThumbnailKind::Audio => &["aformat", "showwavespic"],
    };
    if !base
        || required_filters.iter().any(|required| {
            !tools
                .ffmpeg_filters
                .iter()
                .any(|available| available == required)
        })
    {
        let message = if kind == ThumbnailKind::Audio {
            "Waveform предпросмотр недоступен в установленной сборке FFmpeg"
        } else {
            "Предпросмотр недоступен в установленной сборке FFmpeg"
        };
        return Err(AppError::thumbnail_unavailable(message));
    }
    Ok(())
}

fn ensure_filmstrip_support(state: &AppState) -> AppResult<()> {
    let tools = &state.tools;
    let required_filters = ["scale", "pad", "setsar", "hstack"];
    if !tools.ffmpeg
        || !tools.ffmpeg_encoders.iter().any(|value| value == "png")
        || !tools.ffmpeg_muxers.iter().any(|value| value == "image2")
        || required_filters.iter().any(|required| {
            !tools
                .ffmpeg_filters
                .iter()
                .any(|available| available == required)
        })
    {
        return Err(AppError::filmstrip_unavailable(
            "Раскадровка недоступна в установленной сборке FFmpeg",
        ));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn etag_matches(value: Option<&HeaderValue>, etag: &str) -> bool {
    value
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .map(str::trim)
                .any(|candidate| candidate == "*" || candidate == etag)
        })
}

struct RequestCancellation(tokio_util::sync::CancellationToken);

impl RequestCancellation {
    fn new() -> Self {
        Self(tokio_util::sync::CancellationToken::new())
    }

    fn token(&self) -> &tokio_util::sync::CancellationToken {
        &self.0
    }
}

impl Drop for RequestCancellation {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryMetadataPut {
    #[serde(default)]
    title: Option<String>,
    favorite: bool,
    tags: Vec<String>,
}

#[derive(Debug, Default)]
enum TitlePatch {
    #[default]
    Missing,
    Null,
    Value(String),
}

impl<'de> Deserialize<'de> for TitlePatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TitlePatchVisitor;

        impl<'de> Visitor<'de> for TitlePatchVisitor {
            type Value = TitlePatch;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a string or null")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(TitlePatch::Null)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(TitlePatch::Null)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                String::deserialize(deserializer).map(TitlePatch::Value)
            }
        }

        deserializer.deserialize_option(TitlePatchVisitor)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryMetadataPatch {
    #[serde(default)]
    title: TitlePatch,
    #[serde(default)]
    favorite: Option<bool>,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

pub async fn library_metadata_put_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    crate::error::ApiJson(payload): crate::error::ApiJson<LibraryMetadataPut>,
) -> AppResult<Json<LibraryEntryResponse>> {
    let entry = state
        .library
        .get(&id)
        .await
        .ok_or_else(|| AppError::not_found("Медиафайл не найден"))?;
    let title = validate_title(payload.title)?;
    let tags = validate_tags(payload.tags)?;
    let metadata = state
        .db
        .replace_library_metadata(&id, title, payload.favorite, tags)
        .await
        .map_err(|error| AppError::internal("replace library metadata", error))?;
    Ok(Json(LibraryEntryResponse::new(entry, Some(metadata))))
}

pub async fn library_metadata_patch_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    crate::error::ApiJson(payload): crate::error::ApiJson<LibraryMetadataPatch>,
) -> AppResult<Json<LibraryEntryResponse>> {
    let entry = state
        .library
        .get(&id)
        .await
        .ok_or_else(|| AppError::not_found("Медиафайл не найден"))?;
    let title = match payload.title {
        TitlePatch::Missing => None,
        TitlePatch::Null => Some(None),
        TitlePatch::Value(title) => Some(validate_title(Some(title))?),
    };
    let tags = payload.tags.map(validate_tags).transpose()?;
    if title.is_none() && payload.favorite.is_none() && tags.is_none() {
        return Err(AppError::bad_request(
            "PATCH не содержит изменений metadata",
        ));
    }
    let metadata = state
        .db
        .update_library_metadata(
            &id,
            LibraryMetadataChanges {
                title,
                favorite: payload.favorite,
                tags,
            },
        )
        .await
        .map_err(|error| AppError::internal("patch library metadata", error))?;
    Ok(Json(LibraryEntryResponse::new(entry, Some(metadata))))
}

fn validate_title(title: Option<String>) -> AppResult<Option<String>> {
    normalize_library_title(title).map_err(|_| {
        AppError::bad_request(format!(
            "Название должно быть не длиннее {MAX_LIBRARY_TITLE_CHARS} символов и не содержать управляющих знаков",
        ))
    })
}

fn validate_tags(tags: Vec<String>) -> AppResult<Vec<String>> {
    normalize_library_tags(tags).map_err(|_| {
        AppError::bad_request(format!(
            "Разрешено до {MAX_LIBRARY_TAGS} уникальных тегов длиной до {MAX_LIBRARY_TAG_CHARS} символов",
        ))
    })
}

/// `DELETE /api/library/:id` — remove a library entry and delete its file.
pub async fn library_delete_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<StatusCode> {
    let entry = state.library.get(&id).await;
    if entry.as_ref().is_some_and(|entry| entry.kind == "source") {
        let references = state
            .db
            .composition_source_reference_count(&id)
            .await
            .map_err(|error| AppError::internal("check composition source references", error))?;
        if references > 0 {
            return Err(AppError::conflict(
                "Источник используется сохранённой композицией",
            ));
        }
        super::proxy::cleanup_source_proxies(&state, &id).await?;
    }
    if state.library.remove(&id).await {
        if let Err(error) = state.thumbnail_service.remove_source(&id).await {
            tracing::warn!(media.id = %id, %error, "remove thumbnail cache");
        }
        state
            .db
            .delete_library_metadata(&id)
            .await
            .map_err(|error| AppError::internal("delete library metadata", error))?;
        if let Err(error) = state.media_index.remove(&id).await {
            tracing::warn!(media.id = %id, %error, "remove media from search index");
        }
        if let Some(entry) = entry.filter(|e| e.kind == "output") {
            let _ = state.db.cache_delete_filename(&entry.filename).await;
        }
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Медиафайл не найден"))
    }
}

fn default_search_limit() -> u32 {
    20
}
