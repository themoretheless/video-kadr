//! Pure editor domain. Modules in this tree own media semantics and must not
//! depend on HTTP extractors, async runtimes, subprocesses, or persistence.

pub mod filter_graph;
pub mod geometry;
pub mod keyframes;
pub mod media_pipeline;
pub mod media_probe;
pub mod timeline;
