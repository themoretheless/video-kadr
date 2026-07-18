//! Dependency graph for source and derived artifacts.
//!
//! The graph contains identity and invalidation semantics only. It deliberately
//! knows nothing about files, databases, HTTP, or background workers.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint(String);

impl Fingerprint {
    pub fn parse(value: impl Into<String>) -> Result<Self, ArtifactGraphError> {
        let value = value.into();
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ArtifactGraphError::InvalidFingerprint(value));
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn digest(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self(format!("{:x}", hasher.finalize()))
    }

    /// Hash a sequence without concatenation ambiguity (`[ab, c] != [a, bc]`).
    pub fn combine<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> Self {
        let mut hasher = Sha256::new();
        for part in parts {
            hasher.update((part.len() as u64).to_be_bytes());
            hasher.update(part);
        }
        Self(format!("{:x}", hasher.finalize()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for Fingerprint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Fingerprint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::parse(String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactId(String);

impl ArtifactId {
    pub fn parse(value: impl Into<String>) -> Result<Self, ArtifactGraphError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
        if !valid {
            return Err(ArtifactGraphError::InvalidId(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ArtifactId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for ArtifactId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ArtifactId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::parse(String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Source,
    EditPlan,
    Proxy,
    Analysis,
    FrameChunk,
    EncodedMedia,
    Package,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactNode {
    pub id: ArtifactId,
    pub kind: ArtifactKind,
    pub fingerprint: Fingerprint,
    pub dependencies: BTreeSet<ArtifactId>,
}

impl ArtifactNode {
    pub fn new(
        id: ArtifactId,
        kind: ArtifactKind,
        fingerprint: Fingerprint,
        dependencies: impl IntoIterator<Item = ArtifactId>,
    ) -> Self {
        Self {
            id,
            kind,
            fingerprint,
            dependencies: dependencies.into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ArtifactGraph {
    nodes: BTreeMap<ArtifactId, ArtifactNode>,
}

impl<'de> Deserialize<'de> for ArtifactGraph {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let graph = Self {
            nodes: BTreeMap::deserialize(deserializer)?,
        };
        graph.validate().map_err(D::Error::custom)?;
        Ok(graph)
    }
}

impl ArtifactGraph {
    pub fn insert(&mut self, node: ArtifactNode) -> Result<(), ArtifactGraphError> {
        if self.nodes.contains_key(&node.id) {
            return Err(ArtifactGraphError::Duplicate(node.id));
        }
        if node.dependencies.contains(&node.id) {
            return Err(ArtifactGraphError::Cycle(node.id));
        }
        if let Some(missing) = node
            .dependencies
            .iter()
            .find(|dependency| !self.nodes.contains_key(*dependency))
        {
            return Err(ArtifactGraphError::MissingDependency(missing.clone()));
        }
        self.nodes.insert(node.id.clone(), node);
        Ok(())
    }

    pub fn get(&self, id: &ArtifactId) -> Option<&ArtifactNode> {
        self.nodes.get(id)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn validate(&self) -> Result<(), ArtifactGraphError> {
        for (key, node) in &self.nodes {
            if key != &node.id {
                return Err(ArtifactGraphError::KeyMismatch(
                    key.clone(),
                    node.id.clone(),
                ));
            }
            if let Some(missing) = node
                .dependencies
                .iter()
                .find(|dependency| !self.nodes.contains_key(*dependency))
            {
                return Err(ArtifactGraphError::MissingDependency(missing.clone()));
            }
        }

        let mut indegree: BTreeMap<_, _> = self
            .nodes
            .iter()
            .map(|(id, node)| (id.clone(), node.dependencies.len()))
            .collect();
        let mut queue: VecDeque<_> = indegree
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| id.clone())
            .collect();
        let mut visited = 0_usize;
        while let Some(ready) = queue.pop_front() {
            visited += 1;
            for node in self
                .nodes
                .values()
                .filter(|node| node.dependencies.contains(&ready))
            {
                let count = indegree
                    .get_mut(&node.id)
                    .expect("validated node ids are present in indegree map");
                *count -= 1;
                if *count == 0 {
                    queue.push_back(node.id.clone());
                }
            }
        }
        if visited != self.nodes.len() {
            let cyclic = indegree
                .into_iter()
                .find(|(_, count)| *count > 0)
                .map(|(id, _)| id)
                .expect("unvisited graph contains a node");
            return Err(ArtifactGraphError::Cycle(cyclic));
        }
        Ok(())
    }

    /// Change an artifact identity and remove only transitive dependants.
    pub fn update_fingerprint(
        &mut self,
        id: &ArtifactId,
        fingerprint: Fingerprint,
    ) -> Result<BTreeSet<ArtifactId>, ArtifactGraphError> {
        let node = self
            .nodes
            .get_mut(id)
            .ok_or_else(|| ArtifactGraphError::MissingArtifact(id.clone()))?;
        if node.fingerprint == fingerprint {
            return Ok(BTreeSet::new());
        }
        node.fingerprint = fingerprint;
        Ok(self.invalidate_downstream(std::slice::from_ref(id)))
    }

    pub fn invalidate_downstream(&mut self, roots: &[ArtifactId]) -> BTreeSet<ArtifactId> {
        let mut queue: VecDeque<_> = roots.iter().cloned().collect();
        let mut invalidated = BTreeSet::new();
        while let Some(changed) = queue.pop_front() {
            let direct: Vec<_> = self
                .nodes
                .values()
                .filter(|node| {
                    !invalidated.contains(&node.id) && node.dependencies.contains(&changed)
                })
                .map(|node| node.id.clone())
                .collect();
            for id in direct {
                if invalidated.insert(id.clone()) {
                    queue.push_back(id);
                }
            }
        }
        for id in &invalidated {
            self.nodes.remove(id);
        }
        invalidated
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactGraphError {
    InvalidFingerprint(String),
    InvalidId(String),
    Duplicate(ArtifactId),
    MissingArtifact(ArtifactId),
    MissingDependency(ArtifactId),
    KeyMismatch(ArtifactId, ArtifactId),
    Cycle(ArtifactId),
}

impl fmt::Display for ArtifactGraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid artifact graph: {self:?}")
    }
}

impl std::error::Error for ArtifactGraphError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> ArtifactId {
        ArtifactId::parse(value).unwrap()
    }

    fn fp(value: &str) -> Fingerprint {
        Fingerprint::digest(value.as_bytes())
    }

    fn node(name: &str, kind: ArtifactKind, dependencies: &[&str]) -> ArtifactNode {
        ArtifactNode::new(
            id(name),
            kind,
            fp(name),
            dependencies.iter().map(|value| id(value)),
        )
    }

    #[test]
    fn fingerprint_composition_is_unambiguous_and_strict() {
        assert_ne!(
            Fingerprint::combine([b"ab".as_slice(), b"c".as_slice()]),
            Fingerprint::combine([b"a".as_slice(), b"bc".as_slice()])
        );
        assert!(Fingerprint::parse("abc").is_err());
        let upper = "A".repeat(64);
        assert_eq!(Fingerprint::parse(upper).unwrap().as_str(), "a".repeat(64));
    }

    #[test]
    fn invalidation_removes_only_downstream_artifacts() {
        let mut graph = ArtifactGraph::default();
        graph
            .insert(node("source", ArtifactKind::Source, &[]))
            .unwrap();
        graph
            .insert(node("edit", ArtifactKind::EditPlan, &[]))
            .unwrap();
        graph
            .insert(node("proxy", ArtifactKind::Proxy, &["source"]))
            .unwrap();
        graph
            .insert(node(
                "render",
                ArtifactKind::EncodedMedia,
                &["source", "edit"],
            ))
            .unwrap();
        graph
            .insert(node("package", ArtifactKind::Package, &["render"]))
            .unwrap();

        let invalidated = graph
            .update_fingerprint(&id("edit"), fp("changed-edit"))
            .unwrap();
        assert_eq!(invalidated, BTreeSet::from([id("render"), id("package")]));
        assert!(graph.get(&id("source")).is_some());
        assert!(graph.get(&id("proxy")).is_some());
        assert!(graph.get(&id("edit")).is_some());
    }

    #[test]
    fn unchanged_fingerprint_preserves_cached_artifacts() {
        let mut graph = ArtifactGraph::default();
        graph
            .insert(node("source", ArtifactKind::Source, &[]))
            .unwrap();
        graph
            .insert(node("proxy", ArtifactKind::Proxy, &["source"]))
            .unwrap();

        assert!(graph
            .update_fingerprint(&id("source"), fp("source"))
            .unwrap()
            .is_empty());
        assert_eq!(graph.len(), 2);
    }

    #[test]
    fn dependency_must_exist_before_derived_node() {
        let mut graph = ArtifactGraph::default();
        assert_eq!(
            graph
                .insert(node("proxy", ArtifactKind::Proxy, &["missing"]))
                .unwrap_err(),
            ArtifactGraphError::MissingDependency(id("missing"))
        );
    }

    #[test]
    fn deserialization_cannot_bypass_graph_invariants() {
        let source = fp("source").to_string();
        let proxy = fp("proxy").to_string();
        let valid = serde_json::json!({
            "source": {
                "id": "source",
                "kind": "source",
                "fingerprint": source,
                "dependencies": []
            },
            "proxy": {
                "id": "proxy",
                "kind": "proxy",
                "fingerprint": proxy,
                "dependencies": ["source"]
            }
        });
        assert!(serde_json::from_value::<ArtifactGraph>(valid).is_ok());

        let cycle = serde_json::json!({
            "a": {
                "id": "a",
                "kind": "source",
                "fingerprint": fp("a").to_string(),
                "dependencies": ["b"]
            },
            "b": {
                "id": "b",
                "kind": "proxy",
                "fingerprint": fp("b").to_string(),
                "dependencies": ["a"]
            }
        });
        assert!(serde_json::from_value::<ArtifactGraph>(cycle).is_err());

        let mismatched_key = serde_json::json!({
            "source": {
                "id": "other",
                "kind": "source",
                "fingerprint": fp("source").to_string(),
                "dependencies": []
            }
        });
        assert!(serde_json::from_value::<ArtifactGraph>(mismatched_key).is_err());
    }
}
