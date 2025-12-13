use axum::http::HeaderMap;

/// Cookie name for the session token (IdP JWT, fallback)
pub const COOKIE_NAME: &str = "_rise_session";

/// Cookie name for the ingress authentication token (Rise JWT)
pub const INGRESS_JWT_COOKIE_NAME: &str = "_rise_ingress";

/// Settings for session cookies
#[derive(Debug, Clone)]
pub struct CookieSettings {
    pub domain: String,
    pub secure: bool,
}

/// Create a session cookie with the given JWT token
///
/// The cookie is configured with:
/// - HttpOnly: Prevents JavaScript access (XSS protection)
/// - Secure: HTTPS-only transmission (configurable for development)
/// - SameSite=Lax: CSRF protection while allowing navigation
/// - Domain: Shared across subdomains (e.g., .rise.dev)
/// - Max-Age: Matches JWT expiry time
/// - Path=/: Valid for all paths
pub fn create_session_cookie(
    id_token: &str,
    settings: &CookieSettings,
    max_age_seconds: u64,
) -> String {
    let mut cookie_parts = vec![
        format!("{}={}", COOKIE_NAME, id_token),
        format!("Max-Age={}", max_age_seconds),
        "Path=/".to_string(),
        "HttpOnly".to_string(),
        "SameSite=Lax".to_string(),
    ];

    // Only set Domain if it's not empty (empty means current host only)
    if !settings.domain.is_empty() {
        cookie_parts.push(format!("Domain={}", settings.domain));
    }

    // Only set Secure flag if configured (false for HTTP development)
    if settings.secure {
        cookie_parts.push("Secure".to_string());
    }

    cookie_parts.join("; ")
}

/// Extract the session cookie value from request headers
///
/// Parses the Cookie header and extracts the _rise_session value if present
#[cfg_attr(not(test), allow(dead_code))]
pub fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|cookie| {
            let cookie = cookie.trim();
            cookie
                .strip_prefix(&format!("{}=", COOKIE_NAME))
                .map(|value| value.to_string())
        })
}

/// Create a cookie that clears the session
///
/// Sets Max-Age=0 to immediately expire the cookie
pub fn clear_session_cookie(settings: &CookieSettings) -> String {
    let mut cookie_parts = vec![
        format!("{}=", COOKIE_NAME),
        "Max-Age=0".to_string(),
        "Path=/".to_string(),
        "HttpOnly".to_string(),
        "SameSite=Lax".to_string(),
    ];

    if !settings.domain.is_empty() {
        cookie_parts.push(format!("Domain={}", settings.domain));
    }

    if settings.secure {
        cookie_parts.push("Secure".to_string());
    }

    cookie_parts.join("; ")
}

/// Create an ingress JWT cookie with the given Rise-issued JWT token
///
/// This cookie is specifically for ingress authentication and is separate from
/// the session cookie to prevent projects from accessing Rise APIs.
///
/// The cookie is configured with:
/// - HttpOnly: Prevents JavaScript access (XSS protection)
/// - Secure: HTTPS-only transmission (configurable for development)
/// - SameSite=Lax: CSRF protection while allowing navigation
/// - Domain: Shared across subdomains (e.g., .rise.dev)
/// - Max-Age: Matches JWT expiry time
/// - Path=/: Valid for all paths
pub fn create_ingress_jwt_cookie(
    jwt: &str,
    settings: &CookieSettings,
    max_age_seconds: u64,
) -> String {
    let mut cookie_parts = vec![
        format!("{}={}", INGRESS_JWT_COOKIE_NAME, jwt),
        format!("Max-Age={}", max_age_seconds),
        "Path=/".to_string(),
        "HttpOnly".to_string(),
        "SameSite=Lax".to_string(),
    ];

    // Only set Domain if it's not empty (empty means current host only)
    if !settings.domain.is_empty() {
        cookie_parts.push(format!("Domain={}", settings.domain));
    }

    // Only set Secure flag if configured (false for HTTP development)
    if settings.secure {
        cookie_parts.push("Secure".to_string());
    }

    cookie_parts.join("; ")
}

/// Extract the ingress JWT cookie value from request headers
///
/// Parses the Cookie header and extracts the _rise_ingress value if present
pub fn extract_ingress_jwt_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|cookie| {
            let cookie = cookie.trim();
            cookie
                .strip_prefix(&format!("{}=", INGRESS_JWT_COOKIE_NAME))
                .map(|value| value.to_string())
        })
}

/// Create a cookie that clears the ingress JWT
///
/// Sets Max-Age=0 to immediately expire the cookie
///
/// Future feature: Kubernetes ingress authentication
#[allow(dead_code)]
pub fn clear_ingress_jwt_cookie(settings: &CookieSettings) -> String {
    let mut cookie_parts = vec![
        format!("{}=", INGRESS_JWT_COOKIE_NAME),
        "Max-Age=0".to_string(),
        "Path=/".to_string(),
        "HttpOnly".to_string(),
        "SameSite=Lax".to_string(),
    ];

    if !settings.domain.is_empty() {
        cookie_parts.push(format!("Domain={}", settings.domain));
    }

    if settings.secure {
        cookie_parts.push("Secure".to_string());
    }

    cookie_parts.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_create_session_cookie_with_domain() {
        let settings = CookieSettings {
            domain: ".rise.dev".to_string(),
            secure: true,
        };

        let cookie = create_session_cookie("test_token_123", &settings, 3600);

        assert!(cookie.contains("_rise_session=test_token_123"));
        assert!(cookie.contains("Max-Age=3600"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.contains("Domain=.rise.dev"));
        assert!(cookie.contains("Secure"));
    }

    #[test]
    fn test_create_session_cookie_without_domain() {
        let settings = CookieSettings {
            domain: String::new(),
            secure: false,
        };

        let cookie = create_session_cookie("test_token", &settings, 1800);

        assert!(cookie.contains("_rise_session=test_token"));
        assert!(cookie.contains("Max-Age=1800"));
        assert!(!cookie.contains("Domain="));
        assert!(!cookie.contains("Secure"));
    }

    #[test]
    fn test_extract_session_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            HeaderValue::from_static("_rise_session=my_token; other_cookie=value"),
        );

        let token = extract_session_cookie(&headers);
        assert_eq!(token, Some("my_token".to_string()));
    }

    #[test]
    fn test_extract_session_cookie_with_spaces() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            HeaderValue::from_static("other=value; _rise_session=token123; another=test"),
        );

        let token = extract_session_cookie(&headers);
        assert_eq!(token, Some("token123".to_string()));
    }

    #[test]
    fn test_extract_session_cookie_not_present() {
        let mut headers = HeaderMap::new();
        headers.insert("cookie", HeaderValue::from_static("other_cookie=value"));

        let token = extract_session_cookie(&headers);
        assert_eq!(token, None);
    }

    #[test]
    fn test_extract_session_cookie_no_cookie_header() {
        let headers = HeaderMap::new();
        let token = extract_session_cookie(&headers);
        assert_eq!(token, None);
    }

    #[test]
    fn test_clear_session_cookie() {
        let settings = CookieSettings {
            domain: ".rise.dev".to_string(),
            secure: true,
        };

        let cookie = clear_session_cookie(&settings);

        assert!(cookie.contains("_rise_session="));
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Domain=.rise.dev"));
        assert!(cookie.contains("Secure"));
    }

    #[test]
    fn test_create_ingress_jwt_cookie() {
        let settings = CookieSettings {
            domain: ".rise.dev".to_string(),
            secure: true,
        };

        let cookie = create_ingress_jwt_cookie("jwt_token_xyz", &settings, 3600);

        assert!(cookie.contains("_rise_ingress=jwt_token_xyz"));
        assert!(cookie.contains("Max-Age=3600"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.contains("Domain=.rise.dev"));
        assert!(cookie.contains("Secure"));
    }

    #[test]
    fn test_extract_ingress_jwt_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            HeaderValue::from_static("_rise_ingress=my_jwt; other_cookie=value"),
        );

        let jwt = extract_ingress_jwt_cookie(&headers);
        assert_eq!(jwt, Some("my_jwt".to_string()));
    }

    #[test]
    fn test_extract_ingress_jwt_cookie_not_present() {
        let mut headers = HeaderMap::new();
        headers.insert("cookie", HeaderValue::from_static("other_cookie=value"));

        let jwt = extract_ingress_jwt_cookie(&headers);
        assert_eq!(jwt, None);
    }

    #[test]
    fn test_clear_ingress_jwt_cookie() {
        let settings = CookieSettings {
            domain: ".rise.dev".to_string(),
            secure: true,
        };

        let cookie = clear_ingress_jwt_cookie(&settings);

        assert!(cookie.contains("_rise_ingress="));
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Domain=.rise.dev"));
        assert!(cookie.contains("Secure"));
    }
}
