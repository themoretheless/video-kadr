//! HTTP adapters. Routers depend on application ports; only production port
//! implementations know about SQLite or runtime capability state.

pub mod policy;
pub mod ports;

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::extract::{Path as AxPath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::capabilities::Capabilities;
use crate::db::{
    valid_composition_source_id, CompositionProject, Project, SpaceRole, COMPOSITION_PROJECT_MODE,
    COMPOSITION_PROJECT_SCHEMA_VERSION, MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES,
    MAX_COMPOSITION_PROJECT_NAME_BYTES, MAX_COMPOSITION_PROJECT_SOURCES,
};
use crate::domain::project_collaboration::{ProjectRole, ReviewThread};
use crate::error::{ApiJson, AppError, AppResult};
use crate::model::WireSchemaVersion;

use ports::{
    AuthPort, CompositionProjectDraft, CompositionProjectPort, HealthStatus, ProjectDraft,
    ProjectPort, ProjectReviewPort, SpacePort, SystemPort,
};

const MAX_PROJECT_JSON_BYTES: usize = 64 * 1024;
const COMPOSITION_DOCUMENT_SCHEMA_VERSION: u64 = 1;
const MAX_COMPOSITION_PROJECT_REQUEST_BYTES: usize =
    MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024;

pub fn system_router(port: Arc<dyn SystemPort>) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/capabilities", get(capabilities_handler))
        .with_state(port)
}

pub fn auth_router(port: Arc<dyn AuthPort>) -> Router {
    Router::new()
        .route("/auth/register", post(auth_register_handler))
        .route("/auth/login", post(auth_login_handler))
        .route("/auth/session", get(auth_session_handler))
        .route("/auth/logout", post(auth_logout_handler))
        .layer(DefaultBodyLimit::max(4 * 1024))
        .with_state(port)
}

pub fn project_router(port: Arc<dyn ProjectPort>) -> Router {
    Router::new()
        .route(
            "/projects",
            post(project_upsert_handler).get(project_list_handler),
        )
        .route("/projects/by-video/:videoId", get(project_by_video_handler))
        .route(
            "/projects/:id",
            get(project_get_handler).delete(project_delete_handler),
        )
        .with_state(port)
}

pub fn composition_project_router(port: Arc<dyn CompositionProjectPort>) -> Router {
    Router::new()
        .route(
            "/composition-projects",
            post(composition_project_create_handler).get(composition_project_list_handler),
        )
        .route(
            "/composition-projects/events",
            get(composition_project_events_handler),
        )
        .route(
            "/composition-projects/:id",
            get(composition_project_get_handler)
                .merge(put(composition_project_update_handler))
                .delete(composition_project_delete_handler),
        )
        .layer(DefaultBodyLimit::max(MAX_COMPOSITION_PROJECT_REQUEST_BYTES))
        .with_state(port)
}

pub fn project_review_router(port: Arc<dyn ProjectReviewPort>) -> Router {
    Router::new()
        .route(
            "/composition-projects/:id/reviews",
            post(review_create_handler).get(review_list_handler),
        )
        .route(
            "/composition-projects/:id/members",
            get(review_member_list_handler),
        )
        .route(
            "/composition-projects/:id/review-audit",
            get(review_audit_list_handler),
        )
        .route(
            "/composition-projects/:id/review-shares",
            post(review_share_create_handler),
        )
        .route(
            "/composition-projects/:id/review-shares/:shareId/revoke",
            post(review_share_revoke_handler),
        )
        .route("/review-shares/:token", get(review_share_open_handler))
        .route(
            "/composition-projects/:id/members/:actor",
            put(review_member_set_handler),
        )
        .route(
            "/composition-projects/:id/ownership-transfer",
            post(ownership_transfer_handler),
        )
        .route("/review-threads/:id/replies", post(review_reply_handler))
        .route(
            "/review-threads/:id/resolution",
            put(review_resolution_handler),
        )
        .layer(DefaultBodyLimit::max(16 * 1024))
        .with_state(port)
}

pub fn space_router(port: Arc<dyn SpacePort>) -> Router {
    Router::new()
        .route(
            "/spaces",
            post(space_create_handler).get(space_list_handler),
        )
        .route(
            "/spaces/:id",
            axum::routing::patch(space_rename_handler).delete(space_delete_handler),
        )
        .route("/spaces/:id/members", get(space_member_list_handler))
        .route("/spaces/:id/invites", post(space_invite_create_handler))
        .route(
            "/space-invites/:token/accept",
            post(space_invite_accept_handler),
        )
        .route(
            "/spaces/:id/members/:actor",
            put(space_member_set_handler).delete(space_member_delete_handler),
        )
        .route(
            "/spaces/:id/ownership-transfer",
            post(space_ownership_transfer_handler),
        )
        .route(
            "/spaces/:id/templates",
            get(space_template_list_handler).post(space_template_create_handler),
        )
        .route(
            "/spaces/:id/templates/:templateId",
            put(space_template_update_handler).delete(space_template_delete_handler),
        )
        .route(
            "/spaces/:id/brand-kit",
            get(space_brand_get_handler).put(space_brand_update_handler),
        )
        .layer(DefaultBodyLimit::max(
            crate::db::MAX_SPACE_TEMPLATE_BYTES + 16 * 1024,
        ))
        .with_state(port)
}

async fn health_handler(State(port): State<Arc<dyn SystemPort>>) -> Json<HealthStatus> {
    Json(port.health())
}

async fn capabilities_handler(State(port): State<Arc<dyn SystemPort>>) -> Json<Capabilities> {
    Json(port.capabilities())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectUpsertRequest {
    #[serde(rename = "schemaVersion", default)]
    _schema_version: WireSchemaVersion,
    #[serde(default)]
    video_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    video: Option<Value>,
    #[serde(default)]
    edit: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompositionProjectSaveRequest {
    schema_version: u32,
    mode: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    space_id: Option<String>,
    #[serde(default)]
    base_revision: Option<u64>,
    document: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthCredentialsRequest {
    username: String,
    password: String,
}

async fn auth_register_handler(
    State(port): State<Arc<dyn AuthPort>>,
    ApiJson(body): ApiJson<AuthCredentialsRequest>,
) -> AppResult<(
    StatusCode,
    [(axum::http::HeaderName, axum::http::HeaderValue); 1],
    Json<crate::db::AuthSession>,
)> {
    validate_auth_credentials(&body)?;
    port.register(body.username.trim(), &body.password)
        .await
        .map_err(|error| AppError::internal("register auth user", error))?
        .map(|session| {
            let cookie = auth_cookie(&session.token, crate::db::AUTH_SESSION_TTL_SECS);
            (
                StatusCode::CREATED,
                [(header::SET_COOKIE, cookie)],
                Json(session),
            )
        })
        .ok_or_else(|| AppError::conflict("Пользователь с таким именем уже существует"))
}

async fn auth_login_handler(
    State(port): State<Arc<dyn AuthPort>>,
    ApiJson(body): ApiJson<AuthCredentialsRequest>,
) -> AppResult<(
    [(axum::http::HeaderName, axum::http::HeaderValue); 1],
    Json<crate::db::AuthSession>,
)> {
    validate_auth_credentials_shape(&body)?;
    port.login(body.username.trim(), &body.password)
        .await
        .map_err(|error| AppError::internal("login auth user", error))?
        .map(|session| {
            let cookie = auth_cookie(&session.token, crate::db::AUTH_SESSION_TTL_SECS);
            ([(header::SET_COOKIE, cookie)], Json(session))
        })
        .ok_or_else(|| AppError::unauthorized("Неверное имя пользователя или пароль"))
}

async fn auth_session_handler(
    State(port): State<Arc<dyn AuthPort>>,
    headers: HeaderMap,
) -> AppResult<Json<crate::db::AuthUser>> {
    let token = bearer_token(&headers)?;
    port.resolve(token)
        .await
        .map_err(|error| AppError::internal("resolve auth session", error))?
        .map(Json)
        .ok_or_else(|| AppError::unauthorized("Сессия недействительна или истекла"))
}

async fn auth_logout_handler(
    State(port): State<Arc<dyn AuthPort>>,
    headers: HeaderMap,
) -> AppResult<(
    StatusCode,
    [(axum::http::HeaderName, axum::http::HeaderValue); 1],
)> {
    let token = bearer_token(&headers)?;
    if port
        .logout(token)
        .await
        .map_err(|error| AppError::internal("logout auth session", error))?
    {
        Ok((
            StatusCode::NO_CONTENT,
            [(header::SET_COOKIE, expired_auth_cookie())],
        ))
    } else {
        Err(AppError::unauthorized("Сессия недействительна или истекла"))
    }
}

fn auth_cookie(token: &str, max_age: u64) -> axum::http::HeaderValue {
    format!("video_kadr_session={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}")
        .parse()
        .expect("session token and fixed cookie attributes are header-safe")
}

fn expired_auth_cookie() -> axum::http::HeaderValue {
    "video_kadr_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"
        .parse()
        .expect("fixed expired cookie is header-safe")
}

fn bearer_token(headers: &HeaderMap) -> AppResult<&str> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::unauthorized("Требуется Bearer-сессия"))?;
    Ok(value)
}

fn composition_session_token(headers: &HeaderMap) -> AppResult<&str> {
    bearer_token(headers).or_else(|_| {
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok())
            .and_then(|cookies| {
                cookies
                    .split(';')
                    .map(str::trim)
                    .find_map(|cookie| cookie.strip_prefix("video_kadr_session="))
            })
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::unauthorized("Требуется сессия"))
    })
}

fn validate_auth_credentials(body: &AuthCredentialsRequest) -> AppResult<()> {
    validate_auth_credentials_shape(body)?;
    if body.password.len() < 12 {
        return Err(AppError::bad_request(
            "Пароль должен содержать от 12 до 1024 байт",
        ));
    }
    Ok(())
}

fn validate_auth_credentials_shape(body: &AuthCredentialsRequest) -> AppResult<()> {
    let username = body.username.trim();
    if username.len() < 3
        || username.len() > 64
        || !username
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(AppError::bad_request("Имя пользователя должно содержать 3–64 символа: A-Z, 0-9, точка, дефис или подчёркивание"));
    }
    if body.password.is_empty() || body.password.len() > 1024 {
        return Err(AppError::bad_request("Некорректные учётные данные"));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewCommentRequest {
    body: String,
    #[serde(default)]
    timeline_tick: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewResolutionRequest {
    resolved: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewMemberRequest {
    role: ProjectRole,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OwnershipTransferRequest {
    target_actor: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpaceCreateRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpaceRenameRequest {
    name: String,
    base_updated_at: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpaceMemberRequest {
    role: SpaceRole,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpaceInviteCreateRequest {
    role: SpaceRole,
    ttl_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpaceTemplateCreateRequest {
    template: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpaceTemplateUpdateRequest {
    base_revision: u64,
    template: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpaceBrandUpdateRequest {
    base_revision: u64,
    kit: crate::db::BrandKitPayload,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewShareCreateRequest {
    ttl_seconds: u64,
}

async fn project_upsert_handler(
    State(port): State<Arc<dyn ProjectPort>>,
    ApiJson(body): ApiJson<ProjectUpsertRequest>,
) -> AppResult<Json<Project>> {
    let draft = parse_project_body(body)?;
    port.upsert(draft)
        .await
        .map(Json)
        .map_err(|error| AppError::internal("upsert project", error))
}

async fn project_list_handler(
    State(port): State<Arc<dyn ProjectPort>>,
) -> AppResult<Json<Vec<Project>>> {
    port.list()
        .await
        .map(Json)
        .map_err(|error| AppError::internal("list projects", error))
}

async fn project_get_handler(
    State(port): State<Arc<dyn ProjectPort>>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<Project>> {
    port.get(&id)
        .await
        .map_err(|error| AppError::internal("get project", error))?
        .map(Json)
        .ok_or_else(|| AppError::not_found("Проект не найден"))
}

async fn project_by_video_handler(
    State(port): State<Arc<dyn ProjectPort>>,
    AxPath(video_id): AxPath<String>,
) -> AppResult<Json<Project>> {
    port.get_by_video(&video_id)
        .await
        .map_err(|error| AppError::internal("get project by video", error))?
        .map(Json)
        .ok_or_else(|| AppError::not_found("Проект не найден"))
}

async fn project_delete_handler(
    State(port): State<Arc<dyn ProjectPort>>,
    AxPath(id): AxPath<String>,
) -> AppResult<StatusCode> {
    match port.delete(&id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(AppError::not_found("Проект не найден")),
        Err(error) => Err(AppError::internal("delete project", error)),
    }
}

async fn composition_project_create_handler(
    State(port): State<Arc<dyn CompositionProjectPort>>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<CompositionProjectSaveRequest>,
) -> AppResult<(StatusCode, Json<CompositionProject>)> {
    let draft = parse_composition_project_body(body)?;
    let actor = authenticated_composition_actor(&port, &headers).await?;
    port.create(draft, &actor)
        .await
        .map(|project| (StatusCode::CREATED, Json(project)))
        .map_err(composition_project_error)
}

async fn composition_project_list_handler(
    State(port): State<Arc<dyn CompositionProjectPort>>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let actor = authenticated_composition_actor(&port, &headers).await?;
    let projects = port
        .list(&actor)
        .await
        .map_err(|error| AppError::internal("list composition projects", error))?;
    let mut digest = Sha256::new();
    for project in &projects {
        digest.update(project.id.as_bytes());
        digest.update([0]);
        digest.update(project.revision.to_be_bytes());
    }
    let digest = digest.finalize();
    let etag = axum::http::HeaderValue::from_str(&format!("\"projects-{digest:x}\""))
        .map_err(|error| AppError::internal("build project list ETag", error))?;
    if headers.get(header::IF_NONE_MATCH) == Some(&etag) {
        return Ok(([(header::ETAG, etag)], StatusCode::NOT_MODIFIED).into_response());
    }
    Ok(([(header::ETAG, etag)], Json(projects)).into_response())
}

async fn composition_project_get_handler(
    State(port): State<Arc<dyn CompositionProjectPort>>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let actor = authenticated_composition_actor(&port, &headers).await?;
    let project = port
        .get(&id, &actor)
        .await
        .map_err(|error| AppError::internal("get composition project", error))?
        .ok_or_else(|| AppError::not_found("Композиционный проект не найден"))?;
    let etag = axum::http::HeaderValue::from_str(&format!("\"revision-{}\"", project.revision))
        .map_err(|error| AppError::internal("build project revision ETag", error))?;
    if headers.get(header::IF_NONE_MATCH) == Some(&etag) {
        return Ok(([(header::ETAG, etag)], StatusCode::NOT_MODIFIED).into_response());
    }
    Ok(([(header::ETAG, etag)], Json(project)).into_response())
}

async fn composition_project_update_handler(
    State(port): State<Arc<dyn CompositionProjectPort>>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<CompositionProjectSaveRequest>,
) -> AppResult<Json<CompositionProject>> {
    let draft = parse_composition_project_body(body)?;
    if draft.base_revision.is_none() {
        return Err(AppError::bad_request(
            "baseRevision обязателен при обновлении проекта",
        ));
    }
    let actor = authenticated_composition_actor(&port, &headers).await?;
    port.update(&id, &actor, draft)
        .await
        .map_err(composition_project_error)?
        .map(Json)
        .ok_or_else(|| AppError::not_found("Композиционный проект не найден"))
}

async fn composition_project_events_handler(
    State(port): State<Arc<dyn CompositionProjectPort>>,
    headers: HeaderMap,
) -> AppResult<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>> {
    let token = composition_session_token(&headers)?;
    let actor = port
        .authenticate(token)
        .await
        .map_err(|error| AppError::internal("authenticate project event stream", error))?
        .map(|user| user.username)
        .ok_or_else(|| AppError::unauthorized("Сессия недействительна или истекла"))?;
    let mut receiver = port.subscribe();
    let stream = async_stream::stream! {
        loop {
            match receiver.recv().await {
                Ok(change) if change.audience.iter().any(|member| member == &actor) => {
                    match Event::default().event("project").json_data(&change) {
                        Ok(event) => yield Ok(event),
                        Err(error) => tracing::warn!(%error, "serialize project change event"),
                    }
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    yield Ok(Event::default().event("resync").data("{}"));
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

async fn composition_project_delete_handler(
    State(port): State<Arc<dyn CompositionProjectPort>>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    let actor = authenticated_composition_actor(&port, &headers).await?;
    match port.delete(&id, &actor).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(AppError::not_found("Композиционный проект не найден")),
        Err(error) => Err(composition_project_error(error)),
    }
}

async fn authenticated_composition_actor(
    port: &Arc<dyn CompositionProjectPort>,
    headers: &HeaderMap,
) -> AppResult<String> {
    let token = bearer_token(headers)?;
    port.authenticate(token)
        .await
        .map_err(|error| AppError::internal("authenticate composition project request", error))?
        .map(|user| user.username)
        .ok_or_else(|| AppError::unauthorized("Сессия недействительна или истекла"))
}

fn composition_project_error(error: anyhow::Error) -> AppError {
    let message = error.to_string();
    if message.contains("stale composition project revision") {
        AppError::conflict(
            "Проект уже изменён на другом устройстве; обновите его перед сохранением",
        )
    } else if message.contains("role cannot")
        || message.contains("belongs to another collaboration space")
        || message.contains("scoped source")
    {
        AppError::forbidden("Недостаточно прав для операции с проектом")
    } else {
        AppError::internal("composition project", error)
    }
}

async fn review_create_handler(
    State(port): State<Arc<dyn ProjectReviewPort>>,
    AxPath(project_id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<ReviewCommentRequest>,
) -> AppResult<(StatusCode, Json<ReviewThread>)> {
    validate_review_comment_request(&body, true)?;
    let actor = authenticated_review_actor(&port, &headers).await?;
    port.create(&project_id, &actor, body.body.trim(), body.timeline_tick)
        .await
        .map_err(project_review_error)?
        .map(|thread| (StatusCode::CREATED, Json(thread)))
        .ok_or_else(|| AppError::not_found("Композиционный проект не найден"))
}

async fn review_list_handler(
    State(port): State<Arc<dyn ProjectReviewPort>>,
    AxPath(project_id): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<ReviewThread>>> {
    let actor = authenticated_review_actor(&port, &headers).await?;
    port.list(&project_id, &actor)
        .await
        .map(Json)
        .map_err(project_review_error)
}

async fn review_reply_handler(
    State(port): State<Arc<dyn ProjectReviewPort>>,
    AxPath(thread_id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<ReviewCommentRequest>,
) -> AppResult<Json<ReviewThread>> {
    validate_review_comment_request(&body, false)?;
    let actor = authenticated_review_actor(&port, &headers).await?;
    port.reply(&thread_id, &actor, body.body.trim())
        .await
        .map_err(project_review_error)?
        .map(Json)
        .ok_or_else(|| AppError::not_found("Ветка ревью не найдена"))
}

async fn review_resolution_handler(
    State(port): State<Arc<dyn ProjectReviewPort>>,
    AxPath(thread_id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<ReviewResolutionRequest>,
) -> AppResult<Json<ReviewThread>> {
    let actor = authenticated_review_actor(&port, &headers).await?;
    port.resolve(&thread_id, &actor, body.resolved)
        .await
        .map_err(project_review_error)?
        .map(Json)
        .ok_or_else(|| AppError::not_found("Ветка ревью не найдена"))
}

async fn review_member_list_handler(
    State(port): State<Arc<dyn ProjectReviewPort>>,
    AxPath(project_id): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<crate::domain::project_collaboration::ProjectMember>>> {
    let actor = authenticated_review_actor(&port, &headers).await?;
    port.list_members(&project_id, &actor)
        .await
        .map(Json)
        .map_err(project_review_error)
}

async fn review_audit_list_handler(
    State(port): State<Arc<dyn ProjectReviewPort>>,
    AxPath(project_id): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<crate::domain::project_collaboration::ReviewAuditEvent>>> {
    let actor = authenticated_review_actor(&port, &headers).await?;
    port.list_audit(&project_id, &actor)
        .await
        .map(Json)
        .map_err(project_review_error)
}

async fn review_share_create_handler(
    State(port): State<Arc<dyn ProjectReviewPort>>,
    AxPath(project_id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<ReviewShareCreateRequest>,
) -> AppResult<(
    StatusCode,
    Json<crate::domain::project_collaboration::ReviewShareCreated>,
)> {
    let actor = authenticated_review_actor(&port, &headers).await?;
    port.create_share(&project_id, &actor, body.ttl_seconds)
        .await
        .map_err(project_review_error)?
        .map(|share| (StatusCode::CREATED, Json(share)))
        .ok_or_else(|| AppError::forbidden("Только owner может создать review-ссылку"))
}

async fn review_share_open_handler(
    State(port): State<Arc<dyn ProjectReviewPort>>,
    AxPath(token): AxPath<String>,
) -> AppResult<Json<crate::domain::project_collaboration::SharedReview>> {
    port.open_share(&token)
        .await
        .map_err(project_review_error)?
        .map(Json)
        .ok_or_else(|| AppError::not_found("Review-ссылка недействительна или истекла"))
}

async fn review_share_revoke_handler(
    State(port): State<Arc<dyn ProjectReviewPort>>,
    AxPath((project_id, share_id)): AxPath<(String, String)>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    let actor = authenticated_review_actor(&port, &headers).await?;
    match port.revoke_share(&project_id, &share_id, &actor).await {
        Ok(Some(true)) => Ok(StatusCode::NO_CONTENT),
        Ok(Some(false)) => Err(AppError::not_found("Review-ссылка не найдена")),
        Ok(None) => Err(AppError::forbidden(
            "Только owner может отозвать review-ссылку",
        )),
        Err(error) => Err(project_review_error(error)),
    }
}

async fn review_member_set_handler(
    State(port): State<Arc<dyn ProjectReviewPort>>,
    AxPath((project_id, actor)): AxPath<(String, String)>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<ReviewMemberRequest>,
) -> AppResult<Json<crate::domain::project_collaboration::ProjectMember>> {
    if !valid_review_actor(&actor) {
        return Err(AppError::bad_request("Некорректный участник ревью"));
    }
    let requester = authenticated_review_actor(&port, &headers).await?;
    port.set_member(&project_id, &requester, &actor, body.role)
        .await
        .map_err(project_review_error)?
        .map(Json)
        .ok_or_else(|| AppError::forbidden("Только участник проекта может менять роли"))
}

async fn ownership_transfer_handler(
    State(port): State<Arc<dyn ProjectReviewPort>>,
    AxPath(project_id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<OwnershipTransferRequest>,
) -> AppResult<Json<crate::domain::project_collaboration::ProjectMember>> {
    if !valid_review_actor(&body.target_actor) {
        return Err(AppError::bad_request("Некорректный новый владелец"));
    }
    let requester = authenticated_review_actor(&port, &headers).await?;
    port.transfer_ownership(&project_id, &requester, &body.target_actor)
        .await
        .map_err(project_review_error)?
        .map(Json)
        .ok_or_else(|| AppError::forbidden("Только owner может передать проект"))
}

async fn space_create_handler(
    State(port): State<Arc<dyn SpacePort>>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<SpaceCreateRequest>,
) -> AppResult<(StatusCode, Json<crate::db::Space>)> {
    let actor = authenticated_space_actor(&port, &headers).await?;
    port.create(&actor, &body.name)
        .await
        .map(|space| (StatusCode::CREATED, Json(space)))
        .map_err(space_error)
}

async fn space_list_handler(
    State(port): State<Arc<dyn SpacePort>>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<crate::db::Space>>> {
    let actor = authenticated_space_actor(&port, &headers).await?;
    port.list(&actor).await.map(Json).map_err(space_error)
}

async fn space_rename_handler(
    State(port): State<Arc<dyn SpacePort>>,
    AxPath(space_id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<SpaceRenameRequest>,
) -> AppResult<Json<crate::db::Space>> {
    let requester = authenticated_space_actor(&port, &headers).await?;
    port.rename(&space_id, &requester, &body.name, body.base_updated_at)
        .await
        .map_err(space_error)?
        .map(Json)
        .ok_or_else(|| AppError::forbidden("Только owner может переименовать пространство"))
}

async fn space_delete_handler(
    State(port): State<Arc<dyn SpacePort>>,
    AxPath(space_id): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    let requester = authenticated_space_actor(&port, &headers).await?;
    port.delete_empty(&space_id, &requester)
        .await
        .map_err(space_error)?
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or_else(|| AppError::forbidden("Только owner может удалить пространство"))
}

async fn space_member_list_handler(
    State(port): State<Arc<dyn SpacePort>>,
    AxPath(space_id): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<crate::db::SpaceMember>>> {
    let actor = authenticated_space_actor(&port, &headers).await?;
    port.list_members(&space_id, &actor)
        .await
        .map(Json)
        .map_err(space_error)
}

async fn space_invite_create_handler(
    State(port): State<Arc<dyn SpacePort>>,
    AxPath(space_id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<SpaceInviteCreateRequest>,
) -> AppResult<(StatusCode, Json<crate::db::SpaceInviteCreated>)> {
    let actor = authenticated_space_actor(&port, &headers).await?;
    port.create_invite(&space_id, &actor, body.role, body.ttl_seconds)
        .await
        .map_err(space_error)?
        .map(|invite| (StatusCode::CREATED, Json(invite)))
        .ok_or_else(|| AppError::forbidden("Только owner может создать Space invite"))
}

async fn space_invite_accept_handler(
    State(port): State<Arc<dyn SpacePort>>,
    AxPath(token): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<Json<crate::db::Space>> {
    let actor = authenticated_space_actor(&port, &headers).await?;
    port.accept_invite(&token, &actor)
        .await
        .map_err(space_error)?
        .map(Json)
        .ok_or_else(|| AppError::not_found("Space invite недействителен, использован или истёк"))
}

async fn space_member_set_handler(
    State(port): State<Arc<dyn SpacePort>>,
    AxPath((space_id, actor)): AxPath<(String, String)>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<SpaceMemberRequest>,
) -> AppResult<Json<crate::db::SpaceMember>> {
    if !valid_review_actor(&actor) {
        return Err(AppError::bad_request("Некорректный участник пространства"));
    }
    let requester = authenticated_space_actor(&port, &headers).await?;
    port.set_member(&space_id, &requester, &actor, body.role)
        .await
        .map_err(space_error)?
        .map(Json)
        .ok_or_else(|| AppError::forbidden("Только owner может менять участников пространства"))
}

async fn space_member_delete_handler(
    State(port): State<Arc<dyn SpacePort>>,
    AxPath((space_id, actor)): AxPath<(String, String)>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    if !valid_review_actor(&actor) {
        return Err(AppError::bad_request("Некорректный участник пространства"));
    }
    let requester = authenticated_space_actor(&port, &headers).await?;
    port.remove_member(&space_id, &requester, &actor)
        .await
        .map_err(space_error)?
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or_else(|| AppError::forbidden("Только owner может удалять участников пространства"))
}

async fn space_ownership_transfer_handler(
    State(port): State<Arc<dyn SpacePort>>,
    AxPath(space_id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<OwnershipTransferRequest>,
) -> AppResult<Json<crate::db::SpaceMember>> {
    if !valid_review_actor(&body.target_actor) {
        return Err(AppError::bad_request(
            "Некорректный новый владелец пространства",
        ));
    }
    let requester = authenticated_space_actor(&port, &headers).await?;
    port.transfer_ownership(&space_id, &requester, &body.target_actor)
        .await
        .map_err(space_error)?
        .map(Json)
        .ok_or_else(|| AppError::forbidden("Только owner может передать пространство"))
}

async fn space_template_list_handler(
    State(port): State<Arc<dyn SpacePort>>,
    AxPath(space_id): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<crate::db::SpaceTemplate>>> {
    let actor = authenticated_space_actor(&port, &headers).await?;
    port.list_templates(&space_id, &actor)
        .await
        .map(Json)
        .map_err(space_error)
}

async fn space_template_create_handler(
    State(port): State<Arc<dyn SpacePort>>,
    AxPath(space_id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<SpaceTemplateCreateRequest>,
) -> AppResult<(StatusCode, Json<crate::db::SpaceTemplate>)> {
    let actor = authenticated_space_actor(&port, &headers).await?;
    port.create_template(&space_id, &actor, &body.template)
        .await
        .map_err(space_error)?
        .map(|template| (StatusCode::CREATED, Json(template)))
        .ok_or_else(|| AppError::forbidden("Только owner/editor может публиковать шаблоны"))
}

async fn space_template_update_handler(
    State(port): State<Arc<dyn SpacePort>>,
    AxPath((space_id, template_id)): AxPath<(String, String)>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<SpaceTemplateUpdateRequest>,
) -> AppResult<Json<crate::db::SpaceTemplate>> {
    let actor = authenticated_space_actor(&port, &headers).await?;
    port.update_template(
        &space_id,
        &template_id,
        &actor,
        body.base_revision,
        &body.template,
    )
    .await
    .map_err(space_error)?
    .map(Json)
    .ok_or_else(|| AppError::forbidden("Шаблон не найден или недостаточно прав"))
}

async fn space_template_delete_handler(
    State(port): State<Arc<dyn SpacePort>>,
    AxPath((space_id, template_id)): AxPath<(String, String)>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    let actor = authenticated_space_actor(&port, &headers).await?;
    if port
        .delete_template(&space_id, &template_id, &actor)
        .await
        .map_err(space_error)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::forbidden(
            "Шаблон не найден или недостаточно прав",
        ))
    }
}

async fn space_brand_get_handler(
    State(port): State<Arc<dyn SpacePort>>,
    AxPath(space_id): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<Json<crate::db::SpaceBrandKit>> {
    let actor = authenticated_space_actor(&port, &headers).await?;
    port.get_brand_kit(&space_id, &actor)
        .await
        .map(Json)
        .map_err(space_error)
}

async fn space_brand_update_handler(
    State(port): State<Arc<dyn SpacePort>>,
    AxPath(space_id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<SpaceBrandUpdateRequest>,
) -> AppResult<Json<crate::db::SpaceBrandKit>> {
    let actor = authenticated_space_actor(&port, &headers).await?;
    port.update_brand_kit(&space_id, &actor, body.base_revision, body.kit)
        .await
        .map_err(space_error)?
        .map(Json)
        .ok_or_else(|| AppError::forbidden("Только owner/editor может менять бренд-кит"))
}

async fn authenticated_space_actor(
    port: &Arc<dyn SpacePort>,
    headers: &HeaderMap,
) -> AppResult<String> {
    let token = bearer_token(headers)?;
    port.authenticate(token)
        .await
        .map_err(|error| AppError::internal("authenticate space request", error))?
        .map(|user| user.username)
        .ok_or_else(|| AppError::unauthorized("Сессия недействительна или истекла"))
}

fn space_error(error: anyhow::Error) -> AppError {
    let message = error.to_string();
    if message.contains("revision conflict") {
        AppError::conflict("Ресурс пространства уже изменён в другом клиенте")
    } else if message.contains("space not empty") {
        AppError::conflict("Сначала удалите проекты и медиа пространства")
    } else if message.contains("target must be a space member")
        || message.contains("transfer target must differ")
    {
        AppError::bad_request(message)
    } else if message.contains("quota exceeded") {
        AppError::conflict("В пространстве достигнут лимит шаблонов")
    } else if message.contains("invalid space")
        || message.contains("space invite TTL")
        || message.contains("invite a space owner")
        || message.contains("size limit")
        || message.contains("brand ")
        || message.contains("duplicate brand")
        || message.contains("unsupported brand")
    {
        AppError::bad_request(message)
    } else if message.contains("not a space member")
        || message.contains("cannot manage space")
        || message.contains("cannot create space invite")
        || message.contains("requires transfer")
    {
        AppError::forbidden("Недостаточно прав для операции с пространством")
    } else {
        AppError::internal("space operation", error)
    }
}

fn validate_review_comment_request(body: &ReviewCommentRequest, allow_tick: bool) -> AppResult<()> {
    if body.body.trim().is_empty() || body.body.len() > 8 * 1024 {
        return Err(AppError::bad_request(
            "Комментарий должен содержать от 1 до 8192 байт",
        ));
    }
    if !allow_tick && body.timeline_tick != 0 {
        return Err(AppError::bad_request(
            "Ответ наследует таймкод исходной ветки",
        ));
    }
    Ok(())
}

async fn authenticated_review_actor(
    port: &Arc<dyn ProjectReviewPort>,
    headers: &HeaderMap,
) -> AppResult<String> {
    let token = bearer_token(headers)?;
    port.authenticate(token)
        .await
        .map_err(|error| AppError::internal("authenticate review request", error))?
        .map(|user| user.username)
        .ok_or_else(|| AppError::unauthorized("Сессия недействительна или истекла"))
}

fn valid_review_actor(actor: &str) -> bool {
    !actor.is_empty()
        && actor.len() <= 64
        && actor
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

fn project_review_error(error: anyhow::Error) -> AppError {
    let message = error.to_string();
    if message.contains("not a project member")
        || message.contains("role cannot")
        || message.contains("ownership transfer")
        || message.contains("already owns project")
    {
        AppError::forbidden("Недостаточно прав для операции с проектом")
    } else if message.contains("invalid review") {
        AppError::bad_request(message)
    } else if message.contains("resolved") {
        AppError::conflict("Ветка ревью уже закрыта")
    } else {
        AppError::internal("project review", error)
    }
}

fn parse_project_body(body: ProjectUpsertRequest) -> AppResult<ProjectDraft> {
    let video_id = body
        .video_id
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::bad_request("videoId обязателен"))?;
    let video = body
        .video
        .ok_or_else(|| AppError::bad_request("video обязателен"))?;
    if !video.is_object() {
        return Err(AppError::bad_request("video обязателен"));
    }
    let edit = body
        .edit
        .ok_or_else(|| AppError::bad_request("edit обязателен"))?;
    if !edit.is_object() {
        return Err(AppError::bad_request("edit обязателен"));
    }
    ensure_project_json_size("video", &video)?;
    ensure_project_json_size("edit", &edit)?;
    let name = body
        .name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| video.get("title").and_then(Value::as_str))
        .or_else(|| video.get("filename").and_then(Value::as_str))
        .unwrap_or("Без названия")
        .to_owned();
    Ok(ProjectDraft {
        video_id,
        name,
        video,
        edit,
    })
}

fn ensure_project_json_size(field: &str, value: &Value) -> AppResult<()> {
    let size = serde_json::to_vec(value)
        .map_err(|error| AppError::internal("serialize project field", error))?
        .len();
    if size > MAX_PROJECT_JSON_BYTES {
        return Err(AppError::payload_too_large(format!(
            "{field} больше {MAX_PROJECT_JSON_BYTES} байт"
        )));
    }
    Ok(())
}

fn parse_composition_project_body(
    body: CompositionProjectSaveRequest,
) -> AppResult<CompositionProjectDraft> {
    if body.schema_version != COMPOSITION_PROJECT_SCHEMA_VERSION
        || body.mode != COMPOSITION_PROJECT_MODE
    {
        return Err(AppError::bad_request(
            "поддерживается только schemaVersion 2 с mode composition",
        ));
    }
    let document = body
        .document
        .as_object()
        .ok_or_else(|| AppError::bad_request("document должен быть JSON-объектом"))?;
    if document.get("schemaVersion").and_then(Value::as_u64)
        != Some(COMPOSITION_DOCUMENT_SCHEMA_VERSION)
    {
        return Err(AppError::bad_request(
            "composition document должен иметь schemaVersion 1",
        ));
    }
    let document_size = serde_json::to_vec(&body.document)
        .map_err(|error| AppError::internal("serialize composition project document", error))?
        .len();
    if document_size > MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES {
        return Err(AppError::payload_too_large(format!(
            "document больше {MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES} байт"
        )));
    }
    let sources = document
        .get("sources")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::bad_request("document.sources должен быть JSON-объектом"))?;
    if sources.len() > MAX_COMPOSITION_PROJECT_SOURCES {
        return Err(AppError::bad_request(
            "в composition document слишком много sources",
        ));
    }
    let mut source_ids = Vec::with_capacity(sources.len());
    for (source_id, source) in sources {
        if !valid_composition_source_id(source_id) {
            return Err(AppError::bad_request("некорректный source id"));
        }
        let embedded_id = source
            .as_object()
            .and_then(|object| object.get("id"))
            .and_then(Value::as_str);
        if embedded_id != Some(source_id.as_str()) {
            return Err(AppError::bad_request(
                "ключ document.sources должен совпадать с source.id",
            ));
        }
        source_ids.push(source_id.clone());
    }
    let fallback_name = document
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Без названия");
    let name = body
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(fallback_name)
        .trim();
    if name.is_empty() || name.len() > MAX_COMPOSITION_PROJECT_NAME_BYTES {
        return Err(AppError::bad_request(
            "некорректное имя composition project",
        ));
    }
    Ok(CompositionProjectDraft {
        name: name.to_owned(),
        space_id: body.space_id,
        base_revision: body.base_revision,
        document: body.document,
        source_ids,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use serde_json::json;
    use tower::ServiceExt;

    use crate::state::ToolInfo;

    use super::*;
    use ports::RuntimeSystemPort;

    #[derive(Default)]
    struct FakeProjects {
        state: Mutex<FakeProjectState>,
    }

    #[derive(Default)]
    struct FakeProjectState {
        projects: Vec<Project>,
        next_id: u64,
        clock: i64,
    }

    #[axum::async_trait]
    impl ProjectPort for FakeProjects {
        async fn upsert(&self, draft: ProjectDraft) -> anyhow::Result<Project> {
            let mut state = self.state.lock().unwrap();
            state.clock += 1;
            let now = state.clock;
            if let Some(project) = state
                .projects
                .iter_mut()
                .find(|project| project.video_id == draft.video_id)
            {
                project.name = draft.name;
                project.video = draft.video;
                project.edit = draft.edit;
                project.updated_at = now;
                return Ok(project.clone());
            }

            state.next_id += 1;
            let project = Project {
                id: format!("project-{}", state.next_id),
                name: draft.name,
                video_id: draft.video_id,
                video: draft.video,
                edit: draft.edit,
                created_at: now,
                updated_at: now,
            };
            state.projects.push(project.clone());
            Ok(project)
        }

        async fn list(&self) -> anyhow::Result<Vec<Project>> {
            let mut projects = self.state.lock().unwrap().projects.clone();
            projects.sort_by_key(|project| std::cmp::Reverse(project.updated_at));
            Ok(projects)
        }

        async fn get(&self, id: &str) -> anyhow::Result<Option<Project>> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .projects
                .iter()
                .find(|project| project.id == id)
                .cloned())
        }

        async fn get_by_video(&self, video_id: &str) -> anyhow::Result<Option<Project>> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .projects
                .iter()
                .find(|project| project.video_id == video_id)
                .cloned())
        }

        async fn delete(&self, id: &str) -> anyhow::Result<bool> {
            let mut state = self.state.lock().unwrap();
            let before = state.projects.len();
            state.projects.retain(|project| project.id != id);
            Ok(state.projects.len() != before)
        }
    }

    async fn response_json(router: Router, request: Request<Body>) -> (StatusCode, Value) {
        let response = router.oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn system_contract_uses_runtime_port_without_app_state() {
        let router = system_router(Arc::new(RuntimeSystemPort::new(Arc::new(ToolInfo {
            ffmpeg: true,
            ytdlp: false,
            ..ToolInfo::default()
        }))));
        let (status, body) = response_json(
            router,
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "degraded");
        assert_eq!(body["ffmpeg"], true);
    }

    #[tokio::test]
    async fn project_contract_uses_in_memory_fake_without_db_or_files() {
        let router = project_router(Arc::new(FakeProjects::default()));
        let request = Request::builder()
            .method("POST")
            .uri("/projects")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "videoId": "video-1",
                    "video": { "filename": "clip.mp4" },
                    "edit": { "mute": true }
                })
                .to_string(),
            ))
            .unwrap();
        let (status, project) = response_json(router.clone(), request).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(project["name"], "clip.mp4");

        let second_request = Request::builder()
            .method("POST")
            .uri("/projects")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "videoId": "video-1",
                    "name": "renamed clip",
                    "video": { "filename": "renamed.mp4" },
                    "edit": { "mute": false }
                })
                .to_string(),
            ))
            .unwrap();
        let (status, updated) = response_json(router.clone(), second_request).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(updated["id"], project["id"]);
        assert_eq!(updated["createdAt"], project["createdAt"]);
        assert!(updated["updatedAt"].as_i64() >= project["updatedAt"].as_i64());
        assert_eq!(updated["name"], "renamed clip");
        assert_eq!(updated["video"]["filename"], "renamed.mp4");
        assert_eq!(updated["edit"]["mute"], false);

        let (status, by_video) = response_json(
            router.clone(),
            Request::builder()
                .uri("/projects/by-video/video-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(by_video, updated);

        let (status, list) = response_json(
            router,
            Request::builder()
                .uri("/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(list.as_array().unwrap().len(), 1);
    }
}
