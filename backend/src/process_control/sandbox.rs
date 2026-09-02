//! Fail-closed Linux sandbox profile compiler.
//!
//! This module deliberately separates an executable NsJail configuration from
//! deployment enablement. Public startup remains blocked until authentication
//! and tenant ownership are wired; callers cannot downgrade a missing sandbox
//! into direct process execution.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use super::{FilesystemAccess, KernelLimits, NetworkAccess, ProcessPolicy, ToolRole};

const NSJAIL_PROFILE_VERSION: &str = "video-kadr-nsjail-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantIdentity {
    pub tenant_id: String,
    pub outside_uid: u32,
    pub outside_gid: u32,
    pub work_dir: PathBuf,
}

impl TenantIdentity {
    fn validate(&self) -> Result<()> {
        if !is_token(&self.tenant_id) {
            return Err(anyhow!("sandbox tenant id must be a bounded token"));
        }
        validate_absolute_path(&self.work_dir, "tenant work directory")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeccompProfile {
    pub path: PathBuf,
    pub sha256: String,
    pub tool_fingerprint: String,
}

impl SeccompProfile {
    fn validate(&self) -> Result<()> {
        validate_absolute_path(&self.path, "seccomp profile")?;
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(anyhow!("seccomp profile requires a sha256 checksum"));
        }
        if !is_token(&self.tool_fingerprint) {
            return Err(anyhow!(
                "seccomp profile requires a bounded tool fingerprint"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WasmLimits {
    pub memory_bytes: u64,
    pub fuel: u64,
    pub epoch_deadline: u64,
    pub max_host_calls: u64,
}

impl WasmLimits {
    pub fn validate(self) -> Result<Self> {
        if self.memory_bytes < 1024 * 1024
            || self.memory_bytes > 512 * 1024 * 1024
            || self.fuel == 0
            || self.epoch_deadline == 0
            || self.max_host_calls == 0
        {
            return Err(anyhow!("Wasm capability limits are invalid"));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxCapability {
    NativeTool {
        role: ToolRole,
        executable_sha256: String,
    },
    WasmPlugin {
        module_sha256: String,
        limits: WasmLimits,
        readable_assets: Vec<String>,
    },
}

impl SandboxCapability {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::NativeTool {
                executable_sha256, ..
            } => validate_sha256(executable_sha256, "native executable"),
            Self::WasmPlugin {
                module_sha256,
                limits,
                readable_assets,
            } => {
                validate_sha256(module_sha256, "Wasm module")?;
                limits.validate()?;
                if readable_assets.len() > 32 || readable_assets.iter().any(|id| !is_token(id)) {
                    return Err(anyhow!("Wasm readable assets must be bounded ids"));
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NsJailProfile {
    pub tenant: TenantIdentity,
    pub seccomp: SeccompProfile,
    pub policy: ProcessPolicy,
    pub cpu_millis_per_second: u32,
}

impl NsJailProfile {
    pub fn validate(&self) -> Result<()> {
        self.tenant.validate()?;
        self.seccomp.validate()?;
        self.policy.validate()?;
        if self.cpu_millis_per_second == 0 || self.cpu_millis_per_second > 1000 {
            return Err(anyhow!("sandbox CPU quota must be within 1..=1000 ms/s"));
        }
        if !matches!(self.policy.network, NetworkAccess::Denied) {
            return Err(anyhow!(
                "public native tools require an isolated network namespace"
            ));
        }
        validate_mounts(&self.policy.filesystem, &self.tenant.work_dir)
    }

    /// Render the official protobuf text configuration accepted by
    /// `nsjail --config`. Every mount is mandatory and the root is a private
    /// tmpfs, so paths outside the declared capability are absent.
    pub fn render_config(&self) -> Result<String> {
        self.validate()?;
        let KernelLimits {
            cpu_seconds,
            address_space_bytes,
            child_processes,
            open_files,
            file_size_bytes,
        } = self.policy.kernel;
        let mut output = String::new();
        writeln!(output, "name: \"{NSJAIL_PROFILE_VERSION}\"").unwrap();
        writeln!(output, "description: \"tenant:{}\"", self.tenant.tenant_id).unwrap();
        writeln!(output, "mode: ONCE").unwrap();
        writeln!(output, "hostname: \"media-worker\"").unwrap();
        writeln!(output, "cwd: \"{}\"", escaped_path(&self.tenant.work_dir)).unwrap();
        writeln!(output, "keep_env: false").unwrap();
        writeln!(output, "keep_caps: false").unwrap();
        writeln!(output, "disable_no_new_privs: false").unwrap();
        writeln!(output, "clone_newnet: true").unwrap();
        writeln!(output, "clone_newuser: true").unwrap();
        writeln!(output, "clone_newns: true").unwrap();
        writeln!(output, "clone_newpid: true").unwrap();
        writeln!(output, "clone_newipc: true").unwrap();
        writeln!(output, "clone_newuts: true").unwrap();
        writeln!(output, "clone_newcgroup: true").unwrap();
        writeln!(output, "iface_no_lo: true").unwrap();
        writeln!(output, "detect_cgroupv2: true").unwrap();
        writeln!(output, "time_limit: {cpu_seconds}").unwrap();
        writeln!(output, "rlimit_cpu: {cpu_seconds}").unwrap();
        writeln!(
            output,
            "rlimit_as: {}",
            address_space_bytes.div_ceil(1024 * 1024)
        )
        .unwrap();
        writeln!(output, "rlimit_nofile: {open_files}").unwrap();
        writeln!(output, "rlimit_nproc: {child_processes}").unwrap();
        writeln!(
            output,
            "rlimit_fsize: {}",
            file_size_bytes.div_ceil(1024 * 1024)
        )
        .unwrap();
        writeln!(output, "cgroup_mem_max: {address_space_bytes}").unwrap();
        writeln!(output, "cgroup_pids_max: {child_processes}").unwrap();
        writeln!(
            output,
            "cgroup_cpu_ms_per_sec: {}",
            self.cpu_millis_per_second
        )
        .unwrap();
        writeln!(
            output,
            "uidmap {{ inside_id: \"0\" outside_id: \"{}\" count: 1 }}",
            self.tenant.outside_uid
        )
        .unwrap();
        writeln!(
            output,
            "gidmap {{ inside_id: \"0\" outside_id: \"{}\" count: 1 }}",
            self.tenant.outside_gid
        )
        .unwrap();
        writeln!(
            output,
            "seccomp_policy_file: \"{}\"",
            escaped_path(&self.seccomp.path)
        )
        .unwrap();
        writeln!(output, "mount {{ dst: \"/\" fstype: \"tmpfs\" options: \"size=67108864\" rw: true mandatory: true nosuid: true nodev: true }}").unwrap();
        append_mount(&mut output, &self.tenant.work_dir, true);
        for path in &self.policy.filesystem.read_only {
            append_mount(&mut output, path, false);
        }
        for path in &self.policy.filesystem.read_write {
            append_mount(&mut output, path, true);
        }
        Ok(output)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NsJailAdapter {
    pub binary: PathBuf,
    pub binary_sha256: String,
    pub version: String,
}

impl NsJailAdapter {
    pub fn validate(&self) -> Result<()> {
        validate_absolute_path(&self.binary, "NsJail executable")?;
        validate_sha256(&self.binary_sha256, "NsJail executable")?;
        if self.version != "3.6" {
            return Err(anyhow!("public profile requires reviewed NsJail 3.6"));
        }
        Ok(())
    }

    pub fn command_line(
        &self,
        config_path: &Path,
        executable: &Path,
        arguments: &[String],
    ) -> Result<Vec<String>> {
        self.validate()?;
        validate_absolute_path(config_path, "NsJail config")?;
        validate_absolute_path(executable, "sandboxed executable")?;
        if arguments.iter().any(|value| value.contains('\0')) {
            return Err(anyhow!("sandbox argument contains NUL"));
        }
        Ok(std::iter::once(self.binary.to_string_lossy().into_owned())
            .chain([
                "--config".into(),
                config_path.to_string_lossy().into_owned(),
                "--".into(),
                executable.to_string_lossy().into_owned(),
            ])
            .chain(arguments.iter().cloned())
            .collect())
    }
}

fn validate_mounts(filesystem: &FilesystemAccess, work_dir: &Path) -> Result<()> {
    validate_absolute_path(work_dir, "tenant work directory")?;
    for path in filesystem.read_only.iter().chain(&filesystem.read_write) {
        validate_absolute_path(path, "sandbox mount")?;
        if path == Path::new("/") || path == Path::new("/home") || path == Path::new("/Users") {
            return Err(anyhow!("sandbox mount scope is too broad"));
        }
    }
    Ok(())
}

fn append_mount(output: &mut String, path: &Path, writable: bool) {
    let value = escaped_path(path);
    writeln!(output, "mount {{ src: \"{value}\" dst: \"{value}\" is_bind: true rw: {writable} mandatory: true nosuid: true nodev: true }}").unwrap();
}

fn validate_absolute_path(path: &Path, label: &str) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        || path
            .as_os_str()
            .to_string_lossy()
            .contains(['\n', '\r', '\0'])
    {
        return Err(anyhow!("{label} must be an absolute normalized path"));
    }
    Ok(())
}

fn escaped_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn is_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn validate_sha256(value: &str, label: &str) -> Result<()> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(anyhow!("{label} requires a sha256 checksum"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process_control::{EnvironmentAccess, OutputBudget};

    fn profile() -> NsJailProfile {
        NsJailProfile {
            tenant: TenantIdentity {
                tenant_id: "tenant-7".into(),
                outside_uid: 12007,
                outside_gid: 12007,
                work_dir: "/srv/video-kadr/tenants/tenant-7".into(),
            },
            seccomp: SeccompProfile {
                path: "/etc/video-kadr/seccomp/ffmpeg-v1.policy".into(),
                sha256: "a".repeat(64),
                tool_fingerprint: "ffmpeg-8.0-linux-amd64".into(),
            },
            policy: ProcessPolicy {
                role: ToolRole::Render,
                filesystem: FilesystemAccess::media(
                    vec!["/srv/video-kadr/tenants/tenant-7/sources/input.mp4".into()],
                    vec!["/srv/video-kadr/tenants/tenant-7/staging".into()],
                ),
                network: NetworkAccess::Denied,
                environment: EnvironmentAccess { inherit: vec![] },
                kernel: KernelLimits {
                    cpu_seconds: 120,
                    address_space_bytes: 512 * 1024 * 1024,
                    child_processes: 16,
                    open_files: 64,
                    file_size_bytes: 1024 * 1024 * 1024,
                },
                output: OutputBudget {
                    capture_bytes: 1024,
                    line_bytes: 256,
                },
            },
            cpu_millis_per_second: 700,
        }
    }

    #[test]
    fn profile_has_private_root_namespaces_cgroups_seccomp_and_exact_mounts() {
        let rendered = profile().render_config().unwrap();
        for expected in [
            "clone_newnet: true",
            "clone_newuser: true",
            "clone_newpid: true",
            "clone_newcgroup: true",
            "iface_no_lo: true",
            "detect_cgroupv2: true",
            "cgroup_mem_max: 536870912",
            "cgroup_pids_max: 16",
            "cgroup_cpu_ms_per_sec: 700",
            "seccomp_policy_file: \"/etc/video-kadr/seccomp/ffmpeg-v1.policy\"",
            "outside_id: \"12007\"",
        ] {
            assert!(rendered.contains(expected), "missing {expected}");
        }
        assert!(!rendered.contains("src: \"/home\""));
        assert!(!rendered.contains("src: \"/Users\""));
    }

    #[test]
    fn broad_mount_network_and_unversioned_seccomp_fail_closed() {
        let mut invalid = profile();
        invalid.policy.filesystem.read_only = vec!["/".into()];
        assert!(invalid.render_config().is_err());
        let mut invalid = profile();
        invalid.policy.network = NetworkAccess::PinnedLoopbackProxy("http://127.0.0.1:9".into());
        assert!(invalid.render_config().is_err());
        let mut invalid = profile();
        invalid.seccomp.sha256 = "latest".into();
        assert!(invalid.render_config().is_err());
    }

    #[test]
    fn wasm_and_native_capabilities_are_content_addressed_and_bounded() {
        SandboxCapability::WasmPlugin {
            module_sha256: "b".repeat(64),
            limits: WasmLimits {
                memory_bytes: 64 * 1024 * 1024,
                fuel: 10_000_000,
                epoch_deadline: 10,
                max_host_calls: 256,
            },
            readable_assets: vec!["source-1".into()],
        }
        .validate()
        .unwrap();
        SandboxCapability::NativeTool {
            role: ToolRole::Probe,
            executable_sha256: "c".repeat(64),
        }
        .validate()
        .unwrap();
        assert!(SandboxCapability::WasmPlugin {
            module_sha256: "b".repeat(64),
            limits: WasmLimits {
                memory_bytes: 0,
                fuel: 0,
                epoch_deadline: 0,
                max_host_calls: 0
            },
            readable_assets: vec![],
        }
        .validate()
        .is_err());
    }

    #[test]
    fn adapter_is_pinned_and_never_falls_back_to_direct_exec() {
        let adapter = NsJailAdapter {
            binary: "/usr/local/bin/nsjail".into(),
            binary_sha256: "d".repeat(64),
            version: "3.6".into(),
        };
        let command = adapter
            .command_line(
                Path::new("/run/video-kadr/jobs/j1.cfg"),
                Path::new("/usr/bin/ffmpeg"),
                &["-version".into()],
            )
            .unwrap();
        assert_eq!(command[0], "/usr/local/bin/nsjail");
        assert_eq!(
            &command[1..5],
            [
                "--config",
                "/run/video-kadr/jobs/j1.cfg",
                "--",
                "/usr/bin/ffmpeg"
            ]
        );
        let mut wrong = adapter;
        wrong.version = "3.5".into();
        assert!(wrong
            .command_line(Path::new("/run/a.cfg"), Path::new("/usr/bin/ffmpeg"), &[])
            .is_err());
    }
}
