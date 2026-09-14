//! Render-cache identity helpers for edit workers.
//!
//! Worker lookup keys use compiled `EditPlan.plan_fingerprint`. Enqueue-time
//! helpers may still hash wire JSON for durable dedupe.

use sha2::{Digest, Sha256};

use crate::domain::artifact_graph::Fingerprint;
use crate::model::EditRequest;
use crate::state::ToolInfo;

pub const LEGACY_RENDER_CACHE_PIPELINE_VERSION: &str = "render-cache-v2-color-pipeline-v2";
/// Cache identity is derived from compiled `EditPlan.plan_fingerprint` (not wire JSON).
pub const RENDER_CACHE_PIPELINE_VERSION: &str = "render-cache-v4-plan-fingerprint";

pub fn render_cache_key(req: &EditRequest) -> String {
    render_cache_key_with_context(req, None, None)
}

pub fn render_cache_key_for_tools(req: &EditRequest, tools: &ToolInfo) -> String {
    let fingerprint = render_runtime_fingerprint(tools);
    render_cache_key_with_context(req, Some(&fingerprint), None)
}

pub(super) fn render_cache_key_with_context(
    req: &EditRequest,
    runtime_fingerprint: Option<&str>,
    lut_sha256: Option<&str>,
) -> String {
    render_cache_key_with_pipeline(
        RENDER_CACHE_PIPELINE_VERSION,
        req,
        runtime_fingerprint,
        lut_sha256,
    )
}

pub(super) fn render_cache_key_with_pipeline(
    pipeline_version: &str,
    req: &EditRequest,
    runtime_fingerprint: Option<&str>,
    lut_sha256: Option<&str>,
) -> String {
    let canonical = serde_json::to_string(req).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(pipeline_version.as_bytes());
    hasher.update([0]);
    hasher.update(runtime_fingerprint.unwrap_or("missing").as_bytes());
    hasher.update([0]);
    hasher.update(lut_sha256.unwrap_or("no-lut").as_bytes());
    hasher.update([0]);
    hasher.update(canonical.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn plan_render_cache_key(
    pipeline_version: &str,
    plan_fingerprint: &Fingerprint,
    runtime_fingerprint: &str,
    lut_sha256: Option<&str>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(pipeline_version.as_bytes());
    hasher.update([0]);
    hasher.update(plan_fingerprint.as_str().as_bytes());
    hasher.update([0]);
    hasher.update(runtime_fingerprint.as_bytes());
    hasher.update([0]);
    hasher.update(lut_sha256.unwrap_or("no-lut").as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn render_runtime_fingerprint(tools: &ToolInfo) -> String {
    let mut encoders = tools.ffmpeg_encoders.clone();
    let mut muxers = tools.ffmpeg_muxers.clone();
    let mut filters = tools.ffmpeg_filters.clone();
    encoders.sort();
    muxers.sort();
    filters.sort();

    let mut hash = Sha256::new();
    hash.update(tools.ffmpeg_version.as_deref().unwrap_or("missing"));
    for values in [&encoders, &muxers, &filters] {
        hash.update([0]);
        for value in values {
            hash.update(value.as_bytes());
            hash.update([0]);
        }
    }
    format!("{:x}", hash.finalize())
}
