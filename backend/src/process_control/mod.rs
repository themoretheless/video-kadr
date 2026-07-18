//! Mandatory policy boundary for external processes.
//!
//! Tool adapters describe capabilities and budgets. Only a `PreparedCommand`
//! can cross the production spawn boundary, so every new subprocess must state
//! its filesystem, network, environment, kernel, and output policy.

mod execution;
mod limits;
mod policy;
mod runtime;

pub use policy::{
    EnvironmentAccess, FilesystemAccess, IsolationTier, KernelLimits, NetworkAccess, OutputBudget,
    ProcessPolicy, ProcessRuntimeConfig, SandboxBackend, ToolRole,
};
pub use runtime::{PreparedCommand, ProcessRuntime};

#[cfg(test)]
pub(crate) use execution::test_timeout_error;
pub(crate) use execution::{
    capture_output, is_process_timeout, stream_with_progress, ProcessStatus,
};
