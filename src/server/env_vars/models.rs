use serde::{Deserialize, Serialize};

/// Request to set or update an environment variable
#[derive(Debug, Deserialize)]
pub struct SetEnvVarRequest {
    pub value: String,
    #[serde(default)]
    pub is_secret: bool,
    /// When omitted, defaults to the same value as is_secret (secrets are protected by default)
    pub is_protected: Option<bool>,
}

/// Request to move an environment variable to a different environment
#[derive(Debug, Deserialize)]
pub struct MoveEnvVarRequest {
    /// Source environment name (null for global)
    pub from_environment: Option<String>,
    /// Target environment name (null for global)
    pub to_environment: Option<String>,
}

/// API response for a single environment variable
/// Secrets are always masked in the response unless explicitly decrypted
#[derive(Debug, Serialize)]
pub struct EnvVarResponse {
    pub key: String,
    pub value: String, // Masked as "••••••••" if is_secret = true (unless decrypted)
    pub is_secret: bool,
    pub is_protected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl EnvVarResponse {
    /// Create response from database model, masking secrets
    pub fn from_db_model(key: String, value: String, is_secret: bool, is_protected: bool) -> Self {
        let displayed_value = if is_secret {
            "••••••••".to_string()
        } else {
            value
        };

        Self {
            key,
            value: displayed_value,
            is_secret,
            is_protected,
            environment: None,
            source: None,
        }
    }
}

/// Response containing multiple environment variables
#[derive(Debug, Serialize)]
pub struct EnvVarsResponse {
    pub env_vars: Vec<EnvVarResponse>,
}

/// Response for retrieving a single secret value
#[derive(Debug, Serialize)]
pub struct EnvVarValueResponse {
    pub value: String,
}
