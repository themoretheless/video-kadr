//! Validated process configuration. Environment access ends at this module.

use std::net::IpAddr;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};

use encode_budget::{EncodeBudget, EncodeProfile, RuntimeLimits};

pub mod encode_budget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub storage: PathBuf,
    pub max_concurrent_jobs: usize,
    pub max_upload_bytes: usize,
    pub file_ttl_hours: u64,
    pub bind_addr: IpAddr,
    pub port: u16,
    pub cpu_queue_capacity: usize,
    pub encode_budget: EncodeBudget,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|name| std::env::var(name).ok(), RuntimeLimits::detect())
    }

    fn from_lookup(
        mut lookup: impl FnMut(&str) -> Option<String>,
        limits: RuntimeLimits,
    ) -> Result<Self> {
        let storage = lookup("STORAGE_DIR")
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("storage"));
        let max_concurrent_jobs =
            positive_usize("MAX_CONCURRENT_JOBS", lookup("MAX_CONCURRENT_JOBS"), 2)?;
        let max_upload_bytes = positive_usize(
            "MAX_UPLOAD_BYTES",
            lookup("MAX_UPLOAD_BYTES"),
            2 * 1024 * 1024 * 1024,
        )?;
        let file_ttl_hours = parse_or("FILE_TTL_HOURS", lookup("FILE_TTL_HOURS"), 0_u64)?;
        let bind_addr = lookup("BIND_ADDR")
            .unwrap_or_else(|| "127.0.0.1".into())
            .parse()
            .context("BIND_ADDR must be an IP address")?;
        let port = parse_or("PORT", lookup("PORT"), 8080_u16)?;
        let cpu_queue_capacity = parse_or(
            "CPU_QUEUE_CAPACITY",
            lookup("CPU_QUEUE_CAPACITY"),
            limits.logical_cpus.saturating_mul(2).max(2),
        )?;
        let profile = lookup("ENCODE_PROFILE")
            .as_deref()
            .map(EncodeProfile::parse)
            .transpose()?
            .unwrap_or(EncodeProfile::Balanced);
        let encode_budget = EncodeBudget::from_overrides(profile, limits, |name| lookup(name))?;

        Ok(Self {
            storage,
            max_concurrent_jobs,
            max_upload_bytes,
            file_ttl_hours,
            bind_addr,
            port,
            cpu_queue_capacity,
            encode_budget,
        })
    }
}

fn positive_usize(name: &str, value: Option<String>, default: usize) -> Result<usize> {
    let parsed = parse_or(name, value, default)?;
    if parsed == 0 {
        return Err(anyhow!("{name} must be greater than zero"));
    }
    Ok(parsed)
}

fn parse_or<T>(name: &str, value: Option<String>, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.parse().with_context(|| format!("invalid {name}")))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn config(values: &[(&str, &str)]) -> Result<AppConfig> {
        let values: BTreeMap<_, _> = values
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        AppConfig::from_lookup(
            |name| values.get(name).cloned(),
            RuntimeLimits {
                logical_cpus: 8,
                memory_mib: Some(4096),
            },
        )
    }

    #[test]
    fn defaults_are_local_and_resource_bounded() {
        let value = config(&[]).unwrap();
        assert_eq!(value.storage, PathBuf::from("storage"));
        assert_eq!(value.bind_addr, IpAddr::from([127, 0, 0, 1]));
        assert_eq!(value.port, 8080);
        assert!(value.encode_budget.threads <= 8);
        assert!(value.encode_budget.memory_mib <= 4096);
    }

    #[test]
    fn overrides_are_validated_together() {
        let value = config(&[
            ("BIND_ADDR", "0.0.0.0"),
            ("PORT", "9000"),
            ("MAX_CONCURRENT_JOBS", "4"),
            ("ENCODE_PROFILE", "quality"),
            ("ENCODE_THREADS", "6"),
            ("ENCODE_MEMORY_MIB", "2048"),
        ])
        .unwrap();
        assert_eq!(value.bind_addr, IpAddr::from([0, 0, 0, 0]));
        assert_eq!(value.port, 9000);
        assert_eq!(value.max_concurrent_jobs, 4);
        assert_eq!(value.encode_budget.threads, 6);
        assert_eq!(value.encode_budget.memory_mib, 2048);
    }

    #[test]
    fn invalid_values_fail_startup_instead_of_silently_defaulting() {
        assert!(config(&[("PORT", "not-a-port")]).is_err());
        assert!(config(&[("MAX_CONCURRENT_JOBS", "0")]).is_err());
        assert!(config(&[("ENCODE_THREADS", "99")]).is_err());
        assert!(config(&[("ENCODE_MEMORY_MIB", "8192")]).is_err());
    }
}
