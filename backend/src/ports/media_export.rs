//! Application port for compiling an immutable render execution into an
//! external-tool command. Paths and resource profiles belong to the request;
//! codec/filter semantics remain owned by the selected adapter.

use std::path::Path;

use anyhow::Result;

use crate::services::render::RenderExecution;

pub struct ExportCompileRequest<'a> {
    pub input: &'a Path,
    pub destination: &'a Path,
    pub parallel_jobs: usize,
    pub execution: &'a RenderExecution,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledExportCommand {
    pub arguments: Vec<String>,
    pub expected_duration_seconds: f64,
}

pub trait ExportCommandCompiler: Send + Sync {
    fn compile(&self, request: ExportCompileRequest<'_>) -> Result<CompiledExportCommand>;
}
