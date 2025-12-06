use serde::{Deserialize, Serialize};

/// Registry credentials response
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RegistryCredentials {
    /// Registry URL (e.g., "123456789.dkr.ecr.us-east-1.amazonaws.com")
    pub registry_url: String,
    /// Username for authentication
    pub username: String,
    /// Password or token for authentication
    pub password: String,
    /// How long the credentials are valid (in seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
}

/// Registry credentials request
#[derive(Debug, Deserialize)]
pub struct GetRegistryCredsRequest {
    /// Project ID or name
    pub project: String,
}

/// Registry credentials response wrapper
#[derive(Debug, Serialize)]
pub struct GetRegistryCredsResponse {
    pub credentials: RegistryCredentials,
    pub repository: String,
}

/// Configuration for AWS ECR registry
#[derive(Debug, Clone, Deserialize)]
pub struct EcrConfig {
    /// AWS region (e.g., "us-east-1")
    pub region: String,
    /// AWS account ID (e.g., "123456789012")
    pub account_id: String,
    /// Optional: AWS access key ID (if not using IAM role)
    pub access_key_id: Option<String>,
    /// Optional: AWS secret access key (if not using IAM role)
    pub secret_access_key: Option<String>,
    /// ECR repository name (e.g., "rise-apps")
    pub repository: String,
    /// Optional prefix within the repository for further organization
    /// Final image path: {repository}/{prefix}/{project}:{tag}
    #[serde(default)]
    pub prefix: String,
    /// IAM role ARN to assume for scoped credentials
    /// Backend assumes this role with inline policy restricting to specific repos
    pub push_role_arn: String,
}

/// Configuration for OCI registry with client-side authentication
///
/// This provider is for OCI-compliant registries where the client has already
/// authenticated (e.g., via `docker login`). The backend only provides the
/// registry URL and namespace; credentials are managed by the client's Docker config.
#[derive(Debug, Clone, Deserialize)]
pub struct OciClientAuthConfig {
    /// Registry URL (e.g., "localhost:5000", "registry.example.com")
    pub registry_url: String,
    /// Namespace/path within registry (e.g., "rise-apps", "myorg")
    #[serde(default = "default_namespace")]
    pub namespace: String,
}

fn default_namespace() -> String {
    String::new()
}

/// Registry provider configuration
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RegistryConfig {
    Ecr(EcrConfig),
    #[serde(rename = "oci-client-auth", alias = "docker")]
    OciClientAuth(OciClientAuthConfig),
}
