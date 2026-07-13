use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

use crate::capabilities::Capabilities;
use crate::db::{Db, Project};
use crate::state::ToolInfo;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
    pub status: &'static str,
    pub ffmpeg: bool,
    pub ytdlp: bool,
    pub ffmpeg_version: Option<String>,
    pub ytdlp_version: Option<String>,
}

pub trait SystemPort: Send + Sync {
    fn health(&self) -> HealthStatus;
    fn capabilities(&self) -> Capabilities;
}

#[derive(Clone)]
pub struct RuntimeSystemPort {
    tools: Arc<ToolInfo>,
}

impl RuntimeSystemPort {
    pub fn new(tools: Arc<ToolInfo>) -> Self {
        Self { tools }
    }
}

impl SystemPort for RuntimeSystemPort {
    fn health(&self) -> HealthStatus {
        HealthStatus {
            status: if self.tools.ffmpeg && self.tools.ytdlp {
                "ok"
            } else {
                "degraded"
            },
            ffmpeg: self.tools.ffmpeg,
            ytdlp: self.tools.ytdlp,
            ffmpeg_version: self.tools.ffmpeg_version.clone(),
            ytdlp_version: self.tools.ytdlp_version.clone(),
        }
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::from_tools(&self.tools)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectDraft {
    pub video_id: String,
    pub name: String,
    pub video: Value,
    pub edit: Value,
}

#[axum::async_trait]
pub trait ProjectPort: Send + Sync {
    async fn upsert(&self, draft: ProjectDraft) -> Result<Project>;
    async fn list(&self) -> Result<Vec<Project>>;
    async fn get(&self, id: &str) -> Result<Option<Project>>;
    async fn get_by_video(&self, video_id: &str) -> Result<Option<Project>>;
    async fn delete(&self, id: &str) -> Result<bool>;
}

#[derive(Clone)]
pub struct SqliteProjectPort {
    db: Db,
}

impl SqliteProjectPort {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[axum::async_trait]
impl ProjectPort for SqliteProjectPort {
    async fn upsert(&self, draft: ProjectDraft) -> Result<Project> {
        self.db
            .upsert_project(&draft.video_id, &draft.name, &draft.video, &draft.edit)
            .await
    }

    async fn list(&self) -> Result<Vec<Project>> {
        self.db.list_projects().await
    }

    async fn get(&self, id: &str) -> Result<Option<Project>> {
        self.db.get_project(id).await
    }

    async fn get_by_video(&self, video_id: &str) -> Result<Option<Project>> {
        self.db.get_project_by_video(video_id).await
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        self.db.delete_project(id).await
    }
}
