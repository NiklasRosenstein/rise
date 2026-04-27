use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use moka::future::Cache;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Rate limiter for OAuth endpoints with three independent limits:
/// - Per-IP: 10 requests per 5 minutes
/// - Per-session: 5 requests per 5 minutes (keyed by `rise_jwt` cookie fingerprint)
/// - Global: 1000 requests per minute
pub struct OAuthRateLimiter {
    ip_limiter: Arc<Cache<String, Arc<AtomicU32>>>,
    session_limiter: Arc<Cache<String, Arc<AtomicU32>>>,
    global_limiter: Arc<Cache<String, Arc<AtomicU32>>>,
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

    /// Atomically increment counters and check limits.
    ///
    /// Uses `AtomicU32` counters stored in the cache so concurrent increments are never lost.
    /// The cache's `time_to_live` acts as a fixed window: a counter entry is created (with TTL)
    /// on first access and expires naturally, resetting the count. Subsequent increments within
    /// the window use the existing entry without refreshing its TTL.
    ///
    /// Returns `Ok(())` if within limits after incrementing, `Err(retry_after_secs)` if exceeded.
    pub async fn increment_and_check(
        &self,
        ip: &str,
        session_key: Option<&str>,
    ) -> Result<(), u64> {
        // Global
        let global_counter = self
            .global_limiter
            .entry_by_ref("global")
            .or_insert_with(std::future::ready(Arc::new(AtomicU32::new(0))))
            .await
            .into_value();
        let global_count = global_counter.fetch_add(1, Ordering::Relaxed) + 1;
        if global_count > Self::GLOBAL_MAX {
            return Err(Self::GLOBAL_WINDOW_SECS);
        }

        // Per-IP
        let ip_counter = self
            .ip_limiter
            .entry_by_ref(ip)
            .or_insert_with(std::future::ready(Arc::new(AtomicU32::new(0))))
            .await
            .into_value();
        let ip_count = ip_counter.fetch_add(1, Ordering::Relaxed) + 1;
        if ip_count > Self::IP_MAX {
            return Err(Self::IP_WINDOW_SECS);
        }

        // Per-session
        if let Some(key) = session_key {
            let session_counter = self
                .session_limiter
                .entry_by_ref(key)
                .or_insert_with(std::future::ready(Arc::new(AtomicU32::new(0))))
                .await
                .into_value();
            let session_count = session_counter.fetch_add(1, Ordering::Relaxed) + 1;
            if session_count > Self::SESSION_MAX {
                return Err(Self::SESSION_WINDOW_SECS);
            }
        }

        Ok(())
    }
}

/// Extract the client IP address from request headers.
///
/// Checks `X-Real-IP` first (set by the reverse proxy to the actual client IP), then falls back
/// to the **rightmost** entry in `X-Forwarded-For` (the entry appended by the most recent trusted
/// proxy), then falls back to `"unknown"`.
///
/// `X-Real-IP` is preferred because ingress-nginx sets it to the real connecting client address,
/// while `X-Forwarded-For` can contain client-supplied values (the leftmost entries) that are
/// trivially spoofable.
pub fn extract_client_ip(headers: &HeaderMap) -> String {
    // Prefer X-Real-IP: set by the reverse proxy to the actual connecting client IP.
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(value) = real_ip.to_str() {
            let value = value.trim();
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }

    // Fall back to X-Forwarded-For: use the rightmost entry, which is the one appended by the
    // nearest trusted proxy. The leftmost entries are client-controlled and must not be trusted.
    if let Some(forwarded_for) = headers.get("x-forwarded-for") {
        if let Ok(value) = forwarded_for.to_str() {
            if let Some(ip) = value.rsplit(',').next() {
                let ip = ip.trim();
                if !ip.is_empty() {
                    return ip.to_string();
                }
            }
        }
    }

    "unknown".to_string()
}

/// Extract a session fingerprint from the `rise_jwt` cookie for rate limiting.
///
/// Returns a SHA-256 hash of the full cookie value prefixed with `"session:"`, or `None` if the
/// cookie is absent. Hashing ensures unique keys per-user (a prefix-based approach would collide
/// because all HS256 JWTs share the same base64url-encoded header).
pub fn extract_session_key(headers: &HeaderMap) -> Option<String> {
    let cookie_str = headers.get("cookie")?.to_str().ok()?;

    for pair in cookie_str.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix("rise_jwt=") {
            if !value.is_empty() {
                let hash = Sha256::digest(value.as_bytes());
                let mut key = String::with_capacity("session:".len() + 64);
                key.push_str("session:");
                for b in hash.iter() {
                    use std::fmt::Write;
                    let _ = write!(key, "{b:02x}");
                }
                return Some(key);
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
    fn test_extract_client_ip_real_ip_preferred() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("9.10.11.12"));
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("1.2.3.4, 5.6.7.8"),
        );
        // X-Real-IP takes precedence over X-Forwarded-For
        assert_eq!(extract_client_ip(&headers), "9.10.11.12");
    }

    #[test]
    fn test_extract_client_ip_forwarded_for_rightmost() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("1.2.3.4, 5.6.7.8"),
        );
        // Uses rightmost entry (proxy-appended), not leftmost (client-supplied)
        assert_eq!(extract_client_ip(&headers), "5.6.7.8");
    }

    #[test]
    fn test_extract_client_ip_unknown() {
        let headers = HeaderMap::new();
        assert_eq!(extract_client_ip(&headers), "unknown");
    }

    #[test]
    fn test_extract_session_key_present() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            HeaderValue::from_str("rise_jwt=some-token-value; other=value").unwrap(),
        );
        let key = extract_session_key(&headers).unwrap();
        assert!(key.starts_with("session:"));
        // SHA-256 hex digest is 64 chars
        assert_eq!(key.len(), "session:".len() + 64);
    }

    #[test]
    fn test_extract_session_key_different_tokens_produce_different_keys() {
        let mut headers_a = HeaderMap::new();
        headers_a.insert("cookie", HeaderValue::from_static("rise_jwt=token-a"));
        let mut headers_b = HeaderMap::new();
        headers_b.insert("cookie", HeaderValue::from_static("rise_jwt=token-b"));
        let key_a = extract_session_key(&headers_a).unwrap();
        let key_b = extract_session_key(&headers_b).unwrap();
        assert_ne!(key_a, key_b);
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
