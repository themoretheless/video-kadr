//! Validated process configuration. Environment access ends at this module.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use crate::jobs::QueueLimits;
use crate::process_control::{
    IsolationTier, KernelLimits, OutputBudget, ProcessRuntime, ProcessRuntimeConfig, SandboxBackend,
};
use crate::youtube::{validate_redirect_uri, YouTubeOAuthConfig};
use encode_budget::{EncodeBudget, EncodeProfile, RuntimeLimits};
use resource_classes::ResourceClassLimits;

pub mod encode_budget;
pub mod resource_classes;

const LOCAL_CORS_ORIGINS: [&str; 4] = [
    "http://localhost:5173",
    "http://127.0.0.1:5173",
    "http://localhost:8088",
    "http://127.0.0.1:8088",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorsOrigins(Vec<String>);

impl CorsOrigins {
    fn parse(configured: Option<String>) -> Result<Self> {
        let raw_origins: Vec<&str> = match configured.as_deref() {
            Some(raw) => {
                if raw.trim().is_empty() {
                    return Err(anyhow!(
                        "CORS_ALLOW_ORIGINS must contain at least one origin"
                    ));
                }
                raw.split(',')
                    .map(str::trim)
                    .filter(|origin| !origin.is_empty())
                    .collect()
            }
            None => LOCAL_CORS_ORIGINS.to_vec(),
        };

        let mut origins = Vec::with_capacity(raw_origins.len());
        for origin in raw_origins {
            let normalized = normalize_cors_origin(origin)?;
            if !origins.contains(&normalized) {
                origins.push(normalized);
            }
        }
        if origins.is_empty() {
            return Err(anyhow!(
                "CORS_ALLOW_ORIGINS must contain at least one origin"
            ));
        }
        Ok(Self(origins))
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

impl Default for CorsOrigins {
    fn default() -> Self {
        Self(
            LOCAL_CORS_ORIGINS
                .iter()
                .map(|origin| (*origin).to_owned())
                .collect(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkloadConfig {
    pub job_timeout: Duration,
    pub recover_jobs_limit: i64,
    pub queue_limits: QueueLimits,
    pub max_download_height: u32,
}

impl Default for WorkloadConfig {
    fn default() -> Self {
        Self {
            job_timeout: Duration::from_secs(30 * 60),
            recover_jobs_limit: 200,
            queue_limits: QueueLimits::default(),
            max_download_height: 720,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub storage: PathBuf,
    pub object_store_url: Option<String>,
    pub pexels_api_key: Option<String>,
    pub youtube_oauth: Option<YouTubeOAuthConfig>,
    pub youtube_token_key: Option<[u8; 32]>,
    pub max_concurrent_jobs: usize,
    pub resource_classes: ResourceClassLimits,
    pub max_upload_bytes: usize,
    pub file_ttl_hours: u64,
    pub bind_addr: IpAddr,
    pub port: u16,
    pub cpu_queue_capacity: usize,
    pub encode_budget: EncodeBudget,
    pub process_runtime: ProcessRuntimeConfig,
    pub cors_origins: CorsOrigins,
    pub workload: WorkloadConfig,
    pub console: TelemetryConsoleConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryConsoleConfig {
    pub enabled: bool,
    pub environment: String,
    pub bind: SocketAddr,
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
        let object_store_url = lookup("OBJECT_STORE_URL")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        if let Some(raw_url) = object_store_url.as_deref() {
            let url = url::Url::parse(raw_url).context("OBJECT_STORE_URL must be a valid URL")?;
            if url.scheme() != "s3" || url.host_str().is_none() {
                return Err(anyhow!("OBJECT_STORE_URL must use s3://bucket[/prefix]"));
            }
        }
        let pexels_api_key = lookup("PEXELS_API_KEY")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let youtube_values = [
            lookup("YOUTUBE_CLIENT_ID").filter(|value| !value.trim().is_empty()),
            lookup("YOUTUBE_CLIENT_SECRET").filter(|value| !value.trim().is_empty()),
            lookup("YOUTUBE_REDIRECT_URI").filter(|value| !value.trim().is_empty()),
            lookup("YOUTUBE_TOKEN_KEY").filter(|value| !value.trim().is_empty()),
        ];
        let (youtube_oauth, youtube_token_key) = match youtube_values {
            [None, None, None, None] => (None, None),
            [Some(client_id), Some(client_secret), Some(redirect_uri), Some(token_key)] => {
                validate_redirect_uri(redirect_uri.trim())?;
                (
                    Some(YouTubeOAuthConfig {
                        client_id: client_id.trim().into(),
                        client_secret: client_secret.trim().into(),
                        redirect_uri: redirect_uri.trim().into(),
                    }),
                    Some(parse_hex_key("YOUTUBE_TOKEN_KEY", token_key.trim())?),
                )
            }
            _ => return Err(anyhow!("YOUTUBE_CLIENT_ID, YOUTUBE_CLIENT_SECRET, YOUTUBE_REDIRECT_URI and YOUTUBE_TOKEN_KEY must be configured together")),
        };
        let max_concurrent_jobs =
            positive_usize("MAX_CONCURRENT_JOBS", lookup("MAX_CONCURRENT_JOBS"), 2)?;
        let resource_classes = ResourceClassLimits {
            ingest: positive_usize(
                "INGEST_CONCURRENCY",
                lookup("INGEST_CONCURRENCY"),
                max_concurrent_jobs,
            )?,
            analysis: positive_usize(
                "ANALYSIS_CONCURRENCY",
                lookup("ANALYSIS_CONCURRENCY"),
                max_concurrent_jobs,
            )?,
            export: positive_usize(
                "EXPORT_CONCURRENCY",
                lookup("EXPORT_CONCURRENCY"),
                max_concurrent_jobs,
            )?,
        };
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
        let workload_defaults = WorkloadConfig::default();
        let workload = WorkloadConfig {
            job_timeout: Duration::from_secs(positive_u64(
                "JOB_TIMEOUT_SECS",
                lookup("JOB_TIMEOUT_SECS"),
                workload_defaults.job_timeout.as_secs(),
            )?),
            recover_jobs_limit: positive_i64(
                "RECOVER_JOBS_LIMIT",
                lookup("RECOVER_JOBS_LIMIT"),
                workload_defaults.recover_jobs_limit,
            )?,
            queue_limits: QueueLimits {
                dedupe_ttl: Duration::from_secs(positive_u64(
                    "JOB_DEDUPE_TTL_SECS",
                    lookup("JOB_DEDUPE_TTL_SECS"),
                    workload_defaults.queue_limits.dedupe_ttl.as_secs(),
                )?),
                rate_window: Duration::from_secs(positive_u64(
                    "JOB_RATE_WINDOW_SECS",
                    lookup("JOB_RATE_WINDOW_SECS"),
                    workload_defaults.queue_limits.rate_window.as_secs(),
                )?),
                max_new_jobs: positive_u32(
                    "JOB_RATE_LIMIT",
                    lookup("JOB_RATE_LIMIT"),
                    workload_defaults.queue_limits.max_new_jobs,
                )?,
            },
            max_download_height: positive_u32(
                "MAX_HEIGHT",
                lookup("MAX_HEIGHT"),
                workload_defaults.max_download_height,
            )?,
        };
        let cors_origins = CorsOrigins::parse(lookup("CORS_ALLOW_ORIGINS"))?;
        let profile = lookup("ENCODE_PROFILE")
            .as_deref()
            .map(EncodeProfile::parse)
            .transpose()?
            .unwrap_or(EncodeProfile::Balanced);
        let encode_budget = EncodeBudget::from_overrides(profile, limits, |name| lookup(name))?;
        let isolation_tier = lookup("ISOLATION_TIER")
            .as_deref()
            .map(IsolationTier::parse)
            .transpose()?
            .unwrap_or(IsolationTier::Local);
        let sandbox = lookup("PROCESS_SANDBOX")
            .as_deref()
            .map(SandboxBackend::parse)
            .transpose()?
            .unwrap_or(SandboxBackend::None);
        let default_memory_mib = limits
            .memory_mib
            .map_or(2048, |limit| limit.min(2048))
            .max(128);
        let process_memory_mib = positive_u64(
            "PROCESS_MAX_MEMORY_MIB",
            lookup("PROCESS_MAX_MEMORY_MIB"),
            default_memory_mib,
        )?;
        if limits
            .memory_mib
            .is_some_and(|limit| process_memory_mib > limit)
        {
            return Err(anyhow!(
                "PROCESS_MAX_MEMORY_MIB exceeds the detected runtime memory limit"
            ));
        }
        let default_file_bytes = u64::try_from(max_upload_bytes).unwrap_or(u64::MAX);
        let process_runtime = ProcessRuntimeConfig {
            tier: isolation_tier,
            sandbox,
            kernel: KernelLimits {
                cpu_seconds: positive_u64(
                    "PROCESS_MAX_CPU_SECONDS",
                    lookup("PROCESS_MAX_CPU_SECONDS"),
                    2 * 60 * 60,
                )?,
                address_space_bytes: process_memory_mib
                    .checked_mul(1024 * 1024)
                    .ok_or_else(|| anyhow!("PROCESS_MAX_MEMORY_MIB is too large"))?,
                child_processes: positive_u64(
                    "PROCESS_MAX_CHILDREN",
                    lookup("PROCESS_MAX_CHILDREN"),
                    64,
                )?,
                open_files: positive_u64(
                    "PROCESS_MAX_OPEN_FILES",
                    lookup("PROCESS_MAX_OPEN_FILES"),
                    256,
                )?,
                file_size_bytes: positive_u64(
                    "PROCESS_MAX_FILE_BYTES",
                    lookup("PROCESS_MAX_FILE_BYTES"),
                    default_file_bytes,
                )?,
            },
            output: OutputBudget {
                capture_bytes: positive_usize(
                    "PROCESS_MAX_CAPTURE_BYTES",
                    lookup("PROCESS_MAX_CAPTURE_BYTES"),
                    2 * 1024 * 1024,
                )?,
                line_bytes: positive_usize(
                    "PROCESS_MAX_LINE_BYTES",
                    lookup("PROCESS_MAX_LINE_BYTES"),
                    64 * 1024,
                )?,
            },
        };
        ProcessRuntime::new(process_runtime.clone())?.validate_deployment(bind_addr)?;
        let console = TelemetryConsoleConfig {
            enabled: parse_or(
                "ENABLE_TOKIO_CONSOLE",
                lookup("ENABLE_TOKIO_CONSOLE"),
                false,
            )?,
            environment: lookup("DEPLOY_ENV").unwrap_or_else(|| "local".into()),
            bind: lookup("TOKIO_CONSOLE_BIND")
                .unwrap_or_else(|| "127.0.0.1:6669".into())
                .parse()
                .context("TOKIO_CONSOLE_BIND must be a socket address")?,
        };
        if console.enabled && (console.environment != "staging" || !console.bind.ip().is_loopback())
        {
            return Err(anyhow!(
                "tokio-console is allowed only in staging on a loopback address"
            ));
        }

        Ok(Self {
            storage,
            object_store_url,
            pexels_api_key,
            youtube_oauth,
            youtube_token_key,
            max_concurrent_jobs,
            resource_classes,
            max_upload_bytes,
            file_ttl_hours,
            bind_addr,
            port,
            cpu_queue_capacity,
            encode_budget,
            process_runtime,
            cors_origins,
            workload,
            console,
        })
    }
}

fn parse_hex_key(name: &str, value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(anyhow!(
            "{name} must contain exactly 64 hexadecimal characters"
        ));
    }
    let mut key = [0_u8; 32];
    for (index, slot) in key.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .with_context(|| format!("parse {name}"))?;
    }
    Ok(key)
}

fn normalize_cors_origin(origin: &str) -> Result<String> {
    if origin == "*" {
        return Err(anyhow!(
            "CORS_ALLOW_ORIGINS does not accept wildcard origins"
        ));
    }
    let url = url::Url::parse(origin).context("invalid CORS_ALLOW_ORIGINS entry")?;
    let valid_scheme = matches!(url.scheme(), "http" | "https");
    let plain_origin = url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none();
    if !valid_scheme || url.host_str().is_none() || !plain_origin {
        return Err(anyhow!(
            "CORS_ALLOW_ORIGINS entries must be plain http(s) origins"
        ));
    }
    Ok(url.origin().ascii_serialization())
}

fn positive_usize(name: &str, value: Option<String>, default: usize) -> Result<usize> {
    let parsed = parse_or(name, value, default)?;
    if parsed == 0 {
        return Err(anyhow!("{name} must be greater than zero"));
    }
    Ok(parsed)
}

fn positive_u64(name: &str, value: Option<String>, default: u64) -> Result<u64> {
    let parsed = parse_or(name, value, default)?;
    if parsed == 0 {
        return Err(anyhow!("{name} must be greater than zero"));
    }
    Ok(parsed)
}

fn positive_i64(name: &str, value: Option<String>, default: i64) -> Result<i64> {
    let parsed = parse_or(name, value, default)?;
    if parsed <= 0 {
        return Err(anyhow!("{name} must be greater than zero"));
    }
    Ok(parsed)
}

fn positive_u32(name: &str, value: Option<String>, default: u32) -> Result<u32> {
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
        assert_eq!(value.object_store_url, None);
        assert_eq!(value.pexels_api_key, None);
        assert_eq!(value.youtube_oauth, None);
        assert_eq!(value.youtube_token_key, None);
        assert_eq!(value.bind_addr, IpAddr::from([127, 0, 0, 1]));
        assert_eq!(value.port, 8080);
        assert!(value.encode_budget.threads <= 8);
        assert!(value.encode_budget.memory_mib <= 4096);
        assert_eq!(value.process_runtime.tier, IsolationTier::Local);
        assert_eq!(value.process_runtime.sandbox, SandboxBackend::None);
        assert_eq!(value.resource_classes, ResourceClassLimits::balanced(2));
        assert!(!value.console.enabled);
        assert_eq!(value.workload, WorkloadConfig::default());
        assert_eq!(
            value.cors_origins.iter().collect::<Vec<_>>(),
            LOCAL_CORS_ORIGINS
        );
    }

    #[test]
    fn youtube_oauth_is_all_or_none_and_requires_safe_redirect() {
        assert!(config(&[("YOUTUBE_CLIENT_ID", "id")]).is_err());
        assert!(config(&[
            ("YOUTUBE_CLIENT_ID", "id"),
            ("YOUTUBE_CLIENT_SECRET", "secret"),
            ("YOUTUBE_REDIRECT_URI", "http://example.test/callback"),
            (
                "YOUTUBE_TOKEN_KEY",
                "0000000000000000000000000000000000000000000000000000000000000000"
            ),
        ])
        .is_err());
        let value = config(&[
            ("YOUTUBE_CLIENT_ID", "id"),
            ("YOUTUBE_CLIENT_SECRET", "secret"),
            (
                "YOUTUBE_REDIRECT_URI",
                "http://127.0.0.1:8080/api/publish/youtube/callback",
            ),
            (
                "YOUTUBE_TOKEN_KEY",
                "0101010101010101010101010101010101010101010101010101010101010101",
            ),
        ])
        .unwrap();
        assert_eq!(value.youtube_oauth.unwrap().client_id, "id");
        assert_eq!(value.youtube_token_key, Some([1; 32]));
        assert!(config(&[
            ("YOUTUBE_CLIENT_ID", "id"),
            ("YOUTUBE_CLIENT_SECRET", "secret"),
            ("YOUTUBE_REDIRECT_URI", "https://example.test/callback"),
            ("YOUTUBE_TOKEN_KEY", "short"),
        ])
        .is_err());
    }

    #[test]
    fn validates_optional_s3_object_store_url() {
        let value = config(&[("OBJECT_STORE_URL", "s3://media-bucket/video-kadr")]).unwrap();
        assert_eq!(
            value.object_store_url.as_deref(),
            Some("s3://media-bucket/video-kadr")
        );
        assert!(config(&[("OBJECT_STORE_URL", "https://media.example.test")]).is_err());
        assert!(config(&[("OBJECT_STORE_URL", "s3:///missing-bucket")]).is_err());
    }

    #[test]
    fn overrides_are_validated_together() {
        let value = config(&[
            ("BIND_ADDR", "0.0.0.0"),
            ("ISOLATION_TIER", "lan"),
            ("PORT", "9000"),
            ("MAX_CONCURRENT_JOBS", "4"),
            ("INGEST_CONCURRENCY", "2"),
            ("ANALYSIS_CONCURRENCY", "1"),
            ("EXPORT_CONCURRENCY", "3"),
            ("ENCODE_PROFILE", "quality"),
            ("ENCODE_THREADS", "6"),
            ("ENCODE_MEMORY_MIB", "2048"),
            ("PROCESS_MAX_CPU_SECONDS", "600"),
            ("PROCESS_MAX_OPEN_FILES", "128"),
            ("JOB_TIMEOUT_SECS", "90"),
            ("RECOVER_JOBS_LIMIT", "50"),
            ("JOB_DEDUPE_TTL_SECS", "30"),
            ("JOB_RATE_WINDOW_SECS", "10"),
            ("JOB_RATE_LIMIT", "7"),
            ("MAX_HEIGHT", "1080"),
            (
                "CORS_ALLOW_ORIGINS",
                "https://app.example/, https://app.example, http://localhost:3000",
            ),
        ])
        .unwrap();
        assert_eq!(value.bind_addr, IpAddr::from([0, 0, 0, 0]));
        assert_eq!(value.port, 9000);
        assert_eq!(value.max_concurrent_jobs, 4);
        assert_eq!(value.resource_classes.ingest, 2);
        assert_eq!(value.resource_classes.analysis, 1);
        assert_eq!(value.resource_classes.export, 3);
        assert_eq!(value.encode_budget.threads, 6);
        assert_eq!(value.encode_budget.memory_mib, 2048);
        assert_eq!(value.process_runtime.tier, IsolationTier::Lan);
        assert_eq!(value.process_runtime.kernel.cpu_seconds, 600);
        assert_eq!(value.process_runtime.kernel.open_files, 128);
        assert_eq!(value.workload.job_timeout, Duration::from_secs(90));
        assert_eq!(value.workload.recover_jobs_limit, 50);
        assert_eq!(
            value.workload.queue_limits,
            QueueLimits {
                dedupe_ttl: Duration::from_secs(30),
                rate_window: Duration::from_secs(10),
                max_new_jobs: 7,
            }
        );
        assert_eq!(value.workload.max_download_height, 1080);
        assert_eq!(
            value.cors_origins.iter().collect::<Vec<_>>(),
            vec!["https://app.example", "http://localhost:3000"]
        );
    }

    #[test]
    fn invalid_values_fail_startup_instead_of_silently_defaulting() {
        assert!(config(&[("PORT", "not-a-port")]).is_err());
        assert!(config(&[("MAX_CONCURRENT_JOBS", "0")]).is_err());
        assert!(config(&[("INGEST_CONCURRENCY", "0")]).is_err());
        assert!(config(&[("ENCODE_THREADS", "99")]).is_err());
        assert!(config(&[("ENCODE_MEMORY_MIB", "8192")]).is_err());
        assert!(config(&[("BIND_ADDR", "0.0.0.0")]).is_err());
        assert!(config(&[("ISOLATION_TIER", "public")]).is_err());
        assert!(config(&[("PROCESS_SANDBOX", "nsjail")]).is_err());
        assert!(config(&[("PROCESS_MAX_MEMORY_MIB", "8192")]).is_err());
        assert!(config(&[("PROCESS_MAX_OPEN_FILES", "8")]).is_err());
        assert!(config(&[("JOB_TIMEOUT_SECS", "0")]).is_err());
        assert!(config(&[("RECOVER_JOBS_LIMIT", "-1")]).is_err());
        assert!(config(&[("JOB_DEDUPE_TTL_SECS", "bad")]).is_err());
        assert!(config(&[("JOB_RATE_WINDOW_SECS", "0")]).is_err());
        assert!(config(&[("JOB_RATE_LIMIT", "0")]).is_err());
        assert!(config(&[("MAX_HEIGHT", "0")]).is_err());
        assert!(config(&[("CORS_ALLOW_ORIGINS", "")]).is_err());
        assert!(config(&[("CORS_ALLOW_ORIGINS", " , ")]).is_err());
        assert!(config(&[("CORS_ALLOW_ORIGINS", "*")]).is_err());
        assert!(config(&[("CORS_ALLOW_ORIGINS", "https://app.example/path")]).is_err());
        assert!(config(&[("CORS_ALLOW_ORIGINS", "https://app.example?token=secret")]).is_err());
        assert!(config(&[("ENABLE_TOKIO_CONSOLE", "true")]).is_err());
        assert!(config(&[
            ("ENABLE_TOKIO_CONSOLE", "true"),
            ("DEPLOY_ENV", "staging"),
            ("TOKIO_CONSOLE_BIND", "0.0.0.0:6669")
        ])
        .is_err());
        assert!(config(&[
            ("PROCESS_MAX_CAPTURE_BYTES", "1024"),
            ("PROCESS_MAX_LINE_BYTES", "2048")
        ])
        .is_err());
    }
}
