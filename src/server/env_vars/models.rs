use serde::{Deserialize, Serialize};

fn default_protected() -> bool {
    true
}

/// Request to set or update an environment variable
#[derive(Debug, Deserialize)]
pub struct SetEnvVarRequest {
    pub value: String,
    #[serde(default)]
    pub is_secret: bool,
    #[serde(default = "default_protected")]
    pub is_protected: bool,
}

/// API response for a single environment variable
/// Secrets are always masked in the response unless explicitly decrypted
#[derive(Debug, Serialize)]
pub struct EnvVarResponse {
    pub key: String,
    pub value: String, // Masked as "••••••••" if is_secret = true (unless decrypted)
    pub is_secret: bool,
    pub is_protected: bool,
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
