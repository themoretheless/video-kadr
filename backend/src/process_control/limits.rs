#[cfg(not(unix))]
use anyhow::anyhow;
use anyhow::Result;
use tokio::process::Command;

use super::KernelLimits;

#[cfg(unix)]
pub(super) fn configure_process_group(command: &mut Command) {
    command.process_group(0);
}

#[cfg(not(unix))]
pub(super) fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
pub(super) fn configure_kernel_limits(command: &mut Command, limits: KernelLimits) -> Result<()> {
    limits.validate()?;
    // SAFETY: the closure calls only `setrlimit`, does not allocate, and reports
    // errors through `io::Error`. Values are validated before fork.
    unsafe {
        command.pre_exec(move || apply_kernel_limits(limits));
    }
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn configure_kernel_limits(_command: &mut Command, _limits: KernelLimits) -> Result<()> {
    Err(anyhow!(
        "hard external process limits are unavailable on this platform"
    ))
}

#[cfg(unix)]
fn apply_kernel_limits(limits: KernelLimits) -> std::io::Result<()> {
    macro_rules! set_limit {
        ($resource:expr, $value:expr) => {{
            let value = libc::rlim_t::try_from($value).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "rlimit value overflow")
            })?;
            let limit = libc::rlimit {
                rlim_cur: value,
                rlim_max: value,
            };
            if unsafe { libc::setrlimit($resource, &limit) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }};
    }

    set_limit!(libc::RLIMIT_CPU, limits.cpu_seconds);
    #[cfg(any(target_os = "linux", target_os = "android"))]
    set_limit!(libc::RLIMIT_AS, limits.address_space_bytes);
    set_limit!(libc::RLIMIT_NOFILE, limits.open_files);
    set_limit!(libc::RLIMIT_FSIZE, limits.file_size_bytes);
    Ok(())
}
