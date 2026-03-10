// Proxy environment variable handling for build backends

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use tracing::{debug, warn};

/// Proxy environment variable names to support (both uppercase and lowercase)
pub(crate) const PROXY_VAR_NAMES: &[&str] = &[
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "NO_PROXY",
    "no_proxy",
];

/// Read proxy variables from environment and transform localhost URLs to host.docker.internal
pub(crate) fn read_and_transform_proxy_vars() -> HashMap<String, String> {
    let mut proxy_vars = HashMap::new();

    for var_name in PROXY_VAR_NAMES {
        if let Some(value) = super::env_var_non_empty(var_name) {
            // NO_PROXY and no_proxy are comma-separated lists, not URLs - don't transform
            let transformed_value = if var_name.eq_ignore_ascii_case("NO_PROXY") {
                value
            } else {
                match transform_proxy_url(&value) {
                    Ok(transformed) => transformed,
                    Err(e) => {
                        warn!(
                            "Failed to transform proxy URL for {}: {}. Using original value.",
                            var_name, e
                        );
                        value
                    }
                }
            };

            debug!("Found proxy variable: {}={}", var_name, transformed_value);
            proxy_vars.insert(var_name.to_string(), transformed_value);
        }
    }

    proxy_vars
}

/// Transform localhost/127.0.0.1 to host.docker.internal in a proxy URL
///
/// This is necessary because builds execute in containers where localhost
/// refers to the container itself, not the host machine.
fn transform_proxy_url(url: &str) -> Result<String> {
    // Parse the URL
    let mut parsed = url::Url::parse(url).context("Failed to parse proxy URL")?;

    // Check if host is localhost or 127.0.0.1 (case-insensitive)
    let host = parsed.host_str().context("URL has no host")?;

    if host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" {
        parsed
            .set_host(Some("host.docker.internal"))
            .context("Failed to set host to host.docker.internal")?;

        debug!("Transformed proxy URL: {} -> {}", url, parsed.as_str());
    }

    Ok(parsed.to_string())
}

/// Format proxy variables for pack CLI (--env KEY=VALUE format)
pub(crate) fn format_for_pack(vars: &HashMap<String, String>) -> Vec<String> {
    vars.iter()
        .map(|(key, value)| format!("{}={}", key, value))
        .collect()
}

/// Parse environment variables from CLI format to HashMap.
/// Supports both KEY=VALUE and KEY (reads from current environment).
/// Fails if a KEY-only variable is not set in the current environment.
pub(crate) fn parse_env_vars(env: &[String]) -> Result<HashMap<String, String>> {
    let mut result = HashMap::new();

    for env_var in env {
        if let Some((key, value)) = env_var.split_once('=') {
            result.insert(key.to_string(), value.to_string());
        } else {
            // KEY format - read from environment
            if let Ok(value) = std::env::var(env_var) {
                result.insert(env_var.to_string(), value);
            } else {
                bail!(
                    "Environment variable '{}' is not set in current environment",
                    env_var
                );
            }
        }
    }

    Ok(result)
}

/// Check if any proxy variable values reference host.docker.internal,
/// indicating that --add-host host.docker.internal:host-gateway is needed.
pub(crate) fn needs_host_gateway(vars: &HashMap<String, String>) -> bool {
    vars.values().any(|v| v.contains("host.docker.internal"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_localhost_transformation() {
        let result = transform_proxy_url("http://localhost:3128").unwrap();
        assert_eq!(result, "http://host.docker.internal:3128/");
    }

    #[test]
    fn test_127_0_0_1_transformation() {
        let result = transform_proxy_url("https://127.0.0.1:8080").unwrap();
        assert_eq!(result, "https://host.docker.internal:8080/");
    }

    #[test]
    fn test_localhost_with_port() {
        let result = transform_proxy_url("http://localhost:9999").unwrap();
        assert_eq!(result, "http://host.docker.internal:9999/");
    }

    #[test]
    fn test_localhost_with_path() {
        let result = transform_proxy_url("http://localhost:3128/proxy").unwrap();
        assert_eq!(result, "http://host.docker.internal:3128/proxy");
    }

    #[test]
    fn test_localhost_with_credentials() {
        let result = transform_proxy_url("http://user:pass@localhost:3128").unwrap();
        assert_eq!(result, "http://user:pass@host.docker.internal:3128/");
    }

    #[test]
    fn test_external_url_unchanged() {
        let url = "http://proxy.example.com:8080";
        let result = transform_proxy_url(url).unwrap();
        assert_eq!(result, format!("{}/", url));
    }

    #[test]
    fn test_no_proxy_unchanged() {
        // NO_PROXY values should not be transformed
        let mut vars = HashMap::new();
        vars.insert("NO_PROXY".to_string(), "localhost,127.0.0.1".to_string());

        let formatted = format_for_pack(&vars);
        assert_eq!(formatted.len(), 1);
        assert_eq!(formatted[0], "NO_PROXY=localhost,127.0.0.1");
    }

    #[test]
    fn test_invalid_url_fallback() {
        // Invalid URLs should return an error
        let result = transform_proxy_url("not a valid url");
        assert!(result.is_err());
    }

    #[test]
    fn test_format_for_pack() {
        let mut vars = HashMap::new();
        vars.insert("HTTP_PROXY".to_string(), "http://proxy:3128".to_string());
        vars.insert("HTTPS_PROXY".to_string(), "https://proxy:3128".to_string());

        let formatted = format_for_pack(&vars);
        assert_eq!(formatted.len(), 2);
        assert!(formatted.contains(&"HTTP_PROXY=http://proxy:3128".to_string()));
        assert!(formatted.contains(&"HTTPS_PROXY=https://proxy:3128".to_string()));
    }

    #[test]
    fn test_parse_env_vars_key_value() {
        let env = vec!["FOO=bar".to_string(), "BAZ=qux".to_string()];
        let result = parse_env_vars(&env).unwrap();
        assert_eq!(result.get("FOO").unwrap(), "bar");
        assert_eq!(result.get("BAZ").unwrap(), "qux");
    }

    #[test]
    fn test_parse_env_vars_key_only() {
        std::env::set_var("TEST_PARSE_ENV_KEY", "from_env");
        let env = vec!["TEST_PARSE_ENV_KEY".to_string()];
        let result = parse_env_vars(&env).unwrap();
        assert_eq!(result.get("TEST_PARSE_ENV_KEY").unwrap(), "from_env");
        std::env::remove_var("TEST_PARSE_ENV_KEY");
    }

    #[test]
    fn test_parse_env_vars_missing_key() {
        std::env::remove_var("DEFINITELY_NOT_SET_12345");
        let env = vec!["DEFINITELY_NOT_SET_12345".to_string()];
        let result = parse_env_vars(&env);
        assert!(result.is_err());
    }

    #[test]
    fn test_needs_host_gateway_true() {
        let mut vars = HashMap::new();
        vars.insert(
            "HTTP_PROXY".to_string(),
            "http://host.docker.internal:3128/".to_string(),
        );
        assert!(needs_host_gateway(&vars));
    }

    #[test]
    fn test_needs_host_gateway_false() {
        let mut vars = HashMap::new();
        vars.insert(
            "HTTP_PROXY".to_string(),
            "http://proxy.example.com:3128/".to_string(),
        );
        assert!(!needs_host_gateway(&vars));
    }

    #[test]
    fn test_needs_host_gateway_empty() {
        let vars = HashMap::new();
        assert!(!needs_host_gateway(&vars));
    }
}
