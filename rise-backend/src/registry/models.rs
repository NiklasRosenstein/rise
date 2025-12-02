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
}

/// Configuration for JFrog Artifactory registry
#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactoryConfig {
    /// Artifactory base URL (e.g., "https://mycompany.jfrog.io")
    pub base_url: String,
    /// Docker repository path (e.g., "docker-local")
    pub repository: String,
    /// Optional: Username for authentication
    pub username: Option<String>,
    /// Optional: Password or API token for authentication
    pub password: Option<String>,
    /// Whether to use Docker credential helper instead of static credentials
    #[serde(default)]
    pub use_credential_helper: bool,
}

/// Registry provider configuration
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RegistryConfig {
    Ecr(EcrConfig),
    Artifactory(ArtifactoryConfig),
}
