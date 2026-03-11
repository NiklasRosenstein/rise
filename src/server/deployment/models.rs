use super::controller::DeploymentUrls;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum DeploymentStatus {
    // Build/Deploy states
    #[default]
    Pending,
    Building,
    Pushing,
    Pushed, // Handoff point between CLI and controller
    Deploying,

    // Running states
    Healthy,
    Unhealthy,

    // Cancellation states
    Cancelling,
    Cancelled,

    // Termination states
    Terminating,
    Stopped,
    Superseded,

    // Terminal states
    Failed,
    Expired,
}

impl std::fmt::Display for DeploymentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeploymentStatus::Pending => write!(f, "Pending"),
            DeploymentStatus::Building => write!(f, "Building"),
            DeploymentStatus::Pushing => write!(f, "Pushing"),
            DeploymentStatus::Pushed => write!(f, "Pushed"),
            DeploymentStatus::Deploying => write!(f, "Deploying"),
            DeploymentStatus::Healthy => write!(f, "Healthy"),
            DeploymentStatus::Unhealthy => write!(f, "Unhealthy"),
            DeploymentStatus::Cancelling => write!(f, "Cancelling"),
            DeploymentStatus::Cancelled => write!(f, "Cancelled"),
            DeploymentStatus::Terminating => write!(f, "Terminating"),
            DeploymentStatus::Stopped => write!(f, "Stopped"),
            DeploymentStatus::Superseded => write!(f, "Superseded"),
            DeploymentStatus::Failed => write!(f, "Failed"),
            DeploymentStatus::Expired => write!(f, "Expired"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Deployment {
    #[serde(default)]
    pub id: String,
    pub deployment_id: String,
    pub project: String,          // Project ID
    pub created_by: String,       // User ID
    pub created_by_email: String, // User email for display
    #[serde(default)]
    pub status: DeploymentStatus,
    #[serde(default = "default_group")]
    pub deployment_group: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>, // RFC3339 timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_logs: Option<String>,
    #[serde(default)]
    pub controller_metadata: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_domain_urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_digest: Option<String>,
    #[serde(default)]
    pub http_port: u16,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
}

fn default_group() -> String {
    DEFAULT_DEPLOYMENT_GROUP.to_string()
}

/// The default deployment group name
/// This group drives the overall project status and is used for primary deployments
pub const DEFAULT_DEPLOYMENT_GROUP: &str = "default";

/// Normalize a deployment group name for use in URLs and resource names.
///
/// Replaces sequences of characters that are not alphanumeric, `-`, `_`, or `.`
/// with `--` (e.g., `mr/123` → `mr--123`). The result is also trimmed so it
/// starts and ends with an alphanumeric character, satisfying the Kubernetes
/// label value regex: `(([A-Za-z0-9][-A-Za-z0-9_.]*)?[A-Za-z0-9])?`
///
/// This matches the normalization used in the `{deployment_group}` placeholder
/// of `staging_ingress_url_template`.
pub fn normalize_deployment_group(deployment_group: &str) -> String {
    let mut result = String::new();
    let mut last_was_invalid = false;

    for ch in deployment_group.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            result.push(ch);
            last_was_invalid = false;
        } else if !last_was_invalid {
            result.push_str("--");
            last_was_invalid = true;
        }
    }

    result
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_string()
}

/// Generate the Rise system environment variables for a deployment.
///
/// Returns `(key, value)` pairs for:
/// - `RISE_ISSUER` — Rise server URL (base URL for all Rise endpoints and JWT issuer)
/// - `RISE_APP_URL` — Canonical URL where the app is accessible
/// - `RISE_APP_URLS` — JSON array of all URLs where the app can be accessed
/// - `RISE_DEPLOYMENT_GROUP` — The deployment group name (e.g. "default", "mr/123")
/// - `RISE_DEPLOYMENT_GROUP_NORMALIZED` — The group name normalized for URLs (e.g. "mr--123")
pub fn rise_system_env_vars(
    public_url: &str,
    deployment_group: &str,
    deployment_urls: &DeploymentUrls,
) -> Vec<(String, String)> {
    let mut all_urls = vec![deployment_urls.default_url.clone()];
    all_urls.extend(deployment_urls.custom_domain_urls.clone());
    let app_urls_json = serde_json::to_string(&all_urls).unwrap_or_else(|_| "[]".to_string());

    vec![
        ("RISE_ISSUER".to_string(), public_url.to_string()),
        (
            "RISE_APP_URL".to_string(),
            deployment_urls.primary_url.clone(),
        ),
        ("RISE_APP_URLS".to_string(), app_urls_json),
        (
            "RISE_DEPLOYMENT_GROUP".to_string(),
            deployment_group.to_string(),
        ),
        (
            "RISE_DEPLOYMENT_GROUP_NORMALIZED".to_string(),
            normalize_deployment_group(deployment_group),
        ),
    ]
}

// Request to create a deployment
#[derive(Debug, Deserialize)]
pub struct CreateDeploymentRequest {
    pub project: String, // Project name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>, // Optional pre-built image reference
    #[serde(default = "default_group")]
    pub group: String, // Deployment group (e.g., 'default', 'mr/27')
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<String>, // Expiration duration (e.g., '7d', '2h', '30m')
    /// HTTP port the application listens on.
    /// If not provided, uses the project's PORT env var or defaults to 8080.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_deployment: Option<String>, // Optional source deployment ID to create from
    #[serde(default)]
    pub use_source_env_vars: bool, // If true and from_deployment is set, copy env vars from source (default: false = use current project env vars)
    #[serde(default)]
    pub push_image: bool, // If true with image, CLI will pull and push image to Rise registry
}

// Response from creating a deployment
#[derive(Debug, Serialize)]
pub struct CreateDeploymentResponse {
    pub deployment_id: String,
    pub image_tag: String, // Full tag: registry_url/namespace/project:deployment_id
    pub credentials: crate::server::registry::models::RegistryCredentials,
}

// Request to update deployment status
#[derive(Debug, Deserialize)]
pub struct UpdateDeploymentStatusRequest {
    pub status: DeploymentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_deployment_group() {
        // Basic cases
        assert_eq!(normalize_deployment_group("default"), "default");
        assert_eq!(normalize_deployment_group("mr/123"), "mr--123");
        assert_eq!(normalize_deployment_group("mr-123"), "mr-123");

        // Leading/trailing invalid chars are trimmed to alphanumeric boundary
        assert_eq!(normalize_deployment_group("/leading"), "leading");
        assert_eq!(normalize_deployment_group("trailing/"), "trailing");
        assert_eq!(normalize_deployment_group("/both/"), "both");

        // Leading/trailing dots and underscores are also trimmed
        assert_eq!(normalize_deployment_group(".dotted."), "dotted");
        assert_eq!(normalize_deployment_group("_underscored_"), "underscored");
        assert_eq!(normalize_deployment_group("_.-mixed-._"), "mixed");

        // Consecutive invalid chars collapse to a single --
        assert_eq!(normalize_deployment_group("mr//123"), "mr--123");
        assert_eq!(normalize_deployment_group("a///b"), "a--b");

        // Empty and all-invalid inputs
        assert_eq!(normalize_deployment_group(""), "");
        assert_eq!(normalize_deployment_group("/"), "");
        assert_eq!(normalize_deployment_group("///"), "");

        // Dots and underscores in the middle are preserved
        assert_eq!(normalize_deployment_group("a.b_c"), "a.b_c");
    }

    #[test]
    fn test_rise_system_env_vars_default_group() {
        let urls = DeploymentUrls {
            default_url: "https://myapp.rise.dev".to_string(),
            primary_url: "https://myapp.rise.dev".to_string(),
            custom_domain_urls: vec![],
        };

        let vars = rise_system_env_vars("https://rise.dev", "default", &urls);

        let map: std::collections::HashMap<_, _> = vars.into_iter().collect();
        assert_eq!(map["RISE_ISSUER"], "https://rise.dev");
        assert_eq!(map["RISE_APP_URL"], "https://myapp.rise.dev");
        assert_eq!(map["RISE_APP_URLS"], r#"["https://myapp.rise.dev"]"#);
        assert_eq!(map["RISE_DEPLOYMENT_GROUP"], "default");
        assert_eq!(map["RISE_DEPLOYMENT_GROUP_NORMALIZED"], "default");
    }

    #[test]
    fn test_rise_system_env_vars_custom_group_with_domains() {
        let urls = DeploymentUrls {
            default_url: "https://myapp-mr--42.rise.dev".to_string(),
            primary_url: "https://custom.example.com".to_string(),
            custom_domain_urls: vec!["https://custom.example.com".to_string()],
        };

        let vars = rise_system_env_vars("https://rise.dev", "mr/42", &urls);

        let map: std::collections::HashMap<_, _> = vars.into_iter().collect();
        assert_eq!(map["RISE_APP_URL"], "https://custom.example.com");
        assert_eq!(
            map["RISE_APP_URLS"],
            r#"["https://myapp-mr--42.rise.dev","https://custom.example.com"]"#
        );
        assert_eq!(map["RISE_DEPLOYMENT_GROUP"], "mr/42");
        assert_eq!(map["RISE_DEPLOYMENT_GROUP_NORMALIZED"], "mr--42");
    }
}
