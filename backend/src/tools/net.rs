//! URL validation / SSRF guard for imports.

use std::net::IpAddr;
use std::{future::Future, io};

use anyhow::{anyhow, Result};
use url::Url;

/// Reject non-http(s) URLs and ones that point at the local machine / private
/// network (a basic SSRF guard). Returns a Russian error message on rejection.
pub async fn validate_url(raw: &str) -> Result<()> {
    validate_url_with_resolver(raw, |host, port| async move {
        let addrs = tokio::net::lookup_host((host.as_str(), port)).await?;
        Ok(addrs.map(|addr| addr.ip()).collect())
    })
    .await
}

async fn validate_url_with_resolver<F, Fut>(raw: &str, resolve: F) -> Result<()>
where
    F: FnOnce(String, u16) -> Fut,
    Fut: Future<Output = io::Result<Vec<IpAddr>>>,
{
    let u = Url::parse(raw).map_err(|_| anyhow!("Недопустимый URL"))?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err(anyhow!("Недопустимый URL"));
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err(anyhow!("Недопустимый URL"));
    }
    let host = u.host_str().ok_or_else(|| anyhow!("Недопустимый URL"))?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".local") || lower.ends_with(".localhost") {
        return Err(anyhow!("Недопустимый URL"));
    }
    if let Ok(ip) = lower.parse::<IpAddr>() {
        if is_blocked_ip(&ip) {
            return Err(anyhow!("Недопустимый URL"));
        }
        return Ok(());
    }
    if looks_like_noncanonical_ip(&lower) {
        return Err(anyhow!("Недопустимый URL"));
    }
    let port = u
        .port_or_known_default()
        .ok_or_else(|| anyhow!("Недопустимый URL"))?;
    let ips = resolve(lower, port)
        .await
        .map_err(|_| anyhow!("Недопустимый URL"))?;
    if ips.is_empty() || ips.iter().any(is_blocked_ip) {
        return Err(anyhow!("Недопустимый URL"));
    }
    Ok(())
}

fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_multicast()
                || octets[0] == 0
                || octets[0] >= 240
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
                || (octets[0] == 198 && (18..=19).contains(&octets[1]))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_blocked_ip(&IpAddr::V4(v4));
            }
            if v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() {
                return true;
            }
            let seg = v6.segments();
            // fc00::/7 (unique local), fe80::/10 (link local), fec0::/10
            // (deprecated site local).
            (seg[0] & 0xfe00) == 0xfc00
                || (seg[0] & 0xffc0) == 0xfe80
                || (seg[0] & 0xffc0) == 0xfec0
        }
    }
}

fn looks_like_noncanonical_ip(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    labels.iter().all(|label| {
        !label.is_empty()
            && (label.chars().all(|c| c.is_ascii_digit())
                || label.strip_prefix("0x").is_some_and(|rest| {
                    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_hexdigit())
                }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn validate_with_ips(raw: &str, ips: Vec<IpAddr>) -> Result<()> {
        validate_url_with_resolver(raw, |_host, _port| async move { Ok(ips) }).await
    }

    #[tokio::test]
    async fn validate_url_blocks_local_and_bad_schemes() {
        assert!(validate_with_ips(
            "https://example.com/v",
            vec!["93.184.216.34".parse().unwrap()]
        )
        .await
        .is_ok());
        assert!(validate_url("http://1.2.3.4/v").await.is_ok());
        assert!(validate_url("ftp://example.com/v").await.is_err());
        assert!(validate_url("https://localhost/v").await.is_err());
        assert!(validate_url("http://127.0.0.1/v").await.is_err());
        assert!(validate_url("http://10.0.0.5/v").await.is_err());
        assert!(validate_url("http://192.168.1.1/v").await.is_err());
        assert!(validate_url("http://[::1]/v").await.is_err());
        assert!(validate_url("not a url").await.is_err());
    }

    #[tokio::test]
    async fn validate_url_rejects_dns_private_results_and_special_ips() {
        assert!(validate_with_ips(
            "https://public.example/v",
            vec!["10.0.0.5".parse().unwrap()]
        )
        .await
        .is_err());
        assert!(validate_url("http://169.254.169.254/latest").await.is_err());
        assert!(validate_url("http://100.64.0.1/v").await.is_err());
        assert!(validate_url("http://[::ffff:127.0.0.1]/v").await.is_err());
        assert!(validate_url("http://[fc00::1]/v").await.is_err());
        assert!(validate_url("http://[fe80::1]/v").await.is_err());
    }

    #[tokio::test]
    async fn validate_url_rejects_ambiguous_hosts_and_credentials() {
        assert!(validate_url("http://2130706433/v").await.is_err());
        assert!(validate_url("http://0177.0.0.1/v").await.is_err());
        assert!(validate_url("http://0x7f.1/v").await.is_err());
        assert!(validate_url("http://user@example.com/v").await.is_err());
        assert!(validate_url("http://printer.local/v").await.is_err());
        assert!(validate_url("http://app.localhost/v").await.is_err());
    }
}
