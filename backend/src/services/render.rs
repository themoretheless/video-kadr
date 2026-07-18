//! Offline render service contract: immutable edit plan plus resource policy.

use std::sync::Arc;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::config::encode_budget::EncodeBudget;
use crate::domain::artifact_graph::Fingerprint;
use crate::domain::output::OutputSpec;
use crate::model::EditRequest;

const EDIT_PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditPlan {
    pub schema_version: u32,
    pub source_fingerprint: Fingerprint,
    pub plan_fingerprint: Fingerprint,
    pub output: OutputSpec,
    pub edit: EditRequest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditPlanWire {
    schema_version: u32,
    source_fingerprint: Fingerprint,
    plan_fingerprint: Fingerprint,
    output: OutputSpec,
    edit: EditRequest,
}

impl<'de> Deserialize<'de> for EditPlan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = EditPlanWire::deserialize(deserializer)?;
        if wire.schema_version != EDIT_PLAN_SCHEMA_VERSION {
            return Err(D::Error::custom("unsupported edit plan schema"));
        }
        let compiled = Self::compile(wire.source_fingerprint, wire.edit);
        if wire.plan_fingerprint != compiled.plan_fingerprint || wire.output != compiled.output {
            return Err(D::Error::custom(
                "edit plan identity does not match its source and edit",
            ));
        }
        Ok(compiled)
    }
}

impl EditPlan {
    pub fn compile(source_fingerprint: Fingerprint, edit: EditRequest) -> Self {
        let output = OutputSpec::from_edit(&edit);
        let canonical = serde_json::to_vec(&edit).expect("EditRequest serialization cannot fail");
        let schema = EDIT_PLAN_SCHEMA_VERSION.to_be_bytes();
        let plan_fingerprint = Fingerprint::combine([
            b"edit-plan".as_slice(),
            schema.as_slice(),
            source_fingerprint.as_str().as_bytes(),
            canonical.as_slice(),
        ]);
        Self {
            schema_version: EDIT_PLAN_SCHEMA_VERSION,
            source_fingerprint,
            plan_fingerprint,
            output,
            edit,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportExecutionProfile {
    pub encode_budget: EncodeBudget,
    pub verify_checksums: bool,
}

#[derive(Debug, Clone)]
pub struct RenderExecution {
    plan: Arc<EditPlan>,
    pub profile: ExportExecutionProfile,
}

impl RenderExecution {
    pub fn new(plan: Arc<EditPlan>, profile: ExportExecutionProfile) -> Self {
        Self { plan, profile }
    }

    pub fn plan(&self) -> &EditPlan {
        &self.plan
    }

    pub fn output(&self) -> &OutputSpec {
        &self.plan.output
    }
}

#[cfg(test)]
mod tests {
    use crate::config::encode_budget::{EncodeProfile, RuntimeLimits};

    use super::*;

    #[test]
    fn edit_plan_fingerprint_is_deterministic_and_source_sensitive() {
        let edit: EditRequest = serde_json::from_value(serde_json::json!({
            "videoId": "source",
            "trim": {"start": 1.0, "end": 2.0}
        }))
        .unwrap();
        let source = Fingerprint::digest(b"source-a");
        let first = EditPlan::compile(source.clone(), edit.clone());
        let second = EditPlan::compile(source, edit.clone());
        let other = EditPlan::compile(Fingerprint::digest(b"source-b"), edit);
        assert_eq!(first.plan_fingerprint, second.plan_fingerprint);
        assert_ne!(first.plan_fingerprint, other.plan_fingerprint);
    }

    #[test]
    fn export_policy_does_not_own_output_semantics() {
        let edit = serde_json::from_value(serde_json::json!({
            "videoId": "source",
            "quality": 18
        }))
        .unwrap();
        let plan = Arc::new(EditPlan::compile(Fingerprint::digest(b"source"), edit));
        let profile = ExportExecutionProfile {
            encode_budget: EncodeBudget::for_profile(
                EncodeProfile::Balanced,
                RuntimeLimits {
                    logical_cpus: 4,
                    memory_mib: Some(2048),
                },
            )
            .unwrap(),
            verify_checksums: true,
        };
        let execution = RenderExecution::new(plan.clone(), profile);
        assert_eq!(execution.output(), &plan.output);
    }

    #[test]
    fn deserialization_rejects_tampered_plan_identity_and_output() {
        let edit = serde_json::from_value(serde_json::json!({
            "videoId": "source",
            "format": "av1",
            "quality": 28
        }))
        .unwrap();
        let plan = EditPlan::compile(Fingerprint::digest(b"source"), edit);
        let mut value = serde_json::to_value(&plan).unwrap();
        value["output"]["crf"] = serde_json::json!(1);
        assert!(serde_json::from_value::<EditPlan>(value).is_err());

        let mut value = serde_json::to_value(&plan).unwrap();
        value["planFingerprint"] = serde_json::json!(Fingerprint::digest(b"tampered"));
        assert!(serde_json::from_value::<EditPlan>(value).is_err());
    }
}
