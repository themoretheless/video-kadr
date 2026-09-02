//! Pure editor domain. Modules in this tree own media semantics and must not
//! depend on HTTP extractors, async runtimes, subprocesses, or persistence.

pub mod artifact_graph;
pub mod audio_output;
pub mod audio_pipeline;
pub mod color_pipeline;
pub mod composition;
pub mod edit;
pub mod filter_graph;
pub mod geometry;
pub mod keyframes;
pub mod media_contract;
pub mod media_pipeline;
pub mod media_probe;
pub mod output;
pub mod project_collaboration;
pub mod project_version;
pub mod still_container;
pub mod timeline;
