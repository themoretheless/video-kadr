//! Pure editor domain. Modules in this tree own media semantics and must not
//! depend on HTTP extractors, async runtimes, subprocesses, or persistence.

pub mod arithmetic;
pub mod artifact_graph;
pub mod audio_output;
#[cfg(feature = "spec-contracts")]
pub mod audio_pipeline;
#[cfg(feature = "spec-contracts")]
pub mod color_pipeline;
pub mod composition;
pub mod edit;
pub mod filter_graph;
#[cfg(feature = "spec-contracts")]
pub mod geometry;
pub mod keyframes;
pub mod media_contract;
#[cfg(feature = "spec-contracts")]
pub mod media_pipeline;
pub mod media_probe;
pub mod output;
pub mod project_collaboration;
#[cfg(feature = "spec-contracts")]
pub mod project_version;
#[cfg(feature = "spec-contracts")]
pub mod still_container;
#[cfg(feature = "spec-contracts")]
pub mod timeline;
