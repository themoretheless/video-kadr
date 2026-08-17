//! Multi-input `filter_complex` builder shared by every parity-wave feature.
//!
//! `args.rs` stays the FFmpeg entry point but delegates the parts of the graph
//! that need extra inputs or explicit pad labels to this module. Each feature
//! agent owns exactly one sibling file and appends to the plan through the
//! `ComplexPlan` API; nobody writes raw `-filter_complex` text anywhere else.
//!
//! An untouched edit produces a trivial plan (only the primary input, no
//! statements), and `args.rs` then emits exactly the arguments it emits today.

pub mod audio_mix;
pub mod color;
pub mod motion;
pub mod overlays;
pub mod spatial;
pub mod titles;
pub mod transitions;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::domain::edit::EditSpec;
use crate::domain::filter_graph::{FilterGraph, MediaKind};

/// One FFmpeg input. `loop_still` repeats a single image for the whole output,
/// and `seek` becomes an input-side `-ss` for that input only.
#[derive(Debug, Clone, PartialEq)]
pub struct InputSpec {
    pub path: PathBuf,
    pub loop_still: bool,
    pub seek: Option<f64>,
}

impl InputSpec {
    pub fn source(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            loop_still: false,
            seek: None,
        }
    }

    pub fn still(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            loop_still: true,
            seek: None,
        }
    }

    pub fn seeking(mut self, seconds: f64) -> Self {
        // A non-finite or negative seek would become a malformed `-ss` value.
        self.seek = seconds.is_finite().then(|| seconds.max(0.0));
        self
    }
}

/// Resolved, immutable render context: probe facts about the primary source,
/// the expected output length, and private paths for the asset ids the edit
/// refers to. Feature modules never resolve an id to a path themselves.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderContext {
    pub width: u32,
    pub height: u32,
    pub duration_seconds: f64,
    pub has_audio: bool,
    pub fps: Option<f64>,
    pub output_duration_seconds: f64,
    assets: BTreeMap<String, PathBuf>,
}

impl RenderContext {
    pub fn new(
        width: u32,
        height: u32,
        duration_seconds: f64,
        has_audio: bool,
        fps: Option<f64>,
        output_duration_seconds: f64,
    ) -> Self {
        Self {
            width,
            height,
            duration_seconds,
            has_audio,
            fps,
            output_duration_seconds,
            assets: BTreeMap::new(),
        }
    }

    /// Register a private path for an asset id resolved by the HTTP adapter.
    pub fn with_asset(mut self, id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        self.assets.insert(id.into(), path.into());
        self
    }

    pub fn asset_path(&self, id: &str) -> Option<&Path> {
        self.assets.get(id).map(PathBuf::as_path)
    }

    /// Same as `asset_path`, but fails the render instead of silently dropping
    /// a layer the user asked for.
    pub fn require_asset(&self, id: &str) -> anyhow::Result<&Path> {
        self.asset_path(id)
            .ok_or_else(|| anyhow::anyhow!("render plan references unresolved asset '{id}'"))
    }

    /// Output aspect ratio of the primary source, or 16:9 when unknown.
    pub fn source_aspect(&self) -> f64 {
        if self.width == 0 || self.height == 0 {
            return 16.0 / 9.0;
        }
        f64::from(self.width) / f64::from(self.height)
    }
}

/// Accumulating `filter_complex` program. Statements are emitted in insertion
/// order and joined with `;`; the terminal video/audio pad labels are threaded
/// through every stage so a module never needs to know its neighbours.
#[derive(Debug, Clone, PartialEq)]
pub struct ComplexPlan {
    inputs: Vec<InputSpec>,
    statements: Vec<String>,
    video_label: String,
    audio_label: Option<String>,
    /// Monotonic counter behind `next_label`, so pad names never collide.
    label_sequence: usize,
    /// Set by `transitions::apply` when it has built the whole timeline from
    /// `clips`/`segments`. `args.rs` uses it to tell "nothing consumed the
    /// segment list" apart from "some later stage rebound the video pad".
    timeline_consumed: bool,
}

impl ComplexPlan {
    /// Start a plan whose input 0 is the primary source. The initial pads are
    /// the raw input streams, `0:v` and (when present) `0:a`.
    pub fn new(primary: InputSpec, has_audio: bool) -> Self {
        Self {
            inputs: vec![primary],
            statements: Vec::new(),
            video_label: "0:v".to_owned(),
            audio_label: has_audio.then(|| "0:a".to_owned()),
            label_sequence: 0,
            timeline_consumed: false,
        }
    }

    /// Append an FFmpeg input and return its index, which is what a statement
    /// references as `[<index>:v]` or `[<index>:a]`.
    pub fn add_input(&mut self, spec: InputSpec) -> usize {
        self.inputs.push(spec);
        self.inputs.len() - 1
    }

    pub fn inputs(&self) -> &[InputSpec] {
        &self.inputs
    }

    /// A unique pad name derived from `hint`. The hint is sanitized to
    /// `[a-z0-9_]` so a label can never inject filter syntax.
    pub fn next_label(&mut self, hint: &str) -> String {
        let sanitized: String = hint
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        let base = if sanitized.trim_matches('_').is_empty() {
            "pad".to_owned()
        } else {
            sanitized
        };
        self.label_sequence += 1;
        format!("{base}_{}", self.label_sequence)
    }

    pub fn push(&mut self, statement: String) {
        if !statement.trim().is_empty() {
            self.statements.push(statement);
        }
    }

    /// Append a linear video chain to the current terminal video pad and make
    /// its output the new terminal pad. An empty chain is a no-op.
    pub fn chain_video(&mut self, filters: &[String], hint: &str) -> anyhow::Result<()> {
        if filters.is_empty() {
            return Ok(());
        }
        let chain = serialize_chain(MediaKind::Video, filters)?;
        let label = self.next_label(hint);
        let current = self.video_label.clone();
        self.push(format!("[{current}]{chain}[{label}]"));
        self.video_label = label;
        Ok(())
    }

    /// Audio counterpart of `chain_video`. Without an audio pad there is
    /// nothing to chain, so the call is a no-op rather than an error.
    pub fn chain_audio(&mut self, filters: &[String], hint: &str) -> anyhow::Result<()> {
        if filters.is_empty() {
            return Ok(());
        }
        let Some(current) = self.audio_label.clone() else {
            return Ok(());
        };
        let chain = serialize_chain(MediaKind::Audio, filters)?;
        let label = self.next_label(hint);
        self.push(format!("[{current}]{chain}[{label}]"));
        self.audio_label = Some(label);
        Ok(())
    }

    /// Record that a stage has consumed the `clips`/`segments` timeline.
    pub fn mark_timeline_consumed(&mut self) {
        self.timeline_consumed = true;
    }

    pub fn timeline_consumed(&self) -> bool {
        self.timeline_consumed
    }

    pub fn video_label(&self) -> &str {
        &self.video_label
    }

    pub fn audio_label(&self) -> Option<&str> {
        self.audio_label.as_deref()
    }

    pub fn set_video_label(&mut self, label: String) {
        self.video_label = label;
    }

    pub fn set_audio_label(&mut self, label: Option<String>) {
        self.audio_label = label;
    }

    /// The `-filter_complex` value for this plan.
    pub fn render(&self) -> String {
        self.statements.join(";")
    }

    /// True when nothing was added: one input, no statements, and the terminal
    /// pads are still the raw input streams. `args.rs` then takes the legacy
    /// path and emits byte-identical arguments.
    pub fn is_trivial(&self) -> bool {
        self.inputs.len() == 1
            && self.statements.is_empty()
            && self.video_label == "0:v"
            && matches!(self.audio_label.as_deref(), None | Some("0:a"))
    }
}

/// Validate and serialize a linear chain through the existing DAG builder, so
/// a malformed operation is caught before it reaches FFmpeg.
fn serialize_chain(media: MediaKind, filters: &[String]) -> anyhow::Result<String> {
    FilterGraph::linear(media, filters)
        .and_then(|graph| graph.ffmpeg_linear_chain())
        .map_err(|error| anyhow::anyhow!("invalid {media:?} filter chain: {error}"))
}

/// Run every feature stage in the fixed contract order.
///
/// Stages 3, 5, 7 and 10 of the documented order (existing geometry, look and
/// LUT, scale and finishing, fades) are still emitted by `args.rs` around this
/// plan. They are not called from here, and the ordering only becomes visible
/// once a feature module stops being a stub; see the module docs of each file.
pub fn compose(plan: &mut ComplexPlan, spec: &EditSpec, ctx: &RenderContext) -> anyhow::Result<()> {
    transitions::apply(plan, spec, ctx)?;
    spatial::apply(plan, spec, ctx)?;
    color::apply(plan, spec, ctx)?;
    motion::apply(plan, spec, ctx)?;
    // Overlays publish the audio pads of every picture-in-picture layer; the
    // mixer is the stage that actually places and sums them.
    let overlay_pads = overlays::apply_with_audio(plan, spec, ctx)?;
    titles::apply(plan, spec, ctx)?;
    audio_mix::apply_with_overlays(plan, spec, ctx, &overlay_pads)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> ComplexPlan {
        ComplexPlan::new(InputSpec::source("/in.mp4"), true)
    }

    fn context() -> RenderContext {
        RenderContext::new(1920, 1080, 10.0, true, Some(30.0), 10.0)
    }

    #[test]
    fn a_fresh_plan_is_trivial_and_renders_nothing() {
        let plan = plan();
        assert!(plan.is_trivial());
        assert_eq!(plan.render(), "");
        assert_eq!(plan.video_label(), "0:v");
        assert_eq!(plan.audio_label(), Some("0:a"));
        assert_eq!(plan.inputs().len(), 1);

        let silent = ComplexPlan::new(InputSpec::source("/in.mp4"), false);
        assert!(silent.is_trivial());
        assert_eq!(silent.audio_label(), None);
    }

    #[test]
    fn extra_inputs_are_indexed_from_one_and_break_triviality() {
        let mut plan = plan();
        assert_eq!(plan.add_input(InputSpec::still("/logo.png")), 1);
        assert_eq!(plan.add_input(InputSpec::source("/music.mp3")), 2);
        assert_eq!(plan.inputs()[1].path, PathBuf::from("/logo.png"));
        assert!(plan.inputs()[1].loop_still);
        assert!(!plan.is_trivial());
    }

    #[test]
    fn labels_are_unique_and_cannot_carry_filter_syntax() {
        let mut plan = plan();
        let first = plan.next_label("overlay");
        let second = plan.next_label("overlay");
        assert_ne!(first, second);
        assert_eq!(first, "overlay_1");
        assert_eq!(second, "overlay_2");
        assert_eq!(
            plan.next_label("a];drawtext=text=x[b"),
            "a__drawtext_text_x_b_3"
        );
        assert_eq!(plan.next_label("[]"), "pad_4");
    }

    #[test]
    fn chaining_threads_the_terminal_labels() {
        let mut plan = plan();
        plan.chain_video(&["hflip".into(), "vflip".into()], "geometry")
            .unwrap();
        plan.chain_audio(&["volume=2.000".into()], "gain").unwrap();
        plan.chain_video(&["vignette".into()], "finish").unwrap();

        assert_eq!(plan.video_label(), "finish_3");
        assert_eq!(plan.audio_label(), Some("gain_2"));
        assert_eq!(
            plan.render(),
            "[0:v]hflip,vflip[geometry_1];[0:a]volume=2.000[gain_2];[geometry_1]vignette[finish_3]"
        );
        assert!(!plan.is_trivial());
    }

    #[test]
    fn empty_chains_and_missing_audio_are_no_ops() {
        let mut plan = ComplexPlan::new(InputSpec::source("/in.mp4"), false);
        plan.chain_video(&[], "noop").unwrap();
        plan.chain_audio(&["volume=2.000".into()], "gain").unwrap();
        assert!(plan.is_trivial());
        assert_eq!(plan.render(), "");
    }

    #[test]
    fn explicit_labels_can_replace_the_terminal_pads() {
        let mut plan = plan();
        let label = plan.next_label("concat");
        plan.push(format!("[0:v][0:v]concat=n=2:v=1:a=0[{label}]"));
        plan.set_video_label(label.clone());
        plan.set_audio_label(None);

        assert_eq!(plan.video_label(), label);
        assert_eq!(plan.audio_label(), None);
        assert!(!plan.is_trivial());
    }

    #[test]
    fn an_invalid_chain_is_rejected_before_it_reaches_ffmpeg() {
        let mut plan = plan();
        assert!(plan.chain_video(&["   ".into()], "broken").is_err());
    }

    #[test]
    fn composing_an_untouched_edit_leaves_the_plan_trivial() {
        let request: crate::model::EditRequest =
            serde_json::from_value(serde_json::json!({ "videoId": "x" })).unwrap();
        let extensions = crate::domain::edit::EditExtensions::from_request(&request).unwrap();
        let spec = crate::services::render::EditPlan::compile(
            crate::domain::artifact_graph::Fingerprint::digest(b"source"),
            request,
            crate::services::render::SourceMediaMetadata::new(1920, 1080, 10.0).unwrap(),
        )
        .unwrap()
        .edit
        .with_extensions(extensions)
        .unwrap();

        let mut plan = plan();
        compose(&mut plan, &spec, &context()).unwrap();
        assert!(plan.is_trivial());
        assert_eq!(plan.render(), "");
    }

    #[test]
    fn render_context_resolves_only_registered_assets() {
        let ctx = context().with_asset("ast_0123456789abcdef", "/private/assets/logo.png");
        assert_eq!(
            ctx.asset_path("ast_0123456789abcdef"),
            Some(Path::new("/private/assets/logo.png"))
        );
        assert!(ctx.asset_path("ast_unknown").is_none());
        assert!(ctx.require_asset("ast_unknown").is_err());
        assert!((ctx.source_aspect() - 16.0 / 9.0).abs() < 1e-9);
    }
}
