//! Interactive preview policy over the same immutable `EditPlan` used by export.

use std::sync::Arc;

use crate::domain::output::OutputSpec;

use super::render::EditPlan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewExecutionProfile {
    pub max_width: u32,
    pub max_fps: u32,
    pub frame_cache_mib: u64,
    pub prefer_proxy: bool,
}

impl Default for PreviewExecutionProfile {
    fn default() -> Self {
        Self {
            max_width: 1280,
            max_fps: 30,
            frame_cache_mib: 256,
            prefer_proxy: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PreviewExecution {
    plan: Arc<EditPlan>,
    pub profile: PreviewExecutionProfile,
}

impl PreviewExecution {
    pub fn new(plan: Arc<EditPlan>, profile: PreviewExecutionProfile) -> Self {
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
    use crate::domain::artifact_graph::Fingerprint;

    use super::*;

    #[test]
    fn preview_quality_policy_cannot_change_final_output_spec() {
        let edit = serde_json::from_value(serde_json::json!({
            "videoId": "source",
            "format": "av1",
            "quality": 20,
            "fps": 60.0
        }))
        .unwrap();
        let plan = Arc::new(EditPlan::compile(Fingerprint::digest(b"source"), edit));
        let preview = PreviewExecution::new(
            plan.clone(),
            PreviewExecutionProfile {
                max_width: 640,
                max_fps: 12,
                frame_cache_mib: 32,
                prefer_proxy: true,
            },
        );
        assert_eq!(preview.output(), &plan.output);
        assert_eq!(preview.output().crf, Some(20));
        assert_eq!(preview.output().fps_milli, Some(60_000));
    }
}
