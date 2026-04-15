use std::net::IpAddr;
use std::time::Duration;

/// Errors that can occur during SSRF-safe URL validation.
#[derive(Debug)]
pub enum SsrfError {
    /// URL must use HTTPS scheme.
    HttpsRequired,
    /// The URL could not be parsed.
    InvalidUrl(String),
    /// The hostname could not be resolved.
    DnsResolutionFailed(String),
    /// The resolved IP address is in a blocked range (private, loopback, link-local, etc.).
    BlockedIpRange(IpAddr),
}

impl std::fmt::Display for SsrfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SsrfError::HttpsRequired => write!(f, "URL must use HTTPS"),
            SsrfError::InvalidUrl(msg) => write!(f, "Invalid URL: {}", msg),
            SsrfError::DnsResolutionFailed(msg) => write!(f, "DNS resolution failed: {}", msg),
            SsrfError::BlockedIpRange(ip) => {
                write!(f, "URL resolves to a blocked IP address: {}", ip)
            }
        }
    }
}

/// Check whether an IP address is in a private, loopback, link-local, or otherwise
/// internal range that should not be reachable from server-side requests.
fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_loopback()             // 127.0.0.0/8
                || ipv4.is_private()        // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || ipv4.is_link_local()     // 169.254.0.0/16 (includes AWS metadata 169.254.169.254)
                || ipv4.is_unspecified()     // 0.0.0.0
                || ipv4.is_broadcast()      // 255.255.255.255
                || ipv4.is_documentation()  // 192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24
                || ipv4.octets()[0] == 100 && (ipv4.octets()[1] & 0xC0) == 64 // 100.64.0.0/10 (CGNAT)
        }
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback()          // ::1
                || ipv6.is_unspecified() // ::
                // Unique local (fc00::/7)
                || (ipv6.segments()[0] & 0xfe00) == 0xfc00
                // Link-local (fe80::/10)
                || (ipv6.segments()[0] & 0xffc0) == 0xfe80
                // IPv4-mapped addresses (::ffff:0:0/96) — check the mapped IPv4
                || match ipv6.to_ipv4_mapped() {
                    Some(mapped_v4) => is_blocked_ip(&IpAddr::V4(mapped_v4)),
                    None => false,
                }
        }
    }
}

/// Validate that a URL is safe to fetch (SSRF protection).
///
/// Checks:
/// 1. Scheme must be HTTPS (unless `allow_private_networks` is true)
/// 2. Hostname must be present
/// 3. All resolved IP addresses must not be in blocked ranges (unless `allow_private_networks` is true)
///
/// When `allow_private_networks` is true, HTTP and private/loopback IPs are permitted.
/// **WARNING**: Only enable for local development. Never enable in production.
pub async fn validate_url(url: &str, allow_private_networks: bool) -> Result<(), SsrfError> {
    // Parse the URL
    let parsed = url::Url::parse(url).map_err(|e| SsrfError::InvalidUrl(e.to_string()))?;

    // Require HTTPS (unless relaxed for development)
    if !allow_private_networks && parsed.scheme() != "https" {
        return Err(SsrfError::HttpsRequired);
    }

    // Extract hostname
    let host = parsed
        .host_str()
        .ok_or_else(|| SsrfError::InvalidUrl("URL has no host".to_string()))?;

    // If host is already an IP address, check it directly
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !allow_private_networks && is_blocked_ip(&ip) {
            return Err(SsrfError::BlockedIpRange(ip));
        }
        return Ok(());
    }

    // Skip DNS resolution checks when private networks are allowed
    if allow_private_networks {
        return Ok(());
    }

    // Resolve hostname and check all resolved addresses
    let port = parsed.port().unwrap_or(443);
    let addr = format!("{}:{}", host, port);

    let resolved = tokio::net::lookup_host(&addr)
        .await
        .map_err(|e| SsrfError::DnsResolutionFailed(format!("{}: {}", host, e)))?;

    let addrs: Vec<_> = resolved.collect();
    if addrs.is_empty() {
        return Err(SsrfError::DnsResolutionFailed(format!(
            "{}: no addresses resolved",
            host
        )));
    }

    for addr in &addrs {
        if is_blocked_ip(&addr.ip()) {
            return Err(SsrfError::BlockedIpRange(addr.ip()));
        }
    }

    Ok(())
}

/// Create an HTTP client configured with SSRF mitigations.
///
/// The client has:
/// - 10-second connect and total request timeout
/// - Custom redirect policy (max 3 hops, HTTPS-only, blocks private/internal IPs)
///
/// When `allow_private_networks` is true, redirect checks for HTTPS and blocked IPs are skipped.
/// **WARNING**: Only enable for local development. Never enable in production.
pub fn safe_client(allow_private_networks: bool) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 3 {
                attempt.error("too many redirects")
            } else if !allow_private_networks && attempt.url().scheme() != "https" {
                attempt.error("redirect target must use HTTPS")
            } else if let Some(host) = attempt.url().host_str() {
                if !allow_private_networks {
                    if let Ok(ip) = host.parse::<IpAddr>() {
                        if is_blocked_ip(&ip) {
                            return attempt
                                .error(format!("redirect target resolves to blocked IP: {}", ip));
                        }
                    }
                }
                attempt.follow()
            } else {
                attempt.error("redirect target has no host")
            }
        }))
        .build()
        .expect("failed to build SSRF-safe HTTP client")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocked_ipv4_loopback() {
        assert!(is_blocked_ip(&"127.0.0.1".parse().unwrap()));
        assert!(is_blocked_ip(&"127.0.0.2".parse().unwrap()));
    }

    #[test]
    fn test_blocked_ipv4_private() {
        assert!(is_blocked_ip(&"10.0.0.1".parse().unwrap()));
        assert!(is_blocked_ip(&"172.16.0.1".parse().unwrap()));
        assert!(is_blocked_ip(&"172.31.255.255".parse().unwrap()));
        assert!(is_blocked_ip(&"192.168.0.1".parse().unwrap()));
        assert!(is_blocked_ip(&"192.168.1.100".parse().unwrap()));
    }

    #[test]
    fn test_blocked_ipv4_link_local() {
        // AWS metadata endpoint
        assert!(is_blocked_ip(&"169.254.169.254".parse().unwrap()));
        assert!(is_blocked_ip(&"169.254.0.1".parse().unwrap()));
    }

    #[test]
    fn test_blocked_ipv4_unspecified() {
        assert!(is_blocked_ip(&"0.0.0.0".parse().unwrap()));
    }

    #[test]
    fn test_blocked_ipv4_cgnat() {
        assert!(is_blocked_ip(&"100.64.0.1".parse().unwrap()));
        assert!(is_blocked_ip(&"100.127.255.255".parse().unwrap()));
    }

    #[test]
    fn test_allowed_ipv4_public() {
        assert!(!is_blocked_ip(&"8.8.8.8".parse().unwrap()));
        assert!(!is_blocked_ip(&"1.1.1.1".parse().unwrap()));
        assert!(!is_blocked_ip(&"142.250.80.46".parse().unwrap()));
    }

    #[test]
    fn test_blocked_ipv6_loopback() {
        assert!(is_blocked_ip(&"::1".parse().unwrap()));
    }

    #[test]
    fn test_blocked_ipv6_unspecified() {
        assert!(is_blocked_ip(&"::".parse().unwrap()));
    }

    #[test]
    fn test_blocked_ipv6_unique_local() {
        assert!(is_blocked_ip(&"fc00::1".parse().unwrap()));
        assert!(is_blocked_ip(&"fd00::1".parse().unwrap()));
    }

    #[test]
    fn test_blocked_ipv6_link_local() {
        assert!(is_blocked_ip(&"fe80::1".parse().unwrap()));
    }

    #[test]
    fn test_blocked_ipv4_mapped_ipv6() {
        // ::ffff:127.0.0.1
        assert!(is_blocked_ip(&"::ffff:127.0.0.1".parse().unwrap()));
        // ::ffff:169.254.169.254
        assert!(is_blocked_ip(&"::ffff:169.254.169.254".parse().unwrap()));
        // ::ffff:10.0.0.1
        assert!(is_blocked_ip(&"::ffff:10.0.0.1".parse().unwrap()));
    }

    #[test]
    fn test_allowed_ipv6_public() {
        assert!(!is_blocked_ip(&"2607:f8b0:4004:800::200e".parse().unwrap()));
    }

    #[tokio::test]
    async fn test_validate_url_rejects_http() {
        let result = validate_url("http://example.com", false).await;
        assert!(matches!(result, Err(SsrfError::HttpsRequired)));
    }

    #[tokio::test]
    async fn test_validate_url_rejects_invalid_url() {
        let result = validate_url("not-a-url", false).await;
        assert!(matches!(result, Err(SsrfError::InvalidUrl(_))));
    }

    #[tokio::test]
    async fn test_validate_url_rejects_ip_loopback() {
        let result = validate_url("https://127.0.0.1/path", false).await;
        assert!(matches!(result, Err(SsrfError::BlockedIpRange(_))));
    }

    #[tokio::test]
    async fn test_validate_url_rejects_ip_metadata() {
        let result = validate_url("https://169.254.169.254/latest/meta-data/", false).await;
        assert!(matches!(result, Err(SsrfError::BlockedIpRange(_))));
    }

    #[tokio::test]
    async fn test_validate_url_rejects_private_ip() {
        let result = validate_url("https://10.0.0.1/internal", false).await;
        assert!(matches!(result, Err(SsrfError::BlockedIpRange(_))));

        let result = validate_url("https://192.168.1.1/admin", false).await;
        assert!(matches!(result, Err(SsrfError::BlockedIpRange(_))));
    }

    #[tokio::test]
    async fn test_validate_url_accepts_public_https() {
        // Use a known public IP to avoid DNS dependency and make the test deterministic.
        let result = validate_url("https://8.8.8.8/", false).await;
        assert!(
            result.is_ok(),
            "expected public HTTPS URL to be accepted, got: {:?}",
            result
        );
    }

    #[test]
    fn test_safe_client_builds_successfully() {
        let _client = safe_client(false);
    }

    // Tests for allow_private_networks = true

    #[tokio::test]
    async fn test_validate_url_allows_http_when_private_networks_enabled() {
        let result = validate_url("http://localhost:5556", true).await;
        assert!(
            result.is_ok(),
            "expected HTTP URL to be accepted with allow_private_networks, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_validate_url_allows_loopback_when_private_networks_enabled() {
        let result = validate_url("https://127.0.0.1/path", true).await;
        assert!(
            result.is_ok(),
            "expected loopback IP to be accepted with allow_private_networks, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_validate_url_allows_private_ip_when_private_networks_enabled() {
        let result = validate_url("https://192.168.1.1/admin", true).await;
        assert!(
            result.is_ok(),
            "expected private IP to be accepted with allow_private_networks, got: {:?}",
            result
        );
    }

    #[test]
    fn test_safe_client_builds_with_private_networks() {
        let _client = safe_client(true);
    }
}
