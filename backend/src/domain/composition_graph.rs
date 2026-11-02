//! Native dependency admission for nested composition references.
//! Ordering is children-first, suitable for later flatten/render planning.
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionGraph {
    pub root: String,
    pub references: BTreeMap<String, Vec<String>>,
}

impl CompositionGraph {
    pub fn dependency_order(&self) -> Result<Vec<String>> {
        ensure!(
            !self.references.is_empty() && self.references.len() <= 1024,
            "composition graph requires 1..1024 nodes"
        );
        ensure!(
            self.references.contains_key(&self.root),
            "root composition is missing"
        );
        let mut edges = 0usize;
        for (id, children) in &self.references {
            ensure!(
                !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control),
                "invalid composition id"
            );
            edges = edges
                .checked_add(children.len())
                .ok_or_else(|| anyhow::anyhow!("reference count overflow"))?;
            ensure!(edges <= 4096, "composition graph exceeds 4096 references");
            for child in children {
                ensure!(child != &self.root, "root composition cannot be referenced");
                ensure!(
                    self.references.contains_key(child),
                    "referenced composition {child} is missing"
                );
            }
        }
        let mut visiting = BTreeSet::new();
        let mut complete = BTreeSet::new();
        let mut order = Vec::new();
        // Validate disconnected subgraphs too, including saved unused compositions.
        for id in self.references.keys() {
            self.visit(id, &mut visiting, &mut complete, &mut order)?;
        }
        Ok(order)
    }

    fn visit(
        &self,
        id: &str,
        visiting: &mut BTreeSet<String>,
        complete: &mut BTreeSet<String>,
        order: &mut Vec<String>,
    ) -> Result<()> {
        if complete.contains(id) {
            return Ok(());
        }
        if visiting.contains(id) {
            bail!("composition reference cycle at {id}");
        }
        ensure!(visiting.len() < 64, "composition nesting exceeds 64 levels");
        visiting.insert(id.to_owned());
        for child in &self.references[id] {
            self.visit(child, visiting, complete, order)?;
        }
        visiting.remove(id);
        complete.insert(id.to_owned());
        order.push(id.to_owned());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn graph(entries: &[(&str, &[&str])]) -> CompositionGraph {
        CompositionGraph {
            root: "root".into(),
            references: entries
                .iter()
                .map(|(id, children)| {
                    (
                        (*id).into(),
                        children.iter().map(|child| (*child).into()).collect(),
                    )
                })
                .collect(),
        }
    }
    #[test]
    fn shared_children_are_ordered_once_before_every_parent() {
        let graph = graph(&[
            ("root", &["a", "b"]),
            ("a", &["leaf"]),
            ("b", &["leaf", "leaf"]),
            ("leaf", &[]),
        ]);
        assert_eq!(
            graph.dependency_order().unwrap(),
            ["leaf", "a", "b", "root"]
        );
        let saved = serde_json::to_string(&graph).unwrap();
        assert_eq!(
            serde_json::from_str::<CompositionGraph>(&saved).unwrap(),
            graph
        );
    }
    #[test]
    fn refuses_missing_root_dangling_root_and_disconnected_cycles() {
        for graph in [
            graph(&[("a", &[])]),
            graph(&[("root", &["missing"])]),
            graph(&[("root", &["root"])]),
            graph(&[("root", &[]), ("a", &["b"]), ("b", &["a"])]),
        ] {
            assert!(graph.dependency_order().is_err());
        }
    }
    #[test]
    fn bounded_depth_prevents_recursive_stack_growth() {
        let mut graph = graph(&[("root", &["n00"])]);
        for index in 0..65 {
            graph.references.insert(
                format!("n{index:02}"),
                if index < 64 {
                    vec![format!("n{:02}", index + 1)]
                } else {
                    vec![]
                },
            );
        }
        assert!(graph
            .dependency_order()
            .unwrap_err()
            .to_string()
            .contains("64 levels"));
    }
}

/// Unit-rate source timing for a CompositionRef instance. Overhanging source
/// ranges remain authored data; child duration is applied when resolving frames.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionReferenceTiming {
    pub composition: String,
    pub timeline_start_us: u64,
    pub source_in_us: u64,
    pub source_out_us: u64,
}

impl CompositionReferenceTiming {
    pub fn timeline_end_us(&self) -> Result<u64> {
        ensure!(
            !self.composition.is_empty()
                && self.composition.len() <= 128
                && !self.composition.chars().any(char::is_control),
            "invalid referenced composition id"
        );
        ensure!(
            self.source_out_us > self.source_in_us,
            "composition source range must be positive"
        );
        let end = self
            .timeline_start_us
            .checked_add(self.source_out_us - self.source_in_us)
            .ok_or_else(|| anyhow::anyhow!("composition reference clock overflow"))?;
        ensure!(
            end <= 9_007_199_254_740_991 && self.source_out_us <= 9_007_199_254_740_991,
            "composition reference clock exceeds exact wire range"
        );
        Ok(end)
    }

    pub fn source_time_us(&self, parent_time_us: u64) -> Result<Option<u64>> {
        let end = self.timeline_end_us()?;
        if parent_time_us < self.timeline_start_us || parent_time_us >= end {
            return Ok(None);
        }
        Ok(Some(
            self.source_in_us + parent_time_us - self.timeline_start_us,
        ))
    }

    pub fn split(&self, parent_time_us: u64) -> Result<(Self, Self)> {
        let source = self
            .source_time_us(parent_time_us)?
            .ok_or_else(|| anyhow::anyhow!("split is outside composition reference"))?;
        ensure!(
            parent_time_us > self.timeline_start_us,
            "split must be inside composition reference"
        );
        let mut left = self.clone();
        left.source_out_us = source;
        let mut right = self.clone();
        right.timeline_start_us = parent_time_us;
        right.source_in_us = source;
        left.timeline_end_us()?;
        right.timeline_end_us()?;
        Ok((left, right))
    }
}

#[cfg(test)]
mod timing_tests {
    use super::*;
    #[test]
    fn source_clock_and_split_preserve_nonzero_source_offset_and_overhang() {
        let value = CompositionReferenceTiming {
            composition: "child".into(),
            timeline_start_us: 1_000_000,
            source_in_us: 500_000,
            source_out_us: 2_500_000,
        };
        assert_eq!(value.source_time_us(999_999).unwrap(), None);
        assert_eq!(value.source_time_us(1_500_000).unwrap(), Some(1_000_000));
        assert_eq!(value.source_time_us(3_000_000).unwrap(), None);
        let (left, right) = value.split(2_000_000).unwrap();
        assert_eq!(left.timeline_end_us().unwrap(), right.timeline_start_us);
        assert_eq!(
            right.timeline_end_us().unwrap(),
            value.timeline_end_us().unwrap()
        );
        assert_eq!(
            right.source_time_us(2_250_000).unwrap(),
            value.source_time_us(2_250_000).unwrap()
        );
        assert!(value.split(1_000_000).is_err());
        assert!(value.split(3_000_000).is_err());
        let mut invalid = value;
        invalid.timeline_start_us = u64::MAX;
        assert!(invalid.timeline_end_us().is_err());
    }
}
