//! HTTP adapters. Routers depend on application ports; only production port
//! implementations know about SQLite or runtime capability state.

pub mod policy;
pub mod ports;

use std::sync::Arc;

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use crate::capabilities::Capabilities;
use crate::db::{Project, ProjectRevisionConflict};
use crate::domain::project::{ProjectDocument, ProjectEnvelope};
use crate::error::{ApiJson, AppError, AppResult};
use crate::model::WireSchemaVersion;

use ports::{HealthStatus, ProjectDraft, ProjectPort, SystemPort};

const MAX_PROJECT_JSON_BYTES: usize = 64 * 1024;
const MAX_PROJECT_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

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
        .route("/projects/documents", post(project_document_upsert_handler))
        .route("/projects/documents/:id", get(project_document_get_handler))
        .route(
            "/projects/:id",
            get(project_get_handler).delete(project_delete_handler),
        )
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
struct ProjectDocumentUpsertRequest {
    project_id: String,
    expected_revision: u64,
    document: Value,
}

async fn project_document_upsert_handler(
    State(port): State<Arc<dyn ProjectPort>>,
    ApiJson(body): ApiJson<ProjectDocumentUpsertRequest>,
) -> AppResult<Json<ProjectEnvelope>> {
    if body.project_id.trim().is_empty() {
        return Err(AppError::bad_request("projectId обязателен"));
    }
    ensure_json_size("document", &body.document, MAX_PROJECT_DOCUMENT_BYTES)?;
    let document = ProjectDocument::migrate(body.document)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    port.upsert_document(&body.project_id, body.expected_revision, &document)
        .await
        .map(Json)
        .map_err(|error| {
            if let Some(conflict) = error.downcast_ref::<ProjectRevisionConflict>() {
                AppError::revision_conflict(conflict.expected_revision, conflict.actual_revision)
            } else {
                AppError::internal("upsert project document", error)
            }
        })
}

async fn project_document_get_handler(
    State(port): State<Arc<dyn ProjectPort>>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<ProjectEnvelope>> {
    port.get_document(&id)
        .await
        .map_err(|error| AppError::internal("get project document", error))?
        .map(Json)
        .ok_or_else(|| AppError::not_found("Проект не найден"))
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
    ensure_json_size(field, value, MAX_PROJECT_JSON_BYTES)
}

fn ensure_json_size(field: &str, value: &Value, maximum: usize) -> AppResult<()> {
    let size = serde_json::to_vec(value)
        .map_err(|error| AppError::internal("serialize project field", error))?
        .len();
    if size > maximum {
        return Err(AppError::payload_too_large(format!(
            "{field} больше {maximum} байт"
        )));
    }
    Ok(())
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
        documents: Vec<ProjectEnvelope>,
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

        async fn upsert_document(
            &self,
            project_id: &str,
            expected_revision: u64,
            document: &ProjectDocument,
        ) -> anyhow::Result<ProjectEnvelope> {
            let mut state = self.state.lock().unwrap();
            state.clock += 1;
            let now = state.clock;
            if let Some(index) = state
                .documents
                .iter()
                .position(|envelope| envelope.project_id == project_id)
            {
                let current = &state.documents[index];
                if current.revision != expected_revision {
                    return Err(crate::db::ProjectRevisionConflict {
                        project_id: project_id.to_owned(),
                        expected_revision,
                        actual_revision: Some(current.revision),
                    }
                    .into());
                }
                let next = current.next_revision(document.clone(), now)?;
                state.documents[index] = next.clone();
                return Ok(next);
            }
            if expected_revision != 0 {
                return Err(crate::db::ProjectRevisionConflict {
                    project_id: project_id.to_owned(),
                    expected_revision,
                    actual_revision: None,
                }
                .into());
            }
            let envelope = ProjectEnvelope::new(project_id, document.clone(), now)?;
            state.documents.push(envelope.clone());
            Ok(envelope)
        }

        async fn get_document(&self, project_id: &str) -> anyhow::Result<Option<ProjectEnvelope>> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .documents
                .iter()
                .find(|envelope| envelope.project_id == project_id)
                .cloned())
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

    #[tokio::test]
    async fn canonical_document_contract_rejects_stale_revision_without_overwrite() {
        let router = project_router(Arc::new(FakeProjects::default()));
        let document = serde_json::to_value(
            ProjectDocument::from_legacy(
                "Монтаж",
                "video-1",
                json!({"id":"video-1", "duration": 1}),
                json!({"filter":"sepia"}),
            )
            .unwrap(),
        )
        .unwrap();
        let save = |expected_revision: u64, document: Value| {
            Request::builder()
                .method("POST")
                .uri("/projects/documents")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "projectId":"project-1",
                        "expectedRevision":expected_revision,
                        "document":document
                    })
                    .to_string(),
                ))
                .unwrap()
        };

        let (status, created) = response_json(router.clone(), save(0, document.clone())).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(created["revision"], 1);

        let (status, updated) = response_json(router.clone(), save(1, document.clone())).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(updated["revision"], 2);

        let (status, conflict) = response_json(router.clone(), save(1, document)).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(conflict["code"], "project_revision_conflict");
        assert_eq!(conflict["expectedRevision"], 1);
        assert_eq!(conflict["actualRevision"], 2);

        let (status, stored) = response_json(
            router,
            Request::builder()
                .uri("/projects/documents/project-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(stored["revision"], 2);
    }
}
