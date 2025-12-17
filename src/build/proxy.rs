// Proxy environment variable handling for build backends

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
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
        if let Ok(value) = std::env::var(var_name) {
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

/// Embed proxy variables into railpack plan.json using the secrets mechanism
///
/// Railpack secrets are defined at the top level as "KEY=VALUE" and referenced
/// in each step by KEY name. The railpack frontend makes these available as
/// environment variables in the build context.
pub(crate) fn embed_as_secrets_in_plan(
    plan_file: &Path,
    vars: &HashMap<String, String>,
) -> Result<()> {
    use serde_json::Value;

    debug!(
        "Embedding {} proxy variables as secrets into {}",
        vars.len(),
        plan_file.display()
    );

    // Read and parse plan.json
    let plan_contents = std::fs::read_to_string(plan_file)
        .with_context(|| format!("Failed to read plan file: {}", plan_file.display()))?;

    let mut plan: Value = serde_json::from_str(&plan_contents)
        .with_context(|| format!("Failed to parse plan.json: {}", plan_file.display()))?;

    // Ensure plan is an object
    if !plan.is_object() {
        anyhow::bail!("plan.json root is not an object");
    }

    let plan_obj = plan.as_object_mut().unwrap();

    // Add top-level "secrets" array with "KEY=VALUE" format
    let secrets_array = if let Some(existing_secrets) = plan_obj.get_mut("secrets") {
        // Merge with existing secrets
        existing_secrets
            .as_array_mut()
            .context("'secrets' field is not an array")?
    } else {
        // Create new secrets array
        plan_obj.insert("secrets".to_string(), Value::Array(vec![]));
        plan_obj.get_mut("secrets").unwrap().as_array_mut().unwrap()
    };

    // Add proxy secrets in "KEY=VALUE" format
    for (key, value) in vars {
        let secret_value = format!("{}={}", key, value);
        secrets_array.push(Value::String(secret_value));
    }

    // Add proxy variable names to each step's "secrets" array
    let steps = plan_obj
        .get_mut("steps")
        .and_then(|s| s.as_array_mut())
        .context("plan.json missing 'steps' array")?;

    if steps.is_empty() {
        anyhow::bail!("plan.json has empty 'steps' array");
    }

    // Add secret references to all steps
    for step in steps {
        if !step.is_object() {
            continue;
        }

        let step_obj = step.as_object_mut().unwrap();

        // Get or create step's secrets array
        let step_secrets = if let Some(existing) = step_obj.get_mut("secrets") {
            existing
                .as_array_mut()
                .context("step 'secrets' field is not an array")?
        } else {
            step_obj.insert("secrets".to_string(), Value::Array(vec![]));
            step_obj.get_mut("secrets").unwrap().as_array_mut().unwrap()
        };

        // Add each proxy variable name to the step's secrets
        for key in vars.keys() {
            step_secrets.push(Value::String(key.clone()));
        }
    }

    // Write modified plan back
    let modified_plan =
        serde_json::to_string_pretty(&plan).context("Failed to serialize modified plan.json")?;

    std::fs::write(plan_file, modified_plan).with_context(|| {
        format!(
            "Failed to write modified plan.json: {}",
            plan_file.display()
        )
    })?;

    debug!("✓ Embedded proxy variables as secrets into railpack plan");

    Ok(())
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
    fn test_plan_json_secrets_embedding() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let plan_file = temp_dir.path().join("plan.json");

        // Create a simple plan.json
        let plan = serde_json::json!({
            "steps": [
                {
                    "commands": ["echo hello"]
                }
            ]
        });

        fs::write(&plan_file, serde_json::to_string_pretty(&plan).unwrap()).unwrap();

        // Embed proxy vars
        let mut vars = HashMap::new();
        vars.insert("HTTP_PROXY".to_string(), "http://proxy:3128".to_string());

        embed_as_secrets_in_plan(&plan_file, &vars).unwrap();

        // Read back and verify
        let modified = fs::read_to_string(&plan_file).unwrap();
        let plan: serde_json::Value = serde_json::from_str(&modified).unwrap();

        // Check top-level secrets
        let secrets = plan["secrets"].as_array().unwrap();
        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets[0], "HTTP_PROXY=http://proxy:3128");

        // Check step secrets
        let step_secrets = plan["steps"][0]["secrets"].as_array().unwrap();
        assert_eq!(step_secrets.len(), 1);
        assert_eq!(step_secrets[0], "HTTP_PROXY");
    }

    #[test]
    fn test_plan_json_secrets_all_steps() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let plan_file = temp_dir.path().join("plan.json");

        // Create a plan.json with multiple steps
        let plan = serde_json::json!({
            "steps": [
                {"commands": ["echo step1"]},
                {"commands": ["echo step2"]},
                {"commands": ["echo step3"]}
            ]
        });

        fs::write(&plan_file, serde_json::to_string_pretty(&plan).unwrap()).unwrap();

        // Embed proxy vars
        let mut vars = HashMap::new();
        vars.insert("HTTP_PROXY".to_string(), "http://proxy:3128".to_string());

        embed_as_secrets_in_plan(&plan_file, &vars).unwrap();

        // Read back and verify all steps have the secret reference
        let modified = fs::read_to_string(&plan_file).unwrap();
        let plan: serde_json::Value = serde_json::from_str(&modified).unwrap();

        let steps = plan["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 3);

        for step in steps {
            let step_secrets = step["secrets"].as_array().unwrap();
            assert_eq!(step_secrets.len(), 1);
            assert_eq!(step_secrets[0], "HTTP_PROXY");
        }
    }
}
