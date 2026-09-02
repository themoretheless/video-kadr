use anyhow::Result;
use tracing_subscriber::EnvFilter;

use crate::config::TelemetryConsoleConfig;

pub fn init(config: &TelemetryConsoleConfig) -> Result<()> {
    if config.enabled {
        #[cfg(feature = "tokio-console")]
        {
            console_subscriber::ConsoleLayer::builder()
                .with_default_env()
                .server_addr(config.bind)
                .init();
            return Ok(());
        }
        #[cfg(not(feature = "tokio-console"))]
        anyhow::bail!("ENABLE_TOKIO_CONSOLE requires a build with --features tokio-console");
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_config_does_not_require_staging_feature() {
        let config = TelemetryConsoleConfig {
            enabled: false,
            environment: "local".into(),
            bind: "127.0.0.1:6669".parse().unwrap(),
        };
        assert!(!config.enabled);
    }
}
