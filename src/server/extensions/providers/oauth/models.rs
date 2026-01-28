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
    /// OAuth client ID (for upstream provider)
    pub client_id: String,
    /// Environment variable name containing the client secret (e.g., "OAUTH_SNOWFLAKE_CLIENT_SECRET")
    pub client_secret_ref: String,
    /// OAuth provider authorization URL
    pub authorization_endpoint: String,
    /// OAuth provider token URL
    pub token_endpoint: String,
    /// OAuth scopes to request
    pub scopes: Vec<String>,
    /// Rise client ID (for apps to authenticate to Rise's /token endpoint)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rise_client_id: Option<String>,
    /// Environment variable name containing Rise client secret (e.g., "OAUTH_RISE_CLIENT_SECRET_{extension}")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rise_client_secret_ref: Option<String>,
    /// Encrypted backup of Rise client secret in spec (for restoration)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rise_client_secret_encrypted: Option<String>,
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
    /// OAuth flow type (fragment or exchange)
    pub flow_type: OAuthFlowType,
    /// PKCE code verifier (for upstream OAuth provider)
    pub code_verifier: String,
    /// When this state was created
    pub created_at: DateTime<Utc>,
    /// Client's PKCE code challenge (for Rise's token endpoint validation)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_code_challenge: Option<String>,
    /// Client's PKCE code challenge method
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_code_challenge_method: Option<String>,
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

/// Authorization code state for OAuth 2.0 flow
/// Stores encrypted tokens from upstream provider for single-use exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCodeState {
    /// Project ID
    pub project_id: Uuid,
    /// Extension name
    pub extension_name: String,
    /// When this authorization code was created
    pub created_at: DateTime<Utc>,
    /// PKCE code challenge from client (if PKCE flow)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_challenge: Option<String>,
    /// PKCE code challenge method ("S256" or "plain")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_challenge_method: Option<String>,
    /// Encrypted access token from upstream OAuth provider
    pub access_token_encrypted: String,
    /// Encrypted refresh token (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token_encrypted: Option<String>,
    /// Encrypted ID token (optional, OIDC)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token_encrypted: Option<String>,
    /// Token expiration time
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
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
    /// PKCE code challenge (for public clients/SPAs)
    pub code_challenge: Option<String>,
    /// PKCE code challenge method ("S256" or "plain", defaults to "S256")
    pub code_challenge_method: Option<String>,
}

/// OAuth flow type
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OAuthFlowType {
    /// Exchange token flow (backend exchanges token) - for server-rendered apps
    /// SPAs should use PKCE flow instead (code_challenge parameter)
    #[default]
    Exchange,
}

/// Token request (RFC 6749-compliant)
#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    /// Grant type: "authorization_code" or "refresh_token"
    pub grant_type: String,
    /// Authorization code (for authorization_code grant)
    pub code: Option<String>,
    /// Refresh token (for refresh_token grant)
    pub refresh_token: Option<String>,
    /// Rise client ID (required)
    pub client_id: String,
    /// Rise client secret (for confidential clients)
    pub client_secret: Option<String>,
    /// PKCE code verifier (for public clients)
    pub code_verifier: Option<String>,
}

/// OAuth2 token response (RFC 6749-compliant)
#[derive(Debug, Serialize)]
pub struct OAuth2TokenResponse {
    /// Access token
    pub access_token: String,
    /// Token type (always "Bearer")
    pub token_type: String,
    /// Expires in seconds from now (not timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<i64>,
    /// Refresh token (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Scope (space-delimited from spec.scopes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// ID token (optional, OIDC)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
}

/// OAuth2 error response (RFC 6749-compliant)
#[derive(Debug, Serialize)]
pub struct OAuth2ErrorResponse {
    /// Error code
    pub error: String,
    /// Error description (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
}
