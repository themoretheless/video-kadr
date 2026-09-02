//! URL validation / SSRF guard for imports.

use std::net::IpAddr;
use std::time::Duration;
use std::{future::Future, io};

use anyhow::{anyhow, Result};
use url::Url;

pub(super) const DNS_TIMEOUT: Duration = Duration::from_secs(5);

/// Reject non-http(s) URLs and targets outside the public web. The egress proxy
/// repeats the same host/IP policy for every redirect and downloader request.
pub async fn validate_url(raw: &str) -> Result<()> {
    validate_url_with_resolver(raw, |host, port| async move {
        let addrs =
            tokio::time::timeout(DNS_TIMEOUT, tokio::net::lookup_host((host.as_str(), port)))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "DNS lookup timed out"))??;
        Ok(addrs.map(|addr| addr.ip()).collect())
    })
    .await
}

pub(super) async fn validate_url_with_resolver<F, Fut>(raw: &str, resolve: F) -> Result<()>
where
    F: FnOnce(String, u16) -> Fut,
    Fut: Future<Output = io::Result<Vec<IpAddr>>>,
{
    let (host, port) = validate_url_structure(raw)?;
    if host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    let ips = resolve(host, port)
        .await
        .map_err(|_| anyhow!("Недопустимый URL"))?;
    if !resolved_ips_are_public(&ips) {
        return Err(anyhow!("Недопустимый URL"));
    }
    Ok(())
}

/// Pure, DNS-free URL policy used before resolution and by fuzz/security tests.
pub fn validate_url_structure(raw: &str) -> Result<(String, u16)> {
    let u = Url::parse(raw).map_err(|_| anyhow!("Недопустимый URL"))?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err(anyhow!("Недопустимый URL"));
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err(anyhow!("Недопустимый URL"));
    }
    let host = u.host_str().ok_or_else(|| anyhow!("Недопустимый URL"))?;
    let host = normalize_host(host);
    let port = u
        .port_or_known_default()
        .ok_or_else(|| anyhow!("Недопустимый URL"))?;
    if !is_allowed_port(port) || !is_allowed_host(&host) {
        return Err(anyhow!("Недопустимый URL"));
    }
    Ok((host, port))
}

pub(super) fn normalize_host(host: &str) -> String {
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase()
}

pub(super) fn is_allowed_port(port: u16) -> bool {
    matches!(port, 80 | 443)
}

pub(super) fn is_allowed_host(host: &str) -> bool {
    let host = normalize_host(host);
    if host.is_empty()
        || host == "localhost"
        || host.ends_with(".local")
        || host.ends_with(".localhost")
    {
        return false;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return !is_blocked_ip(&ip);
    }
    !looks_like_noncanonical_ip(&host)
}

pub(super) fn resolved_ips_are_public(ips: &[IpAddr]) -> bool {
    !ips.is_empty() && ips.iter().all(|ip| !is_blocked_ip(ip))
}

pub(super) fn is_blocked_ip(ip: &IpAddr) -> bool {
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
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
                || (octets[0] == 192 && octets[1] == 88 && octets[2] == 99)
                || (octets[0] == 198 && (18..=19).contains(&octets[1]))
                || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
                || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
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
            // (deprecated site local), 64:ff9b::/96 (NAT64), and 2001:db8::/32
            // (documentation). NAT64 is blocked as a class so it cannot tunnel
            // a forbidden IPv4 target through an otherwise public IPv6 prefix.
            (seg[0] & 0xfe00) == 0xfc00
                || (seg[0] & 0xffc0) == 0xfe80
                || (seg[0] & 0xffc0) == 0xfec0
                || (seg[0] == 0x0064 && seg[1] == 0xff9b)
                || (seg[0] == 0x2001 && seg[1] == 0x0db8)
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
        assert!(validate_url("http://192.0.2.1/v").await.is_err());
        assert!(validate_url("http://198.51.100.1/v").await.is_err());
        assert!(validate_url("http://203.0.113.1/v").await.is_err());
        assert!(validate_url("http://[::ffff:127.0.0.1]/v").await.is_err());
        assert!(validate_url("http://[fc00::1]/v").await.is_err());
        assert!(validate_url("http://[fe80::1]/v").await.is_err());
        assert!(validate_url("http://[64:ff9b::7f00:1]/v").await.is_err());
        assert!(validate_url("http://[2001:db8::1]/v").await.is_err());
    }

    #[tokio::test]
    async fn validate_url_rejects_ambiguous_hosts_and_credentials() {
        assert!(validate_url("http://2130706433/v").await.is_err());
        assert!(validate_url("http://0177.0.0.1/v").await.is_err());
        assert!(validate_url("http://0x7f.1/v").await.is_err());
        assert!(validate_url("http://user@example.com/v").await.is_err());
        assert!(validate_url("http://printer.local/v").await.is_err());
        assert!(validate_url("http://app.localhost/v").await.is_err());
        assert!(validate_url("https://example.com:8443/v").await.is_err());
    }

    #[tokio::test]
    async fn validate_url_rejects_mixed_public_and_private_dns_answers() {
        assert!(validate_with_ips(
            "https://public.example/v",
            vec![
                "93.184.216.34".parse().unwrap(),
                "127.0.0.1".parse().unwrap(),
            ],
        )
        .await
        .is_err());
    }

    #[test]
    fn structural_policy_is_dns_free_and_matches_public_ip_rules() {
        assert_eq!(
            validate_url_structure("https://example.com/video").unwrap(),
            ("example.com".to_owned(), 443)
        );
        assert!(validate_url_structure("https://user@example.com/video").is_err());
        assert!(validate_url_structure("http://127.0.0.1/video").is_err());
    }
}
