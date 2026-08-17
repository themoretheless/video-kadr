//! Application port for compiling an immutable render execution into an
//! external-tool command. Paths and resource profiles belong to the request;
//! codec/filter semantics remain owned by the selected adapter.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::services::render::RenderExecution;

pub struct ExportCompileRequest<'a> {
    pub input: &'a Path,
    pub destination: &'a Path,
    pub parallel_jobs: usize,
    pub execution: &'a RenderExecution,
}

/// An analysis pass the runner must complete before the render pass. Today only
/// two-pass stabilization needs one: it writes a frame-indexed transform file
/// that the render command reads back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPrepass {
    pub arguments: Vec<String>,
    /// File the pass writes; the runner must remove it afterwards.
    pub output: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledExportCommand {
    pub arguments: Vec<String>,
    pub expected_duration_seconds: f64,
    /// Auxiliary immutable files referenced from filter options rather than
    /// regular `-i` arguments (for example a private `.cube` LUT).
    pub read_only_files: Vec<PathBuf>,
    /// Optional analysis pass to run first.
    pub prepass: Option<CompiledPrepass>,
}

pub trait ExportCommandCompiler: Send + Sync {
    fn compile(&self, request: ExportCompileRequest<'_>) -> Result<CompiledExportCommand>;
}
