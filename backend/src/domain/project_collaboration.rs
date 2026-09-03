//! Local-first operation, sync, upload, blob, lease, and sharing contracts.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::artifact_graph::Fingerprint;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectOperation {
    pub id: String,
    pub actor: String,
    pub sequence: u64,
    pub transaction: String,
    pub kind: String,
    pub payload: Value,
}

impl ProjectOperation {
    pub fn validate(&self) -> bool {
        plain_token(&self.id, 128)
            && plain_token(&self.actor, 64)
            && plain_token(&self.transaction, 128)
            && plain_token(&self.kind, 64)
            && self.sequence > 0
            && serde_json::to_vec(&self.payload).is_ok_and(|bytes| bytes.len() <= 64 * 1024)
            && !contains_media_bytes(&self.payload)
    }
}

fn plain_token(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

fn contains_media_bytes(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, child)| {
            matches!(key.as_str(), "mediaBytes" | "blob" | "base64") || contains_media_bytes(child)
        }),
        Value::Array(items) => items.iter().any(contains_media_bytes),
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectReplica {
    operations: BTreeMap<(String, u64), ProjectOperation>,
    operation_ids: BTreeSet<String>,
}

impl ProjectReplica {
    pub fn apply(&mut self, operation: ProjectOperation) -> Result<bool, &'static str> {
        if !operation.validate() {
            return Err("invalid project operation");
        }
        if self.operation_ids.contains(&operation.id) {
            return Ok(false);
        }
        let key = (operation.actor.clone(), operation.sequence);
        if self.operations.contains_key(&key) {
            return Err("actor sequence conflict");
        }
        self.operation_ids.insert(operation.id.clone());
        self.operations.insert(key, operation);
        Ok(true)
    }

    pub fn state_vector(&self) -> BTreeMap<String, u64> {
        let mut vector: BTreeMap<String, u64> = BTreeMap::new();
        for (actor, sequence) in self.operations.keys() {
            vector
                .entry(actor.clone())
                .and_modify(|current| *current = (*current).max(*sequence))
                .or_insert(*sequence);
        }
        vector
    }

    pub fn missing_for(&self, remote: &BTreeMap<String, u64>) -> Vec<ProjectOperation> {
        self.operations
            .iter()
            .filter(|((actor, sequence), _)| *sequence > remote.get(actor).copied().unwrap_or(0))
            .map(|(_, operation)| operation.clone())
            .collect()
    }

    pub fn fingerprint(&self) -> Fingerprint {
        let bytes = serde_json::to_vec(&self.operations.values().collect::<Vec<_>>())
            .expect("validated operations serialize");
        Fingerprint::digest(&bytes)
    }

    pub fn selective_undo(&self, actor: &str) -> Option<ProjectOperation> {
        self.operations
            .values()
            .rev()
            .find(|operation| operation.actor == actor && !operation.kind.starts_with("undo:"))
            .map(|operation| ProjectOperation {
                id: format!("undo:{}", operation.id),
                actor: actor.into(),
                sequence: self.state_vector().get(actor).copied().unwrap_or(0) + 1,
                transaction: format!("undo:{}", operation.transaction),
                kind: format!("undo:{}", operation.kind),
                payload: serde_json::json!({"operationId": operation.id}),
            })
    }

    pub fn len(&self) -> usize {
        self.operations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotHorizon {
    pub snapshot_fingerprint: Fingerprint,
    pub state_vector: BTreeMap<String, u64>,
    pub acknowledgements: BTreeMap<String, BTreeMap<String, u64>>,
}

impl SnapshotHorizon {
    pub fn can_compact(&self, active_clients: &BTreeSet<String>) -> bool {
        active_clients.iter().all(|client| {
            self.acknowledgements.get(client).is_some_and(|ack| {
                self.state_vector
                    .iter()
                    .all(|(actor, sequence)| ack.get(actor).copied().unwrap_or(0) >= *sequence)
            })
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryDrill {
    pub rpo_seconds: u64,
    pub rto_seconds: u64,
    pub integrity_ok: bool,
    pub restore_verified: bool,
}

impl RecoveryDrill {
    pub fn passes(self, max_rpo: u64, max_rto: u64) -> bool {
        self.integrity_ok
            && self.restore_verified
            && self.rpo_seconds <= max_rpo
            && self.rto_seconds <= max_rto
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumableUpload {
    pub id: String,
    pub total_bytes: u64,
    pub committed_offset: u64,
    pub sha256: String,
    pub expires_at: u64,
}

impl ResumableUpload {
    pub fn append(
        &mut self,
        expected_offset: u64,
        bytes: u64,
        now: u64,
    ) -> Result<u64, &'static str> {
        if now >= self.expires_at {
            return Err("upload expired");
        }
        if expected_offset != self.committed_offset {
            return Err("upload offset conflict");
        }
        self.committed_offset = self
            .committed_offset
            .checked_add(bytes)
            .filter(|offset| *offset <= self.total_bytes)
            .ok_or("upload exceeds declared size")?;
        Ok(self.committed_offset)
    }
    pub fn complete(&self) -> bool {
        self.committed_offset == self.total_bytes
            && self.sha256.len() == 64
            && self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlobReferences {
    owners: BTreeMap<Fingerprint, BTreeSet<String>>,
}

impl BlobReferences {
    pub fn add(&mut self, blob: Fingerprint, owner: String) {
        self.owners.entry(blob).or_default().insert(owner);
    }
    pub fn remove(&mut self, blob: &Fingerprint, owner: &str) -> bool {
        let Some(owners) = self.owners.get_mut(blob) else {
            return false;
        };
        owners.remove(owner);
        if owners.is_empty() {
            self.owners.remove(blob);
            true
        } else {
            false
        }
    }
    pub fn references(&self, blob: &Fingerprint) -> usize {
        self.owners.get(blob).map_or(0, BTreeSet::len)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLease {
    pub project_id: String,
    pub holder: String,
    pub version: u64,
    pub expires_at: u64,
}

impl ProjectLease {
    pub fn acquire(
        current: Option<&Self>,
        project_id: &str,
        holder: &str,
        now: u64,
        ttl: u64,
    ) -> Result<Self, &'static str> {
        if ttl == 0 {
            return Err("lease TTL is zero");
        }
        if current.is_some_and(|lease| lease.expires_at > now && lease.holder != holder) {
            return Err("project is read-only while another editor holds the lease");
        }
        Ok(Self {
            project_id: project_id.into(),
            holder: holder.into(),
            version: current.map_or(1, |lease| lease.version + 1),
            expires_at: now.saturating_add(ttl),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharingPolicy {
    pub enabled: bool,
    pub recipients: BTreeSet<String>,
    pub media_scope: BTreeSet<String>,
    pub encryption: Option<String>,
    pub revocable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectRole {
    Owner,
    Editor,
    Commenter,
    Viewer,
}

impl ProjectRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Editor => "editor",
            Self::Commenter => "commenter",
            Self::Viewer => "viewer",
        }
    }

    pub fn can_comment(self) -> bool {
        !matches!(self, Self::Viewer)
    }

    pub fn can_resolve(self) -> bool {
        matches!(self, Self::Owner | Self::Editor)
    }

    pub fn can_manage_members(self) -> bool {
        matches!(self, Self::Owner)
    }
}

impl TryFrom<&str> for ProjectRole {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "owner" => Ok(Self::Owner),
            "editor" => Ok(Self::Editor),
            "commenter" => Ok(Self::Commenter),
            "viewer" => Ok(Self::Viewer),
            _ => Err("invalid project role"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectMember {
    pub actor: String,
    pub role: ProjectRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewAuditEvent {
    pub id: String,
    pub project_id: String,
    pub actor: String,
    pub action: String,
    pub subject_id: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewShareGrant {
    pub id: String,
    pub project_id: String,
    pub expires_at: u64,
    pub revoked_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewShareCreated {
    pub grant: ReviewShareGrant,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedReview {
    pub project_id: String,
    pub project_name: String,
    pub expires_at: u64,
    pub threads: Vec<ReviewThread>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewComment {
    pub id: String,
    pub author: String,
    pub body: String,
    pub timeline_tick: u64,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub created_at: u64,
}

impl ReviewComment {
    pub fn validate(&self) -> bool {
        plain_token(&self.id, 128)
            && plain_token(&self.author, 64)
            && self
                .parent_id
                .as_deref()
                .is_none_or(|parent| plain_token(parent, 128) && parent != self.id)
            && !self.body.trim().is_empty()
            && self.body.len() <= 8 * 1024
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewThread {
    pub id: String,
    pub project_id: String,
    pub comments: Vec<ReviewComment>,
    #[serde(default)]
    pub resolved_at: Option<u64>,
    #[serde(default)]
    pub resolved_by: Option<String>,
}

impl ReviewThread {
    pub fn new(project_id: String, comment: ReviewComment) -> Result<Self, &'static str> {
        if !plain_token(&project_id, 128) || !comment.validate() || comment.parent_id.is_some() {
            return Err("invalid review thread");
        }
        Ok(Self {
            id: comment.id.clone(),
            project_id,
            comments: vec![comment],
            resolved_at: None,
            resolved_by: None,
        })
    }

    pub fn reply(
        &mut self,
        role: ProjectRole,
        mut comment: ReviewComment,
    ) -> Result<(), &'static str> {
        if !role.can_comment() {
            return Err("role cannot comment");
        }
        if self.resolved_at.is_some() {
            return Err("review thread is resolved");
        }
        if !comment.validate()
            || self.comments.iter().any(|item| item.id == comment.id)
            || comment
                .parent_id
                .as_deref()
                .is_some_and(|parent| parent != self.id)
        {
            return Err("invalid review reply");
        }
        comment.parent_id = Some(self.id.clone());
        self.comments.push(comment);
        Ok(())
    }

    pub fn set_resolved(
        &mut self,
        role: ProjectRole,
        actor: &str,
        resolved: bool,
        now: u64,
    ) -> Result<(), &'static str> {
        if !role.can_resolve() || !plain_token(actor, 64) {
            return Err("role cannot resolve review thread");
        }
        if resolved {
            self.resolved_at = Some(now);
            self.resolved_by = Some(actor.into());
        } else {
            self.resolved_at = None;
            self.resolved_by = None;
        }
        Ok(())
    }
}

impl SharingPolicy {
    pub fn validate(&self) -> bool {
        !self.enabled
            || (!self.recipients.is_empty()
                && !self.media_scope.is_empty()
                && self
                    .encryption
                    .as_ref()
                    .is_some_and(|value| value == "age" || value == "xchacha20-poly1305")
                && self.revocable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation(actor: &str, sequence: u64, id: &str) -> ProjectOperation {
        ProjectOperation {
            id: id.into(),
            actor: actor.into(),
            sequence,
            transaction: format!("tx-{id}"),
            kind: "clip.move".into(),
            payload: serde_json::json!({"clipId":"clip-a","ticks":sequence}),
        }
    }

    #[test]
    fn replay_sync_and_selective_undo_are_deterministic_and_idempotent() {
        let mut replica = ProjectReplica::default();
        assert!(replica.apply(operation("alice", 1, "op-a")).unwrap());
        assert!(!replica.apply(operation("alice", 1, "op-a")).unwrap());
        replica.apply(operation("bob", 1, "op-b")).unwrap();
        let fingerprint = replica.fingerprint();
        let missing = replica.missing_for(&BTreeMap::from([("alice".into(), 1)]));
        assert_eq!(
            missing
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["op-b"]
        );
        let undo = replica.selective_undo("alice").unwrap();
        assert_eq!(undo.payload["operationId"], "op-a");
        assert_eq!(replica.fingerprint(), fingerprint);
    }

    #[test]
    fn compaction_requires_every_active_ack_and_operations_reject_media_bytes() {
        let horizon = SnapshotHorizon {
            snapshot_fingerprint: Fingerprint::digest(b"snapshot"),
            state_vector: BTreeMap::from([("alice".into(), 2)]),
            acknowledgements: BTreeMap::from([(
                "desktop".into(),
                BTreeMap::from([("alice".into(), 2)]),
            )]),
        };
        assert!(horizon.can_compact(&BTreeSet::from(["desktop".into()])));
        assert!(!horizon.can_compact(&BTreeSet::from(["desktop".into(), "offline-laptop".into()])));
        let mut invalid = operation("alice", 1, "op-media");
        invalid.payload = serde_json::json!({"mediaBytes":"AAAA"});
        assert!(!invalid.validate());
    }

    #[test]
    fn upload_blob_lease_recovery_and_sharing_fail_closed() {
        let mut upload = ResumableUpload {
            id: "upload-a".into(),
            total_bytes: 10,
            committed_offset: 0,
            sha256: "a".repeat(64),
            expires_at: 100,
        };
        assert_eq!(upload.append(0, 4, 1), Ok(4));
        assert_eq!(upload.append(0, 2, 1), Err("upload offset conflict"));
        upload.append(4, 6, 1).unwrap();
        assert!(upload.complete());
        let blob = Fingerprint::digest(b"media");
        let mut refs = BlobReferences::default();
        refs.add(blob.clone(), "project-a".into());
        refs.add(blob.clone(), "project-b".into());
        assert!(!refs.remove(&blob, "project-a"));
        assert_eq!(refs.references(&blob), 1);
        assert!(refs.remove(&blob, "project-b"));
        let lease = ProjectLease::acquire(None, "project-a", "alice", 1, 10).unwrap();
        assert!(ProjectLease::acquire(Some(&lease), "project-a", "bob", 2, 10).is_err());
        assert!(RecoveryDrill {
            rpo_seconds: 10,
            rto_seconds: 20,
            integrity_ok: true,
            restore_verified: true
        }
        .passes(30, 60));
        assert!(!SharingPolicy {
            enabled: true,
            recipients: BTreeSet::new(),
            media_scope: BTreeSet::new(),
            encryption: None,
            revocable: false
        }
        .validate());
    }

    #[test]
    fn review_threads_enforce_roles_threading_and_resolution() {
        let root = ReviewComment {
            id: "review-1".into(),
            author: "alice".into(),
            body: "Move this cut two frames earlier".into(),
            timeline_tick: 90_000,
            parent_id: None,
            created_at: 10,
        };
        let mut thread = ReviewThread::new("project-a".into(), root).unwrap();
        let reply = ReviewComment {
            id: "reply-1".into(),
            author: "bob".into(),
            body: "Updated".into(),
            timeline_tick: 90_000,
            parent_id: None,
            created_at: 11,
        };
        assert_eq!(
            thread.reply(ProjectRole::Viewer, reply.clone()),
            Err("role cannot comment")
        );
        thread.reply(ProjectRole::Commenter, reply).unwrap();
        assert_eq!(thread.comments[1].parent_id.as_deref(), Some("review-1"));
        assert_eq!(
            thread.set_resolved(ProjectRole::Commenter, "bob", true, 12),
            Err("role cannot resolve review thread")
        );
        thread
            .set_resolved(ProjectRole::Editor, "alice", true, 12)
            .unwrap();
        assert_eq!(thread.resolved_by.as_deref(), Some("alice"));
        assert!(thread
            .reply(
                ProjectRole::Editor,
                ReviewComment {
                    id: "reply-2".into(),
                    author: "alice".into(),
                    body: "Late reply".into(),
                    timeline_tick: 90_000,
                    parent_id: None,
                    created_at: 13,
                }
            )
            .is_err());
        thread
            .set_resolved(ProjectRole::Owner, "alice", false, 14)
            .unwrap();
        assert!(thread.resolved_at.is_none());
    }

    #[test]
    fn review_comments_reject_empty_oversized_and_recursive_payloads() {
        let mut comment = ReviewComment {
            id: "review-1".into(),
            author: "alice".into(),
            body: " ".into(),
            timeline_tick: 0,
            parent_id: None,
            created_at: 1,
        };
        assert!(!comment.validate());
        comment.body = "x".repeat(8 * 1024 + 1);
        assert!(!comment.validate());
        comment.body = "ok".into();
        comment.parent_id = Some(comment.id.clone());
        assert!(!comment.validate());
    }
}
