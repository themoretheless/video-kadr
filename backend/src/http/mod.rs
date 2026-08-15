//! HTTP adapters. Routers depend on application ports; only production port
//! implementations know about SQLite or runtime capability state.

pub mod policy;
pub mod ports;

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use crate::capabilities::Capabilities;
use crate::db::{
    valid_composition_source_id, CompositionProject, Project, COMPOSITION_PROJECT_MODE,
    COMPOSITION_PROJECT_SCHEMA_VERSION, MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES,
    MAX_COMPOSITION_PROJECT_NAME_BYTES, MAX_COMPOSITION_PROJECT_SOURCES,
};
use crate::error::{ApiJson, AppError, AppResult};
use crate::model::WireSchemaVersion;

use ports::{
    CompositionProjectDraft, CompositionProjectPort, HealthStatus, ProjectDraft, ProjectPort,
    SystemPort,
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
            "/composition-projects/:id",
            get(composition_project_get_handler)
                .merge(put(composition_project_update_handler))
                .delete(composition_project_delete_handler),
        )
        .layer(DefaultBodyLimit::max(MAX_COMPOSITION_PROJECT_REQUEST_BYTES))
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
    document: Value,
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
    ApiJson(body): ApiJson<CompositionProjectSaveRequest>,
) -> AppResult<(StatusCode, Json<CompositionProject>)> {
    let draft = parse_composition_project_body(body)?;
    port.create(draft)
        .await
        .map(|project| (StatusCode::CREATED, Json(project)))
        .map_err(|error| AppError::internal("create composition project", error))
}

async fn composition_project_list_handler(
    State(port): State<Arc<dyn CompositionProjectPort>>,
) -> AppResult<Json<Vec<CompositionProject>>> {
    port.list()
        .await
        .map(Json)
        .map_err(|error| AppError::internal("list composition projects", error))
}

async fn composition_project_get_handler(
    State(port): State<Arc<dyn CompositionProjectPort>>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<CompositionProject>> {
    port.get(&id)
        .await
        .map_err(|error| AppError::internal("get composition project", error))?
        .map(Json)
        .ok_or_else(|| AppError::not_found("Композиционный проект не найден"))
}

async fn composition_project_update_handler(
    State(port): State<Arc<dyn CompositionProjectPort>>,
    AxPath(id): AxPath<String>,
    ApiJson(body): ApiJson<CompositionProjectSaveRequest>,
) -> AppResult<Json<CompositionProject>> {
    let draft = parse_composition_project_body(body)?;
    port.update(&id, draft)
        .await
        .map_err(|error| AppError::internal("update composition project", error))?
        .map(Json)
        .ok_or_else(|| AppError::not_found("Композиционный проект не найден"))
}

async fn composition_project_delete_handler(
    State(port): State<Arc<dyn CompositionProjectPort>>,
    AxPath(id): AxPath<String>,
) -> AppResult<StatusCode> {
    match port.delete(&id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(AppError::not_found("Композиционный проект не найден")),
        Err(error) => Err(AppError::internal("delete composition project", error)),
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
