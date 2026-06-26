//! URL validation / SSRF guard for imports. Pure, no I/O.

use std::net::IpAddr;

use anyhow::{anyhow, Result};
use url::Url;

/// Reject non-http(s) URLs and ones that point at the local machine / private
/// network (a basic SSRF guard). Returns a Russian error message on rejection.
pub fn validate_url(raw: &str) -> Result<()> {
    let u = Url::parse(raw).map_err(|_| anyhow!("Недопустимый URL"))?;
    if !matches!(u.scheme(), "http" | "https") {
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
    }
    Ok(())
}

fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.octets()[0] == 0
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() {
                return true;
            }
            let seg = v6.segments();
            // fc00::/7 (unique local) and fe80::/10 (link local).
            (seg[0] & 0xfe00) == 0xfc00 || (seg[0] & 0xffc0) == 0xfe80
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_url_blocks_local_and_bad_schemes() {
        assert!(validate_url("https://example.com/v").is_ok());
        assert!(validate_url("http://1.2.3.4/v").is_ok());
        assert!(validate_url("ftp://example.com/v").is_err());
        assert!(validate_url("https://localhost/v").is_err());
        assert!(validate_url("http://127.0.0.1/v").is_err());
        assert!(validate_url("http://10.0.0.5/v").is_err());
        assert!(validate_url("http://192.168.1.1/v").is_err());
        assert!(validate_url("http://[::1]/v").is_err());
        assert!(validate_url("not a url").is_err());
    }
}
