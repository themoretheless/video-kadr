//! Application port for compiling a validated multi-source composition into
//! an external-tool command. Source identifiers remain part of the immutable
//! composition while resolved filesystem paths are supplied only at execution.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::composition::{Composition, SourceId};

use super::media_export::CompiledExportCommand;

pub struct CompositionExportCompileRequest<'a> {
    pub inputs: &'a BTreeMap<SourceId, PathBuf>,
    pub destination: &'a Path,
    pub parallel_jobs: usize,
    pub composition: &'a Composition,
    /// H.264 constant-rate-factor value for the first MP4-only slice.
    pub quality: u32,
}

pub trait CompositionExportCommandCompiler: Send + Sync {
    fn compile(
        &self,
        request: CompositionExportCompileRequest<'_>,
    ) -> Result<CompiledExportCommand>;
}
