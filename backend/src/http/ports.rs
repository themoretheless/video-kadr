use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::capabilities::Capabilities;
use crate::db::{
    AuthSession, AuthUser, BrandKitPayload, CompositionProject, Db, Project, Space, SpaceBrandKit,
    SpaceInviteCreated, SpaceMember, SpaceRole, SpaceTemplate,
};
use crate::domain::project_collaboration::{
    ProjectMember, ProjectRole, ReviewAuditEvent, ReviewShareCreated, ReviewThread, SharedReview,
};
use crate::library::Library;
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

#[axum::async_trait]
pub trait AuthPort: Send + Sync {
    async fn register(&self, username: &str, password: &str) -> Result<Option<AuthSession>>;
    async fn login(&self, username: &str, password: &str) -> Result<Option<AuthSession>>;
    async fn resolve(&self, token: &str) -> Result<Option<AuthUser>>;
    async fn logout(&self, token: &str) -> Result<bool>;
}

#[derive(Clone)]
pub struct SqliteAuthPort {
    db: Db,
}

impl SqliteAuthPort {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[axum::async_trait]
impl AuthPort for SqliteAuthPort {
    async fn register(&self, username: &str, password: &str) -> Result<Option<AuthSession>> {
        self.db.register_auth_user(username, password).await
    }

    async fn login(&self, username: &str, password: &str) -> Result<Option<AuthSession>> {
        self.db.login_auth_user(username, password).await
    }

    async fn resolve(&self, token: &str) -> Result<Option<AuthUser>> {
        self.db.resolve_auth_session(token).await
    }

    async fn logout(&self, token: &str) -> Result<bool> {
        self.db.revoke_auth_session(token).await
    }
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

#[derive(Debug, Clone, PartialEq)]
pub struct CompositionProjectDraft {
    pub name: String,
    pub space_id: Option<String>,
    pub base_revision: Option<u64>,
    pub document: Value,
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionProjectChange {
    pub project_id: String,
    pub kind: &'static str,
    pub revision: Option<u64>,
    #[serde(skip)]
    pub audience: Vec<String>,
}

#[axum::async_trait]
pub trait CompositionProjectPort: Send + Sync {
    async fn authenticate(&self, token: &str) -> Result<Option<AuthUser>>;
    async fn create(
        &self,
        draft: CompositionProjectDraft,
        owner: &str,
    ) -> Result<CompositionProject>;
    async fn update(
        &self,
        id: &str,
        actor: &str,
        draft: CompositionProjectDraft,
    ) -> Result<Option<CompositionProject>>;
    async fn list(&self, actor: &str) -> Result<Vec<CompositionProject>>;
    async fn get(&self, id: &str, actor: &str) -> Result<Option<CompositionProject>>;
    async fn delete(&self, id: &str, actor: &str) -> Result<bool>;
    fn subscribe(&self) -> broadcast::Receiver<CompositionProjectChange>;
}

#[derive(Clone)]
pub struct SqliteCompositionProjectPort {
    db: Db,
    library: Library,
    changes: broadcast::Sender<CompositionProjectChange>,
}

impl SqliteCompositionProjectPort {
    pub fn new(db: Db, library: Library) -> Self {
        let (changes, _) = broadcast::channel(256);
        Self {
            db,
            library,
            changes,
        }
    }

    async fn isolate_sources(&self, project: &CompositionProject) -> Result<()> {
        let Some(space_id) = project.space_id.as_deref() else {
            return Ok(());
        };
        for source_id in &project.source_ids {
            self.library
                .relocate_source_to_space(source_id, space_id)
                .await?;
        }
        Ok(())
    }
}

#[axum::async_trait]
impl CompositionProjectPort for SqliteCompositionProjectPort {
    async fn authenticate(&self, token: &str) -> Result<Option<AuthUser>> {
        self.db.resolve_auth_session(token).await
    }

    async fn create(
        &self,
        draft: CompositionProjectDraft,
        owner: &str,
    ) -> Result<CompositionProject> {
        let project = if let Some(space_id) = draft.space_id.as_deref() {
            self.db
                .create_space_composition_project(
                    &draft.name,
                    &draft.document,
                    &draft.source_ids,
                    owner,
                    space_id,
                )
                .await?
        } else {
            self.db
                .create_owned_composition_project(
                    &draft.name,
                    &draft.document,
                    &draft.source_ids,
                    owner,
                )
                .await?
        };
        self.isolate_sources(&project).await?;
        let _ = self.changes.send(CompositionProjectChange {
            project_id: project.id.clone(),
            kind: "upsert",
            revision: Some(project.revision),
            audience: self
                .db
                .composition_project_member_actors(&project.id)
                .await?,
        });
        Ok(project)
    }

    async fn update(
        &self,
        id: &str,
        actor: &str,
        draft: CompositionProjectDraft,
    ) -> Result<Option<CompositionProject>> {
        let project = self
            .db
            .update_composition_project_for(
                id,
                actor,
                &draft.name,
                &draft.document,
                &draft.source_ids,
                draft
                    .base_revision
                    .expect("HTTP update validates base revision"),
            )
            .await?;
        if let Some(project) = project.as_ref() {
            self.isolate_sources(project).await?;
            let _ = self.changes.send(CompositionProjectChange {
                project_id: project.id.clone(),
                kind: "upsert",
                revision: Some(project.revision),
                audience: self
                    .db
                    .composition_project_member_actors(&project.id)
                    .await?,
            });
        }
        Ok(project)
    }

    async fn list(&self, actor: &str) -> Result<Vec<CompositionProject>> {
        self.db.list_composition_projects_for(actor).await
    }

    async fn get(&self, id: &str, actor: &str) -> Result<Option<CompositionProject>> {
        self.db.get_composition_project_for(id, actor).await
    }

    async fn delete(&self, id: &str, actor: &str) -> Result<bool> {
        let audience = self.db.composition_project_member_actors(id).await?;
        let deleted = self.db.delete_composition_project_for(id, actor).await?;
        if deleted {
            let _ = self.changes.send(CompositionProjectChange {
                project_id: id.to_owned(),
                kind: "delete",
                revision: None,
                audience,
            });
        }
        Ok(deleted)
    }

    fn subscribe(&self) -> broadcast::Receiver<CompositionProjectChange> {
        self.changes.subscribe()
    }
}

#[axum::async_trait]
pub trait ProjectReviewPort: Send + Sync {
    async fn authenticate(&self, token: &str) -> Result<Option<AuthUser>>;
    async fn create(
        &self,
        project_id: &str,
        author: &str,
        body: &str,
        timeline_tick: u64,
    ) -> Result<Option<ReviewThread>>;
    async fn list(&self, project_id: &str, actor: &str) -> Result<Vec<ReviewThread>>;
    async fn reply(
        &self,
        thread_id: &str,
        author: &str,
        body: &str,
    ) -> Result<Option<ReviewThread>>;
    async fn resolve(
        &self,
        thread_id: &str,
        actor: &str,
        resolved: bool,
    ) -> Result<Option<ReviewThread>>;
    async fn list_members(&self, project_id: &str, actor: &str) -> Result<Vec<ProjectMember>>;
    async fn list_audit(&self, project_id: &str, actor: &str) -> Result<Vec<ReviewAuditEvent>>;
    async fn create_share(
        &self,
        project_id: &str,
        requester: &str,
        ttl_seconds: u64,
    ) -> Result<Option<ReviewShareCreated>>;
    async fn open_share(&self, token: &str) -> Result<Option<SharedReview>>;
    async fn revoke_share(
        &self,
        project_id: &str,
        share_id: &str,
        requester: &str,
    ) -> Result<Option<bool>>;
    async fn set_member(
        &self,
        project_id: &str,
        requester: &str,
        actor: &str,
        role: ProjectRole,
    ) -> Result<Option<ProjectMember>>;
    async fn transfer_ownership(
        &self,
        project_id: &str,
        requester: &str,
        target: &str,
    ) -> Result<Option<ProjectMember>>;
}

#[derive(Clone)]
pub struct SqliteProjectReviewPort {
    db: Db,
}

impl SqliteProjectReviewPort {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[axum::async_trait]
impl ProjectReviewPort for SqliteProjectReviewPort {
    async fn authenticate(&self, token: &str) -> Result<Option<AuthUser>> {
        self.db.resolve_auth_session(token).await
    }

    async fn create(
        &self,
        project_id: &str,
        author: &str,
        body: &str,
        timeline_tick: u64,
    ) -> Result<Option<ReviewThread>> {
        self.db
            .create_review_thread(project_id, author, body, timeline_tick)
            .await
    }

    async fn list(&self, project_id: &str, actor: &str) -> Result<Vec<ReviewThread>> {
        self.db.list_review_threads_for(project_id, actor).await
    }

    async fn reply(
        &self,
        thread_id: &str,
        author: &str,
        body: &str,
    ) -> Result<Option<ReviewThread>> {
        self.db
            .reply_to_review_thread(thread_id, author, body)
            .await
    }

    async fn resolve(
        &self,
        thread_id: &str,
        actor: &str,
        resolved: bool,
    ) -> Result<Option<ReviewThread>> {
        self.db
            .resolve_review_thread(thread_id, actor, resolved)
            .await
    }

    async fn list_members(&self, project_id: &str, actor: &str) -> Result<Vec<ProjectMember>> {
        self.db.list_review_members_for(project_id, actor).await
    }

    async fn list_audit(&self, project_id: &str, actor: &str) -> Result<Vec<ReviewAuditEvent>> {
        self.db.list_review_audit_for(project_id, actor).await
    }

    async fn create_share(
        &self,
        project_id: &str,
        requester: &str,
        ttl_seconds: u64,
    ) -> Result<Option<ReviewShareCreated>> {
        self.db
            .create_review_share(project_id, requester, ttl_seconds)
            .await
    }

    async fn open_share(&self, token: &str) -> Result<Option<SharedReview>> {
        self.db.open_shared_review(token).await
    }

    async fn revoke_share(
        &self,
        project_id: &str,
        share_id: &str,
        requester: &str,
    ) -> Result<Option<bool>> {
        self.db
            .revoke_review_share(project_id, share_id, requester)
            .await
    }

    async fn set_member(
        &self,
        project_id: &str,
        requester: &str,
        actor: &str,
        role: ProjectRole,
    ) -> Result<Option<ProjectMember>> {
        self.db
            .set_review_member(project_id, requester, actor, role)
            .await
    }

    async fn transfer_ownership(
        &self,
        project_id: &str,
        requester: &str,
        target: &str,
    ) -> Result<Option<ProjectMember>> {
        self.db
            .transfer_project_ownership(project_id, requester, target)
            .await
    }
}

#[axum::async_trait]
pub trait SpacePort: Send + Sync {
    async fn authenticate(&self, token: &str) -> Result<Option<AuthUser>>;
    async fn create(&self, owner: &str, name: &str) -> Result<Space>;
    async fn list(&self, actor: &str) -> Result<Vec<Space>>;
    async fn rename(
        &self,
        space_id: &str,
        requester: &str,
        name: &str,
        base_updated_at: u64,
    ) -> Result<Option<Space>>;
    async fn delete_empty(&self, space_id: &str, requester: &str) -> Result<Option<bool>>;
    async fn list_members(&self, space_id: &str, actor: &str) -> Result<Vec<SpaceMember>>;
    async fn create_invite(
        &self,
        space_id: &str,
        requester: &str,
        role: SpaceRole,
        ttl_seconds: u64,
    ) -> Result<Option<SpaceInviteCreated>>;
    async fn accept_invite(&self, token: &str, actor: &str) -> Result<Option<Space>>;
    async fn set_member(
        &self,
        space_id: &str,
        requester: &str,
        actor: &str,
        role: SpaceRole,
    ) -> Result<Option<SpaceMember>>;
    async fn remove_member(
        &self,
        space_id: &str,
        requester: &str,
        actor: &str,
    ) -> Result<Option<bool>>;
    async fn transfer_ownership(
        &self,
        space_id: &str,
        requester: &str,
        target: &str,
    ) -> Result<Option<SpaceMember>>;
    async fn list_templates(&self, space_id: &str, actor: &str) -> Result<Vec<SpaceTemplate>>;
    async fn create_template(
        &self,
        space_id: &str,
        actor: &str,
        template: &Value,
    ) -> Result<Option<SpaceTemplate>>;
    async fn update_template(
        &self,
        space_id: &str,
        template_id: &str,
        actor: &str,
        base_revision: u64,
        template: &Value,
    ) -> Result<Option<SpaceTemplate>>;
    async fn delete_template(&self, space_id: &str, template_id: &str, actor: &str)
        -> Result<bool>;
    async fn get_brand_kit(&self, space_id: &str, actor: &str) -> Result<SpaceBrandKit>;
    async fn update_brand_kit(
        &self,
        space_id: &str,
        actor: &str,
        base_revision: u64,
        kit: BrandKitPayload,
    ) -> Result<Option<SpaceBrandKit>>;
}

#[derive(Clone)]
pub struct SqliteSpacePort {
    db: Db,
    library: Library,
}

impl SqliteSpacePort {
    pub fn new(db: Db, library: Library) -> Self {
        Self { db, library }
    }
}

#[axum::async_trait]
impl SpacePort for SqliteSpacePort {
    async fn authenticate(&self, token: &str) -> Result<Option<AuthUser>> {
        self.db.resolve_auth_session(token).await
    }
    async fn create(&self, owner: &str, name: &str) -> Result<Space> {
        self.db.create_space(owner, name).await
    }
    async fn list(&self, actor: &str) -> Result<Vec<Space>> {
        self.db.list_spaces_for(actor).await
    }
    async fn rename(
        &self,
        space_id: &str,
        requester: &str,
        name: &str,
        base_updated_at: u64,
    ) -> Result<Option<Space>> {
        self.db
            .rename_space(space_id, requester, name, base_updated_at)
            .await
    }
    async fn delete_empty(&self, space_id: &str, requester: &str) -> Result<Option<bool>> {
        self.db.delete_empty_space(space_id, requester).await
    }
    async fn list_members(&self, space_id: &str, actor: &str) -> Result<Vec<SpaceMember>> {
        self.db.list_space_members_for(space_id, actor).await
    }
    async fn create_invite(
        &self,
        space_id: &str,
        requester: &str,
        role: SpaceRole,
        ttl_seconds: u64,
    ) -> Result<Option<SpaceInviteCreated>> {
        self.db
            .create_space_invite(space_id, requester, role, ttl_seconds)
            .await
    }
    async fn accept_invite(&self, token: &str, actor: &str) -> Result<Option<Space>> {
        self.db.accept_space_invite(token, actor).await
    }
    async fn set_member(
        &self,
        space_id: &str,
        requester: &str,
        actor: &str,
        role: SpaceRole,
    ) -> Result<Option<SpaceMember>> {
        self.db
            .set_space_member(space_id, requester, actor, role)
            .await
    }
    async fn remove_member(
        &self,
        space_id: &str,
        requester: &str,
        actor: &str,
    ) -> Result<Option<bool>> {
        self.db
            .remove_space_member(space_id, requester, actor)
            .await
    }
    async fn transfer_ownership(
        &self,
        space_id: &str,
        requester: &str,
        target: &str,
    ) -> Result<Option<SpaceMember>> {
        self.db
            .transfer_space_ownership(space_id, requester, target)
            .await
    }
    async fn list_templates(&self, space_id: &str, actor: &str) -> Result<Vec<SpaceTemplate>> {
        self.db.list_space_templates_for(space_id, actor).await
    }
    async fn create_template(
        &self,
        space_id: &str,
        actor: &str,
        template: &Value,
    ) -> Result<Option<SpaceTemplate>> {
        self.db
            .create_space_template(space_id, actor, template)
            .await
    }
    async fn update_template(
        &self,
        space_id: &str,
        template_id: &str,
        actor: &str,
        base_revision: u64,
        template: &Value,
    ) -> Result<Option<SpaceTemplate>> {
        self.db
            .update_space_template(space_id, template_id, actor, base_revision, template)
            .await
    }
    async fn delete_template(
        &self,
        space_id: &str,
        template_id: &str,
        actor: &str,
    ) -> Result<bool> {
        self.db
            .delete_space_template(space_id, template_id, actor)
            .await
    }
    async fn get_brand_kit(&self, space_id: &str, actor: &str) -> Result<SpaceBrandKit> {
        self.db.get_space_brand_kit_for(space_id, actor).await
    }
    async fn update_brand_kit(
        &self,
        space_id: &str,
        actor: &str,
        base_revision: u64,
        kit: BrandKitPayload,
    ) -> Result<Option<SpaceBrandKit>> {
        for source_id in &kit.logo_source_ids {
            let entry = self
                .library
                .get(source_id)
                .await
                .ok_or_else(|| anyhow::anyhow!("brand logo source is missing"))?;
            anyhow::ensure!(
                entry.kind == "source" && entry.media_type.as_deref() == Some("image"),
                "brand logo must be an image source"
            );
        }
        self.db
            .update_space_brand_kit(space_id, actor, base_revision, kit)
            .await
    }
}
