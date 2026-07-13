use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

pub const FILTER_GRAPH_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Audio,
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(String);

impl NodeId {
    pub fn new(value: impl Into<String>) -> Result<Self, GraphError> {
        let value = value.into();
        if value.is_empty()
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(GraphError::InvalidIdentifier(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PadId(String);

impl PadId {
    pub fn new(value: impl Into<String>) -> Result<Self, GraphError> {
        let value = value.into();
        if value.is_empty()
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(GraphError::InvalidIdentifier(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pad {
    pub id: PadId,
    pub media: MediaKind,
    pub required: bool,
}

impl Pad {
    pub fn required(id: impl Into<String>, media: MediaKind) -> Result<Self, GraphError> {
        Ok(Self {
            id: PadId::new(id)?,
            media,
            required: true,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FilterNode {
    pub id: NodeId,
    /// An adapter-owned operation name or already escaped linear expression.
    pub operation: String,
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    #[serde(default)]
    pub inputs: Vec<Pad>,
    #[serde(default)]
    pub outputs: Vec<Pad>,
}

impl FilterNode {
    pub fn new(
        id: impl Into<String>,
        operation: impl Into<String>,
        inputs: Vec<Pad>,
        outputs: Vec<Pad>,
    ) -> Result<Self, GraphError> {
        let operation = operation.into();
        if operation.trim().is_empty() {
            return Err(GraphError::EmptyOperation);
        }
        ensure_unique_pads(&inputs)?;
        ensure_unique_pads(&outputs)?;
        Ok(Self {
            id: NodeId::new(id)?,
            operation,
            options: BTreeMap::new(),
            inputs,
            outputs,
        })
    }
}

fn ensure_unique_pads(pads: &[Pad]) -> Result<(), GraphError> {
    let mut ids = BTreeSet::new();
    for pad in pads {
        if !ids.insert(pad.id.clone()) {
            return Err(GraphError::DuplicatePad(pad.id.clone()));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Endpoint {
    pub node: NodeId,
    pub pad: PadId,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edge {
    pub from: Endpoint,
    pub to: Endpoint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FilterGraph {
    pub schema_version: u32,
    pub nodes: BTreeMap<NodeId, FilterNode>,
    pub edges: BTreeSet<Edge>,
}

impl Default for FilterGraph {
    fn default() -> Self {
        Self {
            schema_version: FILTER_GRAPH_SCHEMA_VERSION,
            nodes: BTreeMap::new(),
            edges: BTreeSet::new(),
        }
    }
}

impl FilterGraph {
    pub fn add_node(&mut self, node: FilterNode) -> Result<(), GraphError> {
        if self.nodes.contains_key(&node.id) {
            return Err(GraphError::DuplicateNode(node.id));
        }
        self.nodes.insert(node.id.clone(), node);
        Ok(())
    }

    pub fn connect(
        &mut self,
        from_node: &NodeId,
        from_pad: &PadId,
        to_node: &NodeId,
        to_pad: &PadId,
    ) -> Result<(), GraphError> {
        let edge = Edge {
            from: Endpoint {
                node: from_node.clone(),
                pad: from_pad.clone(),
            },
            to: Endpoint {
                node: to_node.clone(),
                pad: to_pad.clone(),
            },
        };
        self.validate_edge(&edge)?;
        if self.edges.iter().any(|existing| existing.to == edge.to) {
            return Err(GraphError::InputAlreadyConnected(edge.to));
        }
        self.edges.insert(edge);
        Ok(())
    }

    pub fn validate(&self) -> Result<Vec<NodeId>, GraphError> {
        if self.schema_version != FILTER_GRAPH_SCHEMA_VERSION {
            return Err(GraphError::UnsupportedSchema(self.schema_version));
        }
        for (id, node) in &self.nodes {
            if id != &node.id {
                return Err(GraphError::NodeKeyMismatch(id.clone()));
            }
            if node.operation.trim().is_empty() {
                return Err(GraphError::EmptyOperation);
            }
            ensure_unique_pads(&node.inputs)?;
            ensure_unique_pads(&node.outputs)?;
        }
        let mut connected_input_endpoints = BTreeSet::new();
        for edge in &self.edges {
            self.validate_edge(edge)?;
            if !connected_input_endpoints.insert(edge.to.clone()) {
                return Err(GraphError::InputAlreadyConnected(edge.to.clone()));
            }
        }

        let connected_inputs: BTreeSet<_> = self.edges.iter().map(|edge| &edge.to).collect();
        let connected_outputs: BTreeSet<_> = self.edges.iter().map(|edge| &edge.from).collect();
        for node in self.nodes.values() {
            for pad in node.inputs.iter().filter(|pad| pad.required) {
                let endpoint = Endpoint {
                    node: node.id.clone(),
                    pad: pad.id.clone(),
                };
                if !connected_inputs.contains(&endpoint) {
                    return Err(GraphError::RequiredPadUnconnected(endpoint));
                }
            }
            for pad in node.outputs.iter().filter(|pad| pad.required) {
                let endpoint = Endpoint {
                    node: node.id.clone(),
                    pad: pad.id.clone(),
                };
                if !connected_outputs.contains(&endpoint) {
                    return Err(GraphError::RequiredPadUnconnected(endpoint));
                }
            }
        }

        self.topological_order()
    }

    fn validate_edge(&self, edge: &Edge) -> Result<(), GraphError> {
        let source = self
            .nodes
            .get(&edge.from.node)
            .ok_or_else(|| GraphError::MissingNode(edge.from.node.clone()))?;
        let target = self
            .nodes
            .get(&edge.to.node)
            .ok_or_else(|| GraphError::MissingNode(edge.to.node.clone()))?;
        let output = source
            .outputs
            .iter()
            .find(|pad| pad.id == edge.from.pad)
            .ok_or_else(|| GraphError::MissingOutput(edge.from.clone()))?;
        let input = target
            .inputs
            .iter()
            .find(|pad| pad.id == edge.to.pad)
            .ok_or_else(|| GraphError::MissingInput(edge.to.clone()))?;
        if output.media != input.media {
            return Err(GraphError::MediaMismatch {
                from: output.media,
                to: input.media,
            });
        }
        Ok(())
    }

    fn topological_order(&self) -> Result<Vec<NodeId>, GraphError> {
        let mut indegree: BTreeMap<NodeId, usize> =
            self.nodes.keys().cloned().map(|id| (id, 0)).collect();
        let mut outgoing: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
        for edge in &self.edges {
            *indegree.entry(edge.to.node.clone()).or_default() += 1;
            outgoing
                .entry(edge.from.node.clone())
                .or_default()
                .insert(edge.to.node.clone());
        }
        let mut ready: BTreeSet<NodeId> = indegree
            .iter()
            .filter_map(|(id, degree)| (*degree == 0).then_some(id.clone()))
            .collect();
        let mut ordered = Vec::with_capacity(self.nodes.len());
        while let Some(id) = ready.pop_first() {
            ordered.push(id.clone());
            if let Some(next_nodes) = outgoing.get(&id) {
                for next in next_nodes {
                    let degree = indegree.get_mut(next).expect("known node");
                    *degree -= 1;
                    if *degree == 0 {
                        ready.insert(next.clone());
                    }
                }
            }
        }
        if ordered.len() != self.nodes.len() {
            return Err(GraphError::Cycle);
        }
        Ok(ordered)
    }

    /// Build the strict DAG subset used by the current single-stream compiler.
    pub fn linear(media: MediaKind, filters: &[String]) -> Result<Self, GraphError> {
        let mut graph = Self::default();
        let source = FilterNode::new(
            "source",
            "source",
            vec![],
            vec![Pad::required("out", media)?],
        )?;
        graph.add_node(source)?;
        let mut previous = NodeId::new("source")?;
        for (index, operation) in filters.iter().enumerate() {
            let id = NodeId::new(format!("filter_{index:04}"))?;
            graph.add_node(FilterNode::new(
                id.as_str(),
                operation,
                vec![Pad::required("in", media)?],
                vec![Pad::required("out", media)?],
            )?)?;
            graph.connect(&previous, &PadId::new("out")?, &id, &PadId::new("in")?)?;
            previous = id;
        }
        let sink = NodeId::new("sink")?;
        graph.add_node(FilterNode::new(
            sink.as_str(),
            "sink",
            vec![Pad::required("in", media)?],
            vec![],
        )?)?;
        graph.connect(&previous, &PadId::new("out")?, &sink, &PadId::new("in")?)?;
        graph.validate()?;
        Ok(graph)
    }

    pub fn ffmpeg_linear_chain(&self) -> Result<String, GraphError> {
        let ordered = self.validate()?;
        Ok(ordered
            .iter()
            .filter_map(|id| self.nodes.get(id))
            .filter(|node| node.operation != "source" && node.operation != "sink")
            .map(|node| node.operation.as_str())
            .collect::<Vec<_>>()
            .join(","))
    }

    pub fn stable_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn to_dot(&self) -> Result<String, GraphError> {
        self.validate()?;
        let mut lines = vec!["digraph filter_graph {".to_owned()];
        for node in self.nodes.values() {
            lines.push(format!(
                "  \"{}\" [label=\"{}\"];",
                node.id.as_str(),
                escape_dot(&node.operation)
            ));
        }
        for edge in &self.edges {
            lines.push(format!(
                "  \"{}\" -> \"{}\" [label=\"{}:{}\"];",
                edge.from.node.as_str(),
                edge.to.node.as_str(),
                edge.from.pad.as_str(),
                edge.to.pad.as_str()
            ));
        }
        lines.push("}".to_owned());
        Ok(lines.join("\n"))
    }
}

fn escape_dot(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    InvalidIdentifier(String),
    EmptyOperation,
    DuplicateNode(NodeId),
    NodeKeyMismatch(NodeId),
    DuplicatePad(PadId),
    MissingNode(NodeId),
    MissingInput(Endpoint),
    MissingOutput(Endpoint),
    InputAlreadyConnected(Endpoint),
    RequiredPadUnconnected(Endpoint),
    MediaMismatch { from: MediaKind, to: MediaKind },
    UnsupportedSchema(u32),
    Cycle,
}

impl fmt::Display for GraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid filter graph: {self:?}")
    }
}

impl std::error::Error for GraphError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_graph_is_stable_and_serializes_back_to_ffmpeg() {
        let filters = vec!["crop=640:360".to_owned(), "hflip".to_owned()];
        let graph = FilterGraph::linear(MediaKind::Video, &filters).unwrap();

        assert_eq!(graph.ffmpeg_linear_chain().unwrap(), "crop=640:360,hflip");
        assert_eq!(graph.stable_json().unwrap(), graph.stable_json().unwrap());
        insta_dot_shape(&graph.to_dot().unwrap());
    }

    fn insta_dot_shape(dot: &str) {
        assert!(dot.starts_with("digraph filter_graph {\n  \"filter_0000\""));
        assert!(dot.contains("\"source\" -> \"filter_0000\""));
        assert!(dot.ends_with("\n}"));
    }

    #[test]
    fn rejects_media_mismatch_before_an_adapter_runs() {
        let mut graph = FilterGraph::default();
        graph
            .add_node(
                FilterNode::new(
                    "audio_source",
                    "source",
                    vec![],
                    vec![Pad::required("out", MediaKind::Audio).unwrap()],
                )
                .unwrap(),
            )
            .unwrap();
        graph
            .add_node(
                FilterNode::new(
                    "video_sink",
                    "sink",
                    vec![Pad::required("in", MediaKind::Video).unwrap()],
                    vec![],
                )
                .unwrap(),
            )
            .unwrap();

        let error = graph
            .connect(
                &NodeId::new("audio_source").unwrap(),
                &PadId::new("out").unwrap(),
                &NodeId::new("video_sink").unwrap(),
                &PadId::new("in").unwrap(),
            )
            .unwrap_err();
        assert!(matches!(error, GraphError::MediaMismatch { .. }));
    }

    #[test]
    fn rejects_cycles_and_unconnected_required_pads() {
        let mut graph = FilterGraph::default();
        for id in ["a", "b"] {
            graph
                .add_node(
                    FilterNode::new(
                        id,
                        id,
                        vec![Pad::required("in", MediaKind::Video).unwrap()],
                        vec![Pad::required("out", MediaKind::Video).unwrap()],
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        for (from, to) in [("a", "b"), ("b", "a")] {
            graph
                .connect(
                    &NodeId::new(from).unwrap(),
                    &PadId::new("out").unwrap(),
                    &NodeId::new(to).unwrap(),
                    &PadId::new("in").unwrap(),
                )
                .unwrap();
        }
        assert_eq!(graph.validate().unwrap_err(), GraphError::Cycle);
    }

    #[test]
    fn deserialized_graph_cannot_bypass_single_input_invariant() {
        let mut graph = FilterGraph::linear(MediaKind::Video, &["hflip".to_owned()]).unwrap();
        graph
            .add_node(
                FilterNode::new(
                    "second_source",
                    "source",
                    vec![],
                    vec![Pad::required("out", MediaKind::Video).unwrap()],
                )
                .unwrap(),
            )
            .unwrap();
        graph.edges.insert(Edge {
            from: Endpoint {
                node: NodeId::new("second_source").unwrap(),
                pad: PadId::new("out").unwrap(),
            },
            to: Endpoint {
                node: NodeId::new("filter_0000").unwrap(),
                pad: PadId::new("in").unwrap(),
            },
        });

        assert!(matches!(
            graph.validate(),
            Err(GraphError::InputAlreadyConnected(_))
        ));
    }
}
