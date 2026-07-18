//! One validated resource budget shared by render planning and CPU execution.

use std::fmt;
use std::fs;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeProfile {
    Interactive,
    Balanced,
    Quality,
}

impl EncodeProfile {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "interactive" | "low" => Ok(Self::Interactive),
            "balanced" | "default" => Ok(Self::Balanced),
            "quality" | "high" => Ok(Self::Quality),
            _ => Err(anyhow!(
                "ENCODE_PROFILE must be interactive, balanced, or quality"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TileLayout {
    pub columns: u8,
    pub rows: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EncodeBudget {
    pub threads: usize,
    pub tiles: TileLayout,
    /// Encoder speed tier, normalized to the rav1e 0..=10 scale.
    pub speed: u8,
    pub memory_mib: u64,
}

impl EncodeBudget {
    pub fn for_profile(profile: EncodeProfile, limits: RuntimeLimits) -> Result<Self> {
        limits.validate()?;
        let memory_limit = limits.memory_mib.unwrap_or(4096);
        let (threads, columns, speed, desired_memory) = match profile {
            EncodeProfile::Interactive => ((limits.logical_cpus / 2).clamp(1, 4), 1, 9, 512),
            EncodeProfile::Balanced => (limits.logical_cpus.min(8), 1, 6, 1024),
            EncodeProfile::Quality => (
                limits.logical_cpus,
                if limits.logical_cpus >= 4 { 2 } else { 1 },
                3,
                2048,
            ),
        };
        Self {
            threads,
            tiles: TileLayout { columns, rows: 1 },
            speed,
            memory_mib: desired_memory.min(memory_limit),
        }
        .validate(limits)
    }

    pub fn from_overrides(
        profile: EncodeProfile,
        limits: RuntimeLimits,
        mut lookup: impl FnMut(&str) -> Option<String>,
    ) -> Result<Self> {
        let mut budget = Self::for_profile(profile, limits)?;
        override_value(
            "ENCODE_THREADS",
            lookup("ENCODE_THREADS"),
            &mut budget.threads,
        )?;
        override_value(
            "ENCODE_TILE_COLUMNS",
            lookup("ENCODE_TILE_COLUMNS"),
            &mut budget.tiles.columns,
        )?;
        override_value(
            "ENCODE_TILE_ROWS",
            lookup("ENCODE_TILE_ROWS"),
            &mut budget.tiles.rows,
        )?;
        override_value("ENCODE_SPEED", lookup("ENCODE_SPEED"), &mut budget.speed)?;
        override_value(
            "ENCODE_MEMORY_MIB",
            lookup("ENCODE_MEMORY_MIB"),
            &mut budget.memory_mib,
        )?;
        budget.validate(limits)
    }

    pub fn validate(self, limits: RuntimeLimits) -> Result<Self> {
        limits.validate()?;
        if self.threads == 0 || self.threads > limits.logical_cpus {
            return Err(anyhow!(
                "encode threads must be within 1..={} for this runtime",
                limits.logical_cpus
            ));
        }
        if self.speed > 10 {
            return Err(anyhow!("encode speed must be within 0..=10"));
        }
        for (name, value) in [
            ("tile columns", self.tiles.columns),
            ("tile rows", self.tiles.rows),
        ] {
            if value == 0 || value > 16 || !value.is_power_of_two() {
                return Err(anyhow!("{name} must be a power of two within 1..=16"));
            }
        }
        let tile_workers = usize::from(self.tiles.columns) * usize::from(self.tiles.rows);
        if tile_workers > self.threads {
            return Err(anyhow!("tile workers cannot exceed encode threads"));
        }
        if self.memory_mib < 128 {
            return Err(anyhow!("encode memory budget must be at least 128 MiB"));
        }
        if let Some(limit) = limits.memory_mib {
            if self.memory_mib > limit {
                return Err(anyhow!(
                    "encode memory budget exceeds runtime limit of {limit} MiB"
                ));
            }
        }
        Ok(self)
    }

    /// FFmpeg options common to supported encoders. Codec-specific tile/speed
    /// mapping remains in the encoder adapter, where option semantics are known.
    pub fn ffmpeg_thread_args_for_job(&self, parallel_jobs: usize) -> [String; 4] {
        let per_job = (self.threads / parallel_jobs.max(1)).max(1);
        [
            "-filter_threads".into(),
            per_job.to_string(),
            "-threads:v".into(),
            per_job.to_string(),
        ]
    }

    /// FFmpeg's libsvtav1 adapter expects log2 tile counts, while config keeps
    /// human-readable, validated power-of-two counts.
    pub fn ffmpeg_av1_tile_args(&self) -> [String; 4] {
        [
            "-tile_columns".into(),
            self.tiles.columns.ilog2().to_string(),
            "-tile_rows".into(),
            self.tiles.rows.ilog2().to_string(),
        ]
    }
}

fn override_value<T>(name: &str, raw: Option<String>, target: &mut T) -> Result<()>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    if let Some(raw) = raw.filter(|value| !value.trim().is_empty()) {
        *target = raw.parse().with_context(|| format!("invalid {name}"))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeLimits {
    pub logical_cpus: usize,
    pub memory_mib: Option<u64>,
}

impl RuntimeLimits {
    pub fn detect() -> Self {
        let host_cpus = std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1);
        let host_memory = host_memory_mib();
        let cgroup_cpus = cgroup_cpu_limit();
        let cgroup_memory = cgroup_memory_limit_mib();
        Self {
            logical_cpus: cgroup_cpus
                .map_or(host_cpus, |value| value.min(host_cpus))
                .max(1),
            memory_mib: minimum_optional(host_memory, cgroup_memory),
        }
    }

    fn validate(self) -> Result<Self> {
        if self.logical_cpus == 0 {
            return Err(anyhow!("runtime must expose at least one logical CPU"));
        }
        if self.memory_mib == Some(0) {
            return Err(anyhow!("runtime memory limit must be positive"));
        }
        Ok(self)
    }
}

fn minimum_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

#[cfg(unix)]
fn host_memory_mib() -> Option<u64> {
    let pages = unsafe { libc::sysconf(libc::_SC_PHYS_PAGES) };
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if pages <= 0 || page_size <= 0 {
        return None;
    }
    u64::try_from(pages)
        .ok()?
        .checked_mul(u64::try_from(page_size).ok()?)?
        .checked_div(1024 * 1024)
}

#[cfg(not(unix))]
fn host_memory_mib() -> Option<u64> {
    None
}

fn cgroup_cpu_limit() -> Option<usize> {
    if let Ok(value) = fs::read_to_string("/sys/fs/cgroup/cpu.max") {
        return parse_cgroup_cpu_max(&value);
    }
    let quota = fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_quota_us")
        .ok()?
        .trim()
        .parse::<i64>()
        .ok()?;
    let period = fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_period_us")
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    if quota <= 0 || period == 0 {
        return None;
    }
    usize::try_from((quota as u64).div_ceil(period).max(1)).ok()
}

fn parse_cgroup_cpu_max(value: &str) -> Option<usize> {
    let mut fields = value.split_whitespace();
    let quota = fields.next()?;
    if quota == "max" {
        return None;
    }
    let quota: u64 = quota.parse().ok()?;
    let period: u64 = fields.next()?.parse().ok()?;
    if period == 0 {
        return None;
    }
    usize::try_from(quota.div_ceil(period).max(1)).ok()
}

fn cgroup_memory_limit_mib() -> Option<u64> {
    let value = fs::read_to_string("/sys/fs/cgroup/memory.max")
        .or_else(|_| fs::read_to_string("/sys/fs/cgroup/memory/memory.limit_in_bytes"))
        .ok()?;
    parse_cgroup_memory_max(&value)
}

fn parse_cgroup_memory_max(value: &str) -> Option<u64> {
    let value = value.trim();
    if value == "max" {
        return None;
    }
    let bytes = value.parse::<u64>().ok()?;
    // cgroup v1 uses a near-u64-max sentinel when no limit is configured.
    if bytes >= (1_u64 << 60) {
        return None;
    }
    bytes.checked_div(1024 * 1024)
}

impl fmt::Display for EncodeProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Interactive => "interactive",
            Self::Balanced => "balanced",
            Self::Quality => "quality",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> RuntimeLimits {
        RuntimeLimits {
            logical_cpus: 8,
            memory_mib: Some(4096),
        }
    }

    #[test]
    fn profiles_trade_latency_resources_and_quality_explicitly() {
        let interactive = EncodeBudget::for_profile(EncodeProfile::Interactive, limits()).unwrap();
        let balanced = EncodeBudget::for_profile(EncodeProfile::Balanced, limits()).unwrap();
        let quality = EncodeBudget::for_profile(EncodeProfile::Quality, limits()).unwrap();
        assert!(interactive.threads <= balanced.threads);
        assert!(balanced.threads <= quality.threads);
        assert!(interactive.speed > balanced.speed);
        assert!(balanced.speed > quality.speed);
        assert!(interactive.memory_mib <= quality.memory_mib);
    }

    #[test]
    fn invalid_budget_cannot_oversubscribe_runtime() {
        let budget = EncodeBudget {
            threads: 9,
            tiles: TileLayout {
                columns: 1,
                rows: 1,
            },
            speed: 6,
            memory_mib: 1024,
        };
        assert!(budget.validate(limits()).is_err());
        let budget = EncodeBudget {
            threads: 2,
            tiles: TileLayout {
                columns: 4,
                rows: 1,
            },
            speed: 6,
            memory_mib: 1024,
        };
        assert!(budget.validate(limits()).is_err());
    }

    #[test]
    fn cgroup_limits_are_parsed_without_treating_max_as_a_number() {
        assert_eq!(parse_cgroup_cpu_max("200000 100000"), Some(2));
        assert_eq!(parse_cgroup_cpu_max("150000 100000"), Some(2));
        assert_eq!(parse_cgroup_cpu_max("max 100000"), None);
        assert_eq!(parse_cgroup_memory_max("1073741824"), Some(1024));
        assert_eq!(parse_cgroup_memory_max("max"), None);
    }
}
