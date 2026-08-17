//! Pure editor domain. Modules in this tree own media semantics and must not
//! depend on HTTP extractors, async runtimes, subprocesses, or persistence.
//! They may read `crate::model` wire DTOs to validate them into domain types.

pub mod artifact_graph;
pub mod audio_mix;
pub mod color_grade;
pub mod composition;
pub mod edit;
pub mod filter_graph;
pub mod geometry;
pub mod keyframes;
pub mod media_pipeline;
pub mod media_probe;
pub mod motion;
pub mod output;
pub mod overlay;
pub mod spatial;
pub mod timeline;
