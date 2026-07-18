use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, Result};
use tokio::process::{Child, Command};

use super::limits::{configure_kernel_limits, configure_process_group};
use super::{
    EnvironmentAccess, FilesystemAccess, IsolationTier, NetworkAccess, OutputBudget, ProcessPolicy,
    ProcessRuntimeConfig, SandboxBackend, ToolRole,
};

#[derive(Debug, Clone)]
pub struct ProcessRuntime {
    config: ProcessRuntimeConfig,
}

impl ProcessRuntime {
    pub fn new(config: ProcessRuntimeConfig) -> Result<Self> {
        config.kernel.validate()?;
        config.output.validate()?;
        if config.sandbox != SandboxBackend::None {
            return Err(anyhow!(
                "PROCESS_SANDBOX selects an adapter that is not implemented in this build"
            ));
        }
        Ok(Self { config })
    }

    pub fn local_default() -> Self {
        Self::new(ProcessRuntimeConfig::local_default())
            .expect("static local process policy is valid")
    }

    pub fn tier(&self) -> IsolationTier {
        self.config.tier
    }

    pub fn hard_memory_limit_enforced(&self) -> bool {
        cfg!(any(target_os = "linux", target_os = "android"))
    }

    pub fn hard_pid_limit_enforced(&self) -> bool {
        false
    }

    pub fn validate_deployment(&self, bind_addr: IpAddr) -> Result<()> {
        match self.config.tier {
            IsolationTier::Local if !bind_addr.is_loopback() => Err(anyhow!(
                "local isolation tier requires a loopback BIND_ADDR; choose lan explicitly"
            )),
            IsolationTier::Local | IsolationTier::Lan => Ok(()),
            IsolationTier::Public => Err(anyhow!(
                "public isolation tier is fail-closed: authentication, per-tenant ownership, and an enforced NsJail/Bubblewrap adapter are not all implemented"
            )),
        }
    }

    pub fn discovery_policy(&self) -> ProcessPolicy {
        self.policy(
            ToolRole::Discovery,
            FilesystemAccess::discovery(),
            NetworkAccess::Denied,
        )
    }

    pub fn probe_policy(&self, input: impl Into<PathBuf>) -> ProcessPolicy {
        self.policy(
            ToolRole::Probe,
            FilesystemAccess::read_only(input),
            NetworkAccess::Denied,
        )
    }

    pub fn render_policy(&self, inputs: Vec<PathBuf>, output: impl Into<PathBuf>) -> ProcessPolicy {
        let output = output.into();
        let writable = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        self.policy(
            ToolRole::Render,
            FilesystemAccess::media(inputs, vec![writable]),
            NetworkAccess::Denied,
        )
    }

    pub fn download_policy(
        &self,
        destination: impl Into<PathBuf>,
        proxy_url: impl Into<String>,
    ) -> ProcessPolicy {
        self.policy(
            ToolRole::Download,
            FilesystemAccess::media(Vec::new(), vec![destination.into()]),
            NetworkAccess::PinnedLoopbackProxy(proxy_url.into()),
        )
    }

    fn policy(
        &self,
        role: ToolRole,
        filesystem: FilesystemAccess,
        network: NetworkAccess,
    ) -> ProcessPolicy {
        ProcessPolicy {
            role,
            filesystem,
            network,
            environment: EnvironmentAccess::for_role(role),
            kernel: self.config.kernel,
            output: self.config.output,
        }
    }

    pub fn prepare(&self, mut command: Command, policy: ProcessPolicy) -> Result<PreparedCommand> {
        policy.validate()?;
        if self.config.tier == IsolationTier::Public {
            return Err(anyhow!("public subprocess execution is disabled"));
        }

        let inherited = inherited_environment(&policy.environment);
        command.env_clear();
        for (name, value) in inherited {
            command.env(name, value);
        }
        configure_network_environment(&mut command, &policy.network);
        command.stdin(Stdio::null()).kill_on_drop(true);
        configure_process_group(&mut command);
        configure_kernel_limits(&mut command, policy.kernel)?;

        Ok(PreparedCommand { command, policy })
    }
}

pub struct PreparedCommand {
    command: Command,
    policy: ProcessPolicy,
}

impl PreparedCommand {
    pub fn output_budget(&self) -> OutputBudget {
        self.policy.output
    }

    pub fn role(&self) -> ToolRole {
        self.policy.role
    }

    pub fn spawn(mut self) -> std::io::Result<Child> {
        self.command.spawn()
    }
}

fn inherited_environment(policy: &EnvironmentAccess) -> Vec<(String, String)> {
    policy
        .inherit
        .iter()
        .map(String::as_str)
        .filter_map(|name| std::env::var(name).ok().map(|value| (name.into(), value)))
        .collect()
}

fn configure_network_environment(command: &mut Command, network: &NetworkAccess) {
    for name in [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "NO_PROXY",
        "no_proxy",
    ] {
        command.env_remove(name);
    }
    if let NetworkAccess::PinnedLoopbackProxy(proxy) = network {
        for name in [
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
        ] {
            command.env(name, proxy);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployment_tiers_fail_closed() {
        let local = ProcessRuntime::local_default();
        assert!(local
            .validate_deployment(IpAddr::from([127, 0, 0, 1]))
            .is_ok());
        assert!(local
            .validate_deployment(IpAddr::from([0, 0, 0, 0]))
            .is_err());

        let mut config = ProcessRuntimeConfig::local_default();
        config.tier = IsolationTier::Lan;
        assert!(ProcessRuntime::new(config)
            .unwrap()
            .validate_deployment(IpAddr::from([0, 0, 0, 0]))
            .is_ok());

        let mut config = ProcessRuntimeConfig::local_default();
        config.tier = IsolationTier::Public;
        assert!(ProcessRuntime::new(config)
            .unwrap()
            .validate_deployment(IpAddr::from([0, 0, 0, 0]))
            .is_err());

        let mut config = ProcessRuntimeConfig::local_default();
        config.sandbox = SandboxBackend::NsJail;
        assert!(ProcessRuntime::new(config).is_err());
    }

    #[test]
    fn process_capabilities_are_explicit() {
        let runtime = ProcessRuntime::local_default();
        assert!(runtime
            .download_policy("storage", "http://127.0.0.1:4321")
            .validate()
            .is_ok());
        assert!(runtime
            .download_policy("storage", "http://example.com:4321")
            .validate()
            .is_err());

        let mut render = runtime.render_policy(vec!["in.mp4".into()], "out.mp4");
        render.network = NetworkAccess::PinnedLoopbackProxy("http://127.0.0.1:1".into());
        assert!(render.validate().is_err());
        assert!(runtime
            .render_policy(vec!["https://example.com/input.mp4".into()], "out.mp4")
            .validate()
            .is_err());

        let mut discovery = runtime.discovery_policy();
        discovery.environment.inherit.push("HOME".into());
        assert!(discovery.validate().is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn prepared_command_scrubs_unlisted_environment() {
        let runtime = ProcessRuntime::local_default();
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(
                "test -z \"$PROCESS_POLICY_CANARY\" && test -z \"$HTTP_PROXY\" && test -z \"$HOME\"",
            )
            .env("PROCESS_POLICY_CANARY", "secret")
            .env("HTTP_PROXY", "http://example.com:8080");
        let child = runtime
            .prepare(command, runtime.discovery_policy())
            .unwrap()
            .spawn()
            .unwrap();
        assert!(child.wait_with_output().await.unwrap().status.success());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn downloader_receives_only_the_pinned_loopback_proxy() {
        let runtime = ProcessRuntime::local_default();
        let proxy = "http://127.0.0.1:4321";
        let mut command = Command::new("sh");
        command.arg("-c").arg(format!(
            "test \"$HTTP_PROXY\" = '{proxy}' && test \"$HTTPS_PROXY\" = '{proxy}' && test -z \"$NO_PROXY\""
        ));
        let child = runtime
            .prepare(command, runtime.download_policy("storage", proxy))
            .unwrap()
            .spawn()
            .unwrap();
        assert!(child.wait_with_output().await.unwrap().status.success());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn file_size_limit_is_inherited_by_child_processes() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("oversized");
        let mut config = ProcessRuntimeConfig::local_default();
        config.kernel.file_size_bytes = 1024;
        let runtime = ProcessRuntime::new(config).unwrap();
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("head -c 4096 /dev/zero >\"$1\"")
            .arg("runner")
            .arg(&output)
            .stderr(Stdio::null());
        let child = runtime
            .prepare(command, runtime.discovery_policy())
            .unwrap()
            .spawn()
            .unwrap();
        let status = child.wait_with_output().await.unwrap().status;
        assert!(!status.success());
        assert!(std::fs::metadata(output).unwrap().len() <= 1024);
    }
}
