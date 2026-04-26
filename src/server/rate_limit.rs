use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;

/// Rate limiter for OAuth endpoints with three independent limits:
/// - Per-IP: 10 requests per 5 minutes
/// - Per-session: 5 requests per 5 minutes (keyed by `rise_jwt` cookie fingerprint)
/// - Global: 1000 requests per minute
pub struct OAuthRateLimiter {
    ip_limiter: Arc<Cache<String, u32>>,
    session_limiter: Arc<Cache<String, u32>>,
    global_limiter: Arc<Cache<String, u32>>,
}

impl OAuthRateLimiter {
    pub const IP_MAX: u32 = 10;
    pub const IP_WINDOW_SECS: u64 = 300; // 5 minutes
    pub const IP_MAX_CAPACITY: u64 = 50_000;

    pub const SESSION_MAX: u32 = 5;
    pub const SESSION_WINDOW_SECS: u64 = 300; // 5 minutes
    pub const SESSION_MAX_CAPACITY: u64 = 10_000;

    pub const GLOBAL_MAX: u32 = 1000;
    pub const GLOBAL_WINDOW_SECS: u64 = 60; // 1 minute
    pub const GLOBAL_MAX_CAPACITY: u64 = 1;

    pub fn new() -> Self {
        let ip_limiter = Arc::new(
            Cache::builder()
                .time_to_live(Duration::from_secs(Self::IP_WINDOW_SECS))
                .max_capacity(Self::IP_MAX_CAPACITY)
                .build(),
        );
        let session_limiter = Arc::new(
            Cache::builder()
                .time_to_live(Duration::from_secs(Self::SESSION_WINDOW_SECS))
                .max_capacity(Self::SESSION_MAX_CAPACITY)
                .build(),
        );
        let global_limiter = Arc::new(
            Cache::builder()
                .time_to_live(Duration::from_secs(Self::GLOBAL_WINDOW_SECS))
                .max_capacity(Self::GLOBAL_MAX_CAPACITY)
                .build(),
        );
        Self {
            ip_limiter,
            session_limiter,
            global_limiter,
        }
    }

    /// Check all applicable rate limits without incrementing counters.
    ///
    /// Returns `Ok(())` if within all limits, `Err(retry_after_secs)` if any limit is exceeded.
    pub async fn check(&self, ip: &str, session_key: Option<&str>) -> Result<(), u64> {
        // Check global limit first
        let global_count = self.global_limiter.get("global").await.unwrap_or(0);
        if global_count >= Self::GLOBAL_MAX {
            return Err(Self::GLOBAL_WINDOW_SECS);
        }

        // Check per-IP limit
        let ip_count = self.ip_limiter.get(ip).await.unwrap_or(0);
        if ip_count >= Self::IP_MAX {
            return Err(Self::IP_WINDOW_SECS);
        }

        // Check per-session limit if a session key is present
        if let Some(key) = session_key {
            let session_count = self.session_limiter.get(key).await.unwrap_or(0);
            if session_count >= Self::SESSION_MAX {
                return Err(Self::SESSION_WINDOW_SECS);
            }
        }

        Ok(())
    }

    /// Increment all applicable counters.
    pub async fn increment(&self, ip: &str, session_key: Option<&str>) {
        let global_count = self.global_limiter.get("global").await.unwrap_or(0);
        self.global_limiter
            .insert("global".to_string(), global_count + 1)
            .await;

        let ip_count = self.ip_limiter.get(ip).await.unwrap_or(0);
        self.ip_limiter.insert(ip.to_string(), ip_count + 1).await;

        if let Some(key) = session_key {
            let session_count = self.session_limiter.get(key).await.unwrap_or(0);
            self.session_limiter
                .insert(key.to_string(), session_count + 1)
                .await;
        }
    }

    /// Increment counters then check limits. All attempts are counted even if they exceed
    /// the limit, preventing the counter from resetting during an attack.
    ///
    /// Note: The increment and check are two separate cache operations and are not atomic.
    /// Under high concurrency a few extra requests may slip through just as a window expires,
    /// but this is acceptable for in-memory rate limiting where strict atomicity would require
    /// a distributed lock. The increment-first order ensures that even simultaneous requests
    /// are counted, making it harder to bypass the limit by parallelising requests.
    ///
    /// Returns `Ok(())` if within limits after incrementing, `Err(retry_after_secs)` if exceeded.
    pub async fn increment_and_check(
        &self,
        ip: &str,
        session_key: Option<&str>,
    ) -> Result<(), u64> {
        self.increment(ip, session_key).await;
        self.check(ip, session_key).await
    }
}

/// Extract the client IP address from request headers.
///
/// Checks `X-Forwarded-For` first (leftmost entry), then `X-Real-IP`, then falls back to
/// `"unknown"` when neither header is present.
pub fn extract_client_ip(headers: &HeaderMap) -> String {
    if let Some(forwarded_for) = headers.get("x-forwarded-for") {
        if let Ok(value) = forwarded_for.to_str() {
            if let Some(ip) = value.split(',').next() {
                let ip = ip.trim();
                if !ip.is_empty() {
                    return ip.to_string();
                }
            }
        }
    }

    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(value) = real_ip.to_str() {
            let value = value.trim();
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }

    "unknown".to_string()
}

/// Number of leading characters from the `rise_jwt` cookie used as the session fingerprint.
const SESSION_FINGERPRINT_LENGTH: usize = 40;

/// Extract a session fingerprint from the `rise_jwt` cookie for rate limiting.
///
/// Returns the first [`SESSION_FINGERPRINT_LENGTH`] characters of the token value, prefixed with
/// `"session:"`, or `None` if the cookie is absent.
pub fn extract_session_key(headers: &HeaderMap) -> Option<String> {
    let cookie_str = headers.get("cookie")?.to_str().ok()?;

    for pair in cookie_str.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix("rise_jwt=") {
            if !value.is_empty() {
                let fingerprint: String = value.chars().take(SESSION_FINGERPRINT_LENGTH).collect();
                return Some(format!("session:{}", fingerprint));
            }
        }
    }
    None
}

/// Build a `429 Too Many Requests` response with a `Retry-After` header.
pub fn rate_limit_response(retry_after: u64) -> Response {
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        "Rate limit exceeded. Please try again later.",
    )
        .into_response();
    if let Ok(value) = axum::http::HeaderValue::from_str(&retry_after.to_string()) {
        response
            .headers_mut()
            .insert(axum::http::header::RETRY_AFTER, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn test_extract_client_ip_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("1.2.3.4, 5.6.7.8"),
        );
        assert_eq!(extract_client_ip(&headers), "1.2.3.4");
    }

    #[test]
    fn test_extract_client_ip_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("9.10.11.12"));
        assert_eq!(extract_client_ip(&headers), "9.10.11.12");
    }

    #[test]
    fn test_extract_client_ip_unknown() {
        let headers = HeaderMap::new();
        assert_eq!(extract_client_ip(&headers), "unknown");
    }

    #[test]
    fn test_extract_session_key_present() {
        let mut headers = HeaderMap::new();
        let token = "a".repeat(60);
        headers.insert(
            "cookie",
            HeaderValue::from_str(&format!("rise_jwt={token}; other=value")).unwrap(),
        );
        let key = extract_session_key(&headers).unwrap();
        assert!(key.starts_with("session:"));
        // Fingerprint is capped at SESSION_FINGERPRINT_LENGTH chars
        assert_eq!(key.len(), "session:".len() + SESSION_FINGERPRINT_LENGTH);
    }

    #[test]
    fn test_extract_session_key_absent() {
        let mut headers = HeaderMap::new();
        headers.insert("cookie", HeaderValue::from_static("other=value"));
        assert!(extract_session_key(&headers).is_none());
    }

    #[tokio::test]
    async fn test_rate_limiter_allows_within_limit() {
        let limiter = OAuthRateLimiter::new();
        // First request should be allowed
        assert!(limiter.increment_and_check("1.2.3.4", None).await.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks_at_ip_limit() {
        let limiter = OAuthRateLimiter::new();
        let ip = "10.0.0.1";
        // Exhaust the IP limit
        for _ in 0..OAuthRateLimiter::IP_MAX {
            let _ = limiter.increment_and_check(ip, None).await;
        }
        // Next request should be blocked
        let result = limiter.increment_and_check(ip, None).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), OAuthRateLimiter::IP_WINDOW_SECS);
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks_at_session_limit() {
        let limiter = OAuthRateLimiter::new();
        let ip = "10.0.0.2";
        let session = "session:abc123";
        // Exhaust the session limit
        for _ in 0..OAuthRateLimiter::SESSION_MAX {
            let _ = limiter.increment_and_check(ip, Some(session)).await;
        }
        // Next request should be blocked
        let result = limiter.increment_and_check(ip, Some(session)).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), OAuthRateLimiter::SESSION_WINDOW_SECS);
    }
}
