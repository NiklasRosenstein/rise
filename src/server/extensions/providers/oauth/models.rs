use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Extension spec - user-provided OAuth configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthExtensionSpec {
    /// Display name (e.g., "Snowflake Production", "Google OAuth")
    pub provider_name: String,
    /// Description of this OAuth configuration
    #[serde(default)]
    pub description: String,
    /// OAuth client ID
    pub client_id: String,
    /// Environment variable name containing the client secret (e.g., "OAUTH_SNOWFLAKE_CLIENT_SECRET")
    pub client_secret_ref: String,
    /// OAuth provider authorization URL
    pub authorization_endpoint: String,
    /// OAuth provider token URL
    pub token_endpoint: String,
    /// OAuth scopes to request
    pub scopes: Vec<String>,
}

/// Extension status - system-computed metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OAuthExtensionStatus {
    /// Computed redirect URI: https://api.{domain}/oauth/callback/{project}/{extension}
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    /// When the extension was configured
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured_at: Option<DateTime<Utc>>,
    /// Configuration or validation errors
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// OAuth state stored temporarily during authorization flow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthState {
    /// Final redirect destination after OAuth completes (localhost or project URL)
    pub redirect_uri: Option<String>,
    /// Application's CSRF state parameter (passed through to final redirect)
    pub application_state: Option<String>,
    /// Project name
    pub project_name: String,
    /// Extension name (e.g., "oauth-snowflake")
    pub extension_name: String,
    /// Session ID from cookie (if present)
    pub session_id: Option<String>,
    /// When this state was created
    pub created_at: DateTime<Utc>,
}

/// Token response from OAuth provider
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
}

/// Request to initiate OAuth authorization
#[derive(Debug, Deserialize)]
pub struct AuthorizeRequest {
    /// Where to redirect after OAuth completes (optional, for local dev)
    pub redirect_uri: Option<String>,
    /// Application's CSRF state parameter (passed through)
    pub state: Option<String>,
}

/// OAuth callback request from provider
#[derive(Debug, Deserialize)]
pub struct CallbackRequest {
    /// Authorization code from provider
    pub code: String,
    /// State token for CSRF protection
    pub state: String,
}

/// Response containing OAuth credentials
#[derive(Debug, Serialize)]
pub struct CredentialsResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

/// User OAuth token record from database
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserOAuthToken {
    pub id: Uuid,
    pub project_id: Uuid,
    pub extension: String,
    pub session_id: String,
    pub access_token_encrypted: String,
    pub refresh_token_encrypted: Option<String>,
    pub id_token_encrypted: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_refreshed_at: Option<DateTime<Utc>>,
    pub last_accessed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
