use crate::db::env_vars as db_env_vars;
use crate::server::encryption::EncryptionProvider;
use crate::server::extensions::providers::oauth::models::{
    OAuthExtensionSpec, OAuthExtensionStatus, TokenResponse,
};
use crate::server::extensions::Extension;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{debug, error, info};
use url::Url;
use uuid::Uuid;

pub struct OAuthProviderConfig {
    pub db_pool: PgPool,
    pub encryption_provider: Arc<dyn EncryptionProvider>,
    pub http_client: reqwest::Client,
    pub api_domain: String,
}

#[allow(dead_code)]
pub struct OAuthProvider {
    db_pool: PgPool,
    encryption_provider: Arc<dyn EncryptionProvider>,
    http_client: reqwest::Client,
    api_domain: String,
}

impl OAuthProvider {
    pub fn new(config: OAuthProviderConfig) -> Self {
        Self {
            db_pool: config.db_pool,
            encryption_provider: config.encryption_provider,
            http_client: config.http_client,
            api_domain: config.api_domain,
        }
    }

    /// Compute the redirect URI for this OAuth extension
    #[allow(dead_code)]
    pub fn compute_redirect_uri(&self, project_name: &str, extension_name: &str) -> String {
        format!(
            "https://{}/api/v1/oauth/callback/{}/{}",
            self.api_domain, project_name, extension_name
        )
    }

    /// Resolve client_secret from project environment variable
    #[allow(dead_code)]
    pub async fn resolve_client_secret(
        &self,
        project_id: Uuid,
        client_secret_ref: &str,
    ) -> Result<String> {
        // Get all environment variables for the project
        let env_vars = db_env_vars::list_project_env_vars(&self.db_pool, project_id).await?;

        // Find the environment variable by key
        let env_var = env_vars
            .iter()
            .find(|var| var.key == client_secret_ref)
            .ok_or_else(|| {
                anyhow!(
                    "Environment variable '{}' not found for OAuth client secret",
                    client_secret_ref
                )
            })?;

        // Decrypt the value if it's a secret
        let client_secret = if env_var.is_secret {
            self.encryption_provider
                .decrypt(&env_var.value)
                .await
                .context("Failed to decrypt OAuth client secret")?
        } else {
            env_var.value.clone()
        };

        Ok(client_secret)
    }

    /// Exchange authorization code for tokens
    #[allow(dead_code)]
    pub async fn exchange_code_for_tokens(
        &self,
        spec: &OAuthExtensionSpec,
        client_secret: &str,
        authorization_code: &str,
        redirect_uri: &str,
    ) -> Result<TokenResponse> {
        debug!(
            "Exchanging authorization code for tokens with endpoint: {}",
            spec.token_endpoint
        );

        let response = self
            .http_client
            .post(&spec.token_endpoint)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", authorization_code),
                ("client_id", &spec.client_id),
                ("client_secret", client_secret),
                ("redirect_uri", redirect_uri),
            ])
            .send()
            .await
            .context("Failed to send token exchange request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());
            error!(
                "Token exchange failed with status {}: {}",
                status, error_text
            );
            return Err(anyhow!(
                "Token exchange failed with status {}: {}",
                status,
                error_text
            ));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .context("Failed to parse token response")?;

        Ok(token_response)
    }

    /// Refresh an expired token using refresh_token
    #[allow(dead_code)]
    pub async fn refresh_token(
        &self,
        spec: &OAuthExtensionSpec,
        client_secret: &str,
        refresh_token: &str,
    ) -> Result<TokenResponse> {
        debug!("Refreshing token with endpoint: {}", spec.token_endpoint);

        let response = self
            .http_client
            .post(&spec.token_endpoint)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", &spec.client_id),
                ("client_secret", client_secret),
            ])
            .send()
            .await
            .context("Failed to send token refresh request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());
            error!(
                "Token refresh failed with status {}: {}",
                status, error_text
            );
            return Err(anyhow!(
                "Token refresh failed with status {}: {}",
                status,
                error_text
            ));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .context("Failed to parse token refresh response")?;

        Ok(token_response)
    }
}

#[async_trait]
impl Extension for OAuthProvider {
    fn extension_type(&self) -> &str {
        "oauth"
    }

    fn display_name(&self) -> &str {
        "Generic OAuth 2.0"
    }

    async fn validate_spec(&self, spec: &Value) -> Result<()> {
        // Parse and validate the OAuth extension spec
        let spec: OAuthExtensionSpec =
            serde_json::from_value(spec.clone()).context("Invalid OAuth extension spec format")?;

        // Validate required fields
        if spec.client_id.is_empty() {
            return Err(anyhow!("client_id is required"));
        }
        if spec.client_secret_ref.is_empty() {
            return Err(anyhow!("client_secret_ref is required"));
        }
        if spec.authorization_endpoint.is_empty() {
            return Err(anyhow!("authorization_endpoint is required"));
        }
        if spec.token_endpoint.is_empty() {
            return Err(anyhow!("token_endpoint is required"));
        }
        if spec.scopes.is_empty() {
            return Err(anyhow!("at least one scope is required"));
        }

        // Validate URLs
        Url::parse(&spec.authorization_endpoint).context("Invalid authorization_endpoint URL")?;
        Url::parse(&spec.token_endpoint).context("Invalid token_endpoint URL")?;

        Ok(())
    }

    async fn on_spec_updated(
        &self,
        old_spec: &Value,
        new_spec: &Value,
        project_id: Uuid,
        extension_name: &str,
        db_pool: &sqlx::PgPool,
    ) -> Result<()> {
        use crate::db::{extensions as db_extensions, projects};
        use chrono::Utc;

        // Get project name for redirect URI
        let project = projects::find_by_id(db_pool, project_id)
            .await?
            .ok_or_else(|| anyhow!("Project not found"))?;

        // Get current extension to check status
        let ext = db_extensions::find_by_project_and_name(db_pool, project_id, extension_name)
            .await?
            .ok_or_else(|| anyhow!("Extension not found"))?;

        // Parse current status
        let mut status: OAuthExtensionStatus =
            serde_json::from_value(ext.status).unwrap_or_default();

        // Always ensure redirect_uri is set/updated
        let redirect_uri = self.compute_redirect_uri(&project.name, extension_name);
        status.redirect_uri = Some(redirect_uri);

        // Initialize configured_at if not already set
        if status.configured_at.is_none() {
            status.configured_at = Some(Utc::now());
        }

        // Check if auth-sensitive fields changed
        let auth_sensitive_fields = [
            "client_id",
            "client_secret_ref",
            "authorization_endpoint",
            "token_endpoint",
            "scopes",
        ];

        let mut auth_changed = false;
        for field in &auth_sensitive_fields {
            if old_spec.get(field) != new_spec.get(field) {
                auth_changed = true;
                break;
            }
        }

        // Reset auth_verified when critical fields change
        if auth_changed {
            debug!(
                "OAuth spec changed for {}/{}, resetting auth_verified",
                project_id, extension_name
            );
            status.auth_verified = false;
        }

        // Save updated status
        db_extensions::update_status(
            db_pool,
            project_id,
            extension_name,
            &serde_json::to_value(&status)?,
        )
        .await?;

        Ok(())
    }

    fn start(&self) {
        // OAuth extension doesn't need a reconciliation loop
        // Token refresh and cleanup are handled by separate background jobs
        info!("OAuth provider extension started (no reconciliation loop needed)");
    }

    async fn before_deployment(
        &self,
        _deployment_id: Uuid,
        _project_id: Uuid,
        _deployment_group: &str,
    ) -> Result<()> {
        // OAuth extension doesn't inject deployment-specific environment variables
        // The OAuth flow is handled at runtime, not at deployment time
        Ok(())
    }

    fn format_status(&self, status: &Value) -> String {
        let status: OAuthExtensionStatus = match serde_json::from_value(status.clone()) {
            Ok(s) => s,
            Err(_) => return "Invalid status".to_string(),
        };

        if let Some(error) = &status.error {
            return format!("Error: {}", error);
        }

        if let Some(configured_at) = status.configured_at {
            if status.auth_verified {
                format!("Configured ({})", configured_at.format("%Y-%m-%d %H:%M:%S"))
            } else {
                "Waiting For Auth".to_string()
            }
        } else {
            "Not configured".to_string()
        }
    }

    fn description(&self) -> &str {
        "Generic OAuth 2.0 provider for user authentication"
    }

    fn documentation(&self) -> &str {
        r#"# Generic OAuth 2.0 Extension

This extension allows your application to authenticate end users via any OAuth 2.0 provider
(Snowflake, Google, GitHub, custom SSO, etc.) without managing OAuth client secrets locally.

## Configuration

The OAuth extension requires:

1. **OAuth Client Credentials**: Register an OAuth application with your provider to get:
   - `client_id`: OAuth client identifier
   - `client_secret`: OAuth client secret (stored as encrypted environment variable)

2. **Provider Endpoints**:
   - `authorization_endpoint`: OAuth provider's authorization URL
   - `token_endpoint`: OAuth provider's token URL

3. **Scopes**: OAuth scopes to request (provider-specific)

## Setup Steps

### Step 1: Store Client Secret as Environment Variable

```bash
rise env set my-app OAUTH_PROVIDER_SECRET "your_client_secret" --secret
```

### Step 2: Create OAuth Extension

```bash
rise extension create my-app oauth-provider \
  --type oauth \
  --spec '{
    "provider_name": "My OAuth Provider",
    "description": "OAuth authentication for my app",
    "client_id": "your_client_id",
    "client_secret_ref": "OAUTH_PROVIDER_SECRET",
    "authorization_endpoint": "https://provider.com/oauth/authorize",
    "token_endpoint": "https://provider.com/oauth/token",
    "scopes": ["openid", "email", "profile"]
  }'
```

## Provider-Specific Examples

### Snowflake OAuth

```bash
rise extension create my-app oauth-snowflake \
  --type oauth \
  --spec '{
    "provider_name": "Snowflake Production",
    "description": "Snowflake OAuth for analytics",
    "client_id": "ABC123XYZ...",
    "client_secret_ref": "OAUTH_SNOWFLAKE_SECRET",
    "authorization_endpoint": "https://myorg.snowflakecomputing.com/oauth/authorize",
    "token_endpoint": "https://myorg.snowflakecomputing.com/oauth/token-request",
    "scopes": ["refresh_token"]
  }'
```

### Google OAuth

```bash
rise extension create my-app oauth-google \
  --type oauth \
  --spec '{
    "provider_name": "Google",
    "description": "Sign in with Google",
    "client_id": "123456789.apps.googleusercontent.com",
    "client_secret_ref": "OAUTH_GOOGLE_SECRET",
    "authorization_endpoint": "https://accounts.google.com/o/oauth2/v2/auth",
    "token_endpoint": "https://oauth2.googleapis.com/token",
    "scopes": ["openid", "email", "profile"]
  }'
```

### GitHub OAuth

```bash
rise extension create my-app oauth-github \
  --type oauth \
  --spec '{
    "provider_name": "GitHub",
    "description": "Sign in with GitHub",
    "client_id": "Iv1.abc123...",
    "client_secret_ref": "OAUTH_GITHUB_SECRET",
    "authorization_endpoint": "https://github.com/login/oauth/authorize",
    "token_endpoint": "https://github.com/login/oauth/access_token",
    "scopes": ["read:user", "user:email"]
  }'
```

## OAuth Flows

The extension supports two OAuth flows:

### Fragment Flow (Default - Recommended for SPAs)

Tokens are returned in the URL fragment (`#access_token=...`), which never reaches the server.
This is the most secure option for single-page applications.

**Frontend Integration:**

```javascript
// Initiate OAuth login (fragment flow is default)
function login() {
  window.location.href = 'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-provider/oauth/authorize';
}

// Extract tokens from URL fragment after redirect
function extractTokens() {
  const fragment = window.location.hash.substring(1);
  const params = new URLSearchParams(fragment);
  const accessToken = params.get('access_token');
  const idToken = params.get('id_token');
  const expiresAt = params.get('expires_at');

  // Store in sessionStorage or localStorage
  sessionStorage.setItem('access_token', accessToken);

  return { accessToken, idToken, expiresAt };
}
```

### Exchange Token Flow (For Backend Apps)

For server-rendered applications, use the exchange flow. The callback receives a temporary
exchange token as a query parameter, which your backend exchanges for the actual OAuth tokens.

**Backend Integration (Node.js/Express example):**

```javascript
// Initiate OAuth login with exchange flow
app.get('/login', (req, res) => {
  const authUrl = 'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-provider/oauth/authorize?flow=exchange';
  res.redirect(authUrl);
});

// Handle OAuth callback
app.get('/oauth/callback', async (req, res) => {
  const exchangeToken = req.query.exchange_token;

  // Exchange the temporary token for actual OAuth tokens
  const response = await fetch(
    `https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-provider/oauth/exchange?exchange_token=${exchangeToken}`
  );

  const tokens = await response.json();

  // Store tokens in session (HttpOnly cookie recommended)
  req.session.accessToken = tokens.access_token;
  req.session.idToken = tokens.id_token;

  res.redirect('/dashboard');
});
```

### Local Development

For local development, override the redirect URI:

```javascript
// Fragment flow
const authUrl = 'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-provider/oauth/authorize?redirect_uri=http://localhost:3000/callback';

// Exchange flow
const authUrl = 'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-provider/oauth/authorize?flow=exchange&redirect_uri=http://localhost:3000/oauth/callback';
```

## Security Features

- Client secrets stored encrypted in database
- Fragment flow: Tokens in URL fragments (never sent to server)
- Exchange flow: Temporary single-use tokens with 5-minute TTL
- Redirect URI validation (localhost or project domains only)
- CSRF protection via state tokens
- User token caching with automatic refresh
- Session-based token storage with encrypted credentials
- Configurable token retention policies
"#
    }

    fn spec_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": [
                "provider_name",
                "client_id",
                "client_secret_ref",
                "authorization_endpoint",
                "token_endpoint",
                "scopes"
            ],
            "properties": {
                "provider_name": {
                    "type": "string",
                    "description": "Display name for this OAuth provider",
                    "example": "Snowflake Production"
                },
                "description": {
                    "type": "string",
                    "description": "Description of this OAuth configuration",
                    "example": "Snowflake OAuth for analytics access"
                },
                "client_id": {
                    "type": "string",
                    "description": "OAuth client ID",
                    "example": "ABC123XYZ..."
                },
                "client_secret_ref": {
                    "type": "string",
                    "description": "Environment variable name containing the client secret",
                    "example": "OAUTH_SNOWFLAKE_CLIENT_SECRET"
                },
                "authorization_endpoint": {
                    "type": "string",
                    "format": "uri",
                    "description": "OAuth provider authorization URL",
                    "example": "https://myorg.snowflakecomputing.com/oauth/authorize"
                },
                "token_endpoint": {
                    "type": "string",
                    "format": "uri",
                    "description": "OAuth provider token URL",
                    "example": "https://myorg.snowflakecomputing.com/oauth/token-request"
                },
                "scopes": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "OAuth scopes to request",
                    "example": ["refresh_token"]
                }
            }
        })
    }
}
