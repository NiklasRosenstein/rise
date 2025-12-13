use serde::{Deserialize, Serialize};

/// Request to set or update an environment variable
#[derive(Debug, Deserialize)]
pub struct SetEnvVarRequest {
    pub value: String,
    #[serde(default)]
    pub is_secret: bool,
}

/// API response for a single environment variable
/// Secrets are always masked in the response
#[derive(Debug, Serialize)]
pub struct EnvVarResponse {
    pub key: String,
    pub value: String, // Masked as "••••••••" if is_secret = true
    pub is_secret: bool,
}

impl EnvVarResponse {
    /// Create response from database model, masking secrets
    pub fn from_db_model(key: String, value: String, is_secret: bool) -> Self {
        let displayed_value = if is_secret {
            "••••••••".to_string()
        } else {
            value
        };

        Self {
            key,
            value: displayed_value,
            is_secret,
        }
    }
}

/// Response containing multiple environment variables
#[derive(Debug, Serialize)]
pub struct EnvVarsResponse {
    pub env_vars: Vec<EnvVarResponse>,
}
