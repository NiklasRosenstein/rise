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
    /// Whether OAuth flow has been successfully tested
    #[serde(default)]
    pub auth_verified: bool,
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
    /// OAuth flow type (fragment or exchange)
    pub flow_type: OAuthFlowType,
    /// PKCE code verifier (for providers that require PKCE like Snowflake)
    pub code_verifier: String,
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
#[allow(dead_code)]
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

/// Exchange token state for secure backend flow
/// Temporary state linking an exchange token to a user's OAuth session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthExchangeState {
    /// Project ID
    pub project_id: Uuid,
    /// Extension name
    pub extension_name: String,
    /// Session ID from OAuth flow
    pub session_id: String,
    /// When this exchange token was created
    pub created_at: DateTime<Utc>,
}

/// Request to exchange a temporary token for OAuth credentials
#[derive(Debug, Deserialize)]
pub struct ExchangeTokenRequest {
    /// Temporary exchange token (single-use, short TTL)
    pub exchange_token: String,
}

/// Query parameter to enable exchange flow
#[derive(Debug, Deserialize)]
pub struct AuthorizeFlowQuery {
    /// Where to redirect after OAuth completes (optional, for local dev)
    pub redirect_uri: Option<String>,
    /// Application's CSRF state parameter (passed through)
    pub state: Option<String>,
    /// OAuth flow type: "fragment" (default) or "exchange" (for backend apps)
    #[serde(default)]
    pub flow: OAuthFlowType,
}

/// OAuth flow type
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OAuthFlowType {
    /// Fragment-based flow (tokens in URL fragment) - best for SPAs
    #[default]
    Fragment,
    /// Exchange token flow (backend exchanges token) - best for server-rendered apps
    Exchange,
}
