use std::fmt;
use std::net::IpAddr;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationTier {
    Local,
    Lan,
    Public,
}

impl IsolationTier {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "local" => Ok(Self::Local),
            "lan" => Ok(Self::Lan),
            "public" => Ok(Self::Public),
            _ => Err(anyhow!("ISOLATION_TIER must be local, lan, or public")),
        }
    }
}

impl fmt::Display for IsolationTier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Local => "local",
            Self::Lan => "lan",
            Self::Public => "public",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxBackend {
    None,
    NsJail,
    Bubblewrap,
}

impl SandboxBackend {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "none" => Ok(Self::None),
            "nsjail" => Ok(Self::NsJail),
            "bubblewrap" | "bwrap" => Ok(Self::Bubblewrap),
            _ => Err(anyhow!(
                "PROCESS_SANDBOX must be none, nsjail, or bubblewrap"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRole {
    Discovery,
    Probe,
    Render,
    Download,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkAccess {
    Denied,
    PinnedLoopbackProxy(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilesystemAccess {
    pub read_only: Vec<PathBuf>,
    pub read_write: Vec<PathBuf>,
}

impl FilesystemAccess {
    pub fn discovery() -> Self {
        Self::default()
    }

    pub fn read_only(path: impl Into<PathBuf>) -> Self {
        Self {
            read_only: vec![path.into()],
            read_write: Vec::new(),
        }
    }

    pub fn media(read_only: Vec<PathBuf>, read_write: Vec<PathBuf>) -> Self {
        Self {
            read_only,
            read_write,
        }
    }

    fn validate(&self, role: ToolRole) -> Result<()> {
        if role == ToolRole::Probe && self.read_only.is_empty() {
            return Err(anyhow!("probe policy requires a read-only media path"));
        }
        if role == ToolRole::Render && (self.read_only.is_empty() || self.read_write.is_empty()) {
            return Err(anyhow!(
                "render policy requires read-only inputs and a writable output scope"
            ));
        }
        if role == ToolRole::Download && self.read_write.is_empty() {
            return Err(anyhow!("download policy requires a writable destination"));
        }
        for path in self.read_only.iter().chain(&self.read_write) {
            if path.as_os_str().is_empty() {
                return Err(anyhow!("process filesystem scope contains an empty path"));
            }
            let value = path.to_string_lossy().to_ascii_lowercase();
            if ["http://", "https://", "tcp://", "udp://", "rtmp://"]
                .iter()
                .any(|scheme| value.starts_with(scheme))
            {
                return Err(anyhow!(
                    "offline process filesystem scope cannot contain a network URL"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentAccess {
    pub inherit: Vec<String>,
}

impl EnvironmentAccess {
    pub(super) fn for_role(role: ToolRole) -> Self {
        let mut inherit = ["PATH", "TMPDIR", "TEMP", "TMP", "LANG", "LC_ALL"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if role == ToolRole::Download {
            inherit.extend(["SSL_CERT_FILE".to_owned(), "SSL_CERT_DIR".to_owned()]);
        }
        Self { inherit }
    }

    fn validate(&self, role: ToolRole) -> Result<()> {
        const BASE: &[&str] = &["PATH", "TMPDIR", "TEMP", "TMP", "LANG", "LC_ALL"];
        const DOWNLOAD_ONLY: &[&str] = &["SSL_CERT_FILE", "SSL_CERT_DIR"];
        for name in &self.inherit {
            if !BASE.contains(&name.as_str())
                && !(role == ToolRole::Download && DOWNLOAD_ONLY.contains(&name.as_str()))
            {
                return Err(anyhow!(
                    "process policy attempts to inherit a non-allowlisted environment variable"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelLimits {
    pub cpu_seconds: u64,
    pub address_space_bytes: u64,
    pub child_processes: u64,
    pub open_files: u64,
    pub file_size_bytes: u64,
}

impl KernelLimits {
    pub(super) fn validate(self) -> Result<Self> {
        if self.cpu_seconds == 0
            || self.address_space_bytes < 128 * 1024 * 1024
            || self.child_processes == 0
            || self.open_files < 16
            || self.file_size_bytes == 0
        {
            return Err(anyhow!("external process kernel limits are invalid"));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputBudget {
    pub capture_bytes: usize,
    pub line_bytes: usize,
}

impl OutputBudget {
    pub(super) fn validate(self) -> Result<Self> {
        if self.capture_bytes == 0 || self.line_bytes == 0 || self.line_bytes > self.capture_bytes {
            return Err(anyhow!("external process output budget is invalid"));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRuntimeConfig {
    pub tier: IsolationTier,
    pub sandbox: SandboxBackend,
    pub kernel: KernelLimits,
    pub output: OutputBudget,
}

impl ProcessRuntimeConfig {
    pub fn local_default() -> Self {
        Self {
            tier: IsolationTier::Local,
            sandbox: SandboxBackend::None,
            kernel: KernelLimits {
                cpu_seconds: 2 * 60 * 60,
                address_space_bytes: 2 * 1024 * 1024 * 1024,
                child_processes: 64,
                open_files: 256,
                file_size_bytes: 4 * 1024 * 1024 * 1024,
            },
            output: OutputBudget {
                capture_bytes: 2 * 1024 * 1024,
                line_bytes: 64 * 1024,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessPolicy {
    pub role: ToolRole,
    pub filesystem: FilesystemAccess,
    pub network: NetworkAccess,
    pub environment: EnvironmentAccess,
    pub kernel: KernelLimits,
    pub output: OutputBudget,
}

impl ProcessPolicy {
    pub(super) fn validate(&self) -> Result<()> {
        self.filesystem.validate(self.role)?;
        self.environment.validate(self.role)?;
        self.kernel.validate()?;
        self.output.validate()?;
        match (&self.role, &self.network) {
            (ToolRole::Download, NetworkAccess::PinnedLoopbackProxy(url)) => {
                validate_loopback_proxy(url)?;
            }
            (ToolRole::Download, NetworkAccess::Denied) => {
                return Err(anyhow!("download process requires a pinned egress proxy"));
            }
            (_, NetworkAccess::PinnedLoopbackProxy(_)) => {
                return Err(anyhow!("only the downloader may receive network egress"));
            }
            (_, NetworkAccess::Denied) => {}
        }
        Ok(())
    }
}

fn validate_loopback_proxy(value: &str) -> Result<()> {
    let url = Url::parse(value).context("parse pinned process proxy URL")?;
    if url.scheme() != "http"
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url
            .host_str()
            .and_then(|host| host.parse::<IpAddr>().ok())
            .is_some_and(|host| host.is_loopback())
        || url.port().is_none()
    {
        return Err(anyhow!(
            "downloader proxy must be a credential-free loopback HTTP URL with an explicit port"
        ));
    }
    Ok(())
}
