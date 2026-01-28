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
use tracing::{debug, error, info, warn};
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

impl Clone for OAuthProvider {
    fn clone(&self) -> Self {
        Self {
            db_pool: self.db_pool.clone(),
            encryption_provider: self.encryption_provider.clone(),
            http_client: self.http_client.clone(),
            api_domain: self.api_domain.clone(),
        }
    }
}

/// Generate a secure random token for Rise client secret (32 bytes, base64url encoded)
fn generate_rise_client_secret() -> String {
    use base64::Engine;
    use rand::Rng;

    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
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
            "{}/api/v1/oauth/callback/{}/{}",
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

    /// Handle deletion of an OAuth extension
    async fn reconcile_deletion(&self, ext: crate::db::models::ProjectExtension) -> Result<()> {
        use crate::db::{env_vars as db_env_vars, extensions as db_extensions};

        info!(
            "Reconciling deletion for OAuth extension: project_id={}, extension={}",
            ext.project_id, ext.extension
        );

        // Parse spec to get client_secret_ref
        let spec: OAuthExtensionSpec =
            serde_json::from_value(ext.spec).context("Failed to parse OAuth extension spec")?;

        // Delete associated environment variable (upstream OAuth client secret)
        if !spec.client_secret_ref.is_empty() {
            if let Err(e) = db_env_vars::delete_project_env_var(
                &self.db_pool,
                ext.project_id,
                &spec.client_secret_ref,
            )
            .await
            {
                warn!(
                    "Failed to delete environment variable {} for OAuth extension: {:?}",
                    spec.client_secret_ref, e
                );
            } else {
                info!(
                    "Deleted environment variable {} for OAuth extension",
                    spec.client_secret_ref
                );
            }
        }

        // Delete associated Rise client ID env var
        let rise_client_id_ref = format!("OAUTH_RISE_CLIENT_ID_{}", ext.extension.to_uppercase());
        if let Err(e) =
            db_env_vars::delete_project_env_var(&self.db_pool, ext.project_id, &rise_client_id_ref)
                .await
        {
            warn!(
                "Failed to delete Rise client ID env var {} for OAuth extension: {:?}",
                rise_client_id_ref, e
            );
        } else {
            info!(
                "Deleted Rise client ID env var {} for OAuth extension",
                rise_client_id_ref
            );
        }

        // Delete associated Rise client secret env var
        if let Some(ref rise_client_secret_ref) = spec.rise_client_secret_ref {
            if let Err(e) = db_env_vars::delete_project_env_var(
                &self.db_pool,
                ext.project_id,
                rise_client_secret_ref,
            )
            .await
            {
                warn!(
                    "Failed to delete Rise client secret env var {} for OAuth extension: {:?}",
                    rise_client_secret_ref, e
                );
            } else {
                info!(
                    "Deleted Rise client secret env var {} for OAuth extension",
                    rise_client_secret_ref
                );
            }
        }

        // Permanently delete the extension
        db_extensions::delete_permanently(&self.db_pool, ext.project_id, &ext.extension)
            .await
            .context("Failed to permanently delete OAuth extension")?;

        info!(
            "Permanently deleted OAuth extension: project_id={}, extension={}",
            ext.project_id, ext.extension
        );

        Ok(())
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

        // Parse the new spec to check/generate Rise client credentials
        let mut spec: OAuthExtensionSpec = serde_json::from_value(new_spec.clone())
            .context("Invalid OAuth extension spec format")?;

        // Generate Rise client credentials if they don't exist
        let mut spec_updated = false;
        if spec.rise_client_id.is_none() || spec.rise_client_secret_ref.is_none() {
            info!(
                "Generating Rise client credentials for OAuth extension {}/{}",
                project_id, extension_name
            );

            // Generate credentials
            let rise_client_id = Uuid::new_v4().to_string();
            let rise_client_secret = generate_rise_client_secret();
            let rise_client_id_ref =
                format!("OAUTH_RISE_CLIENT_ID_{}", extension_name.to_uppercase());
            let rise_client_secret_ref =
                format!("OAUTH_RISE_CLIENT_SECRET_{}", extension_name.to_uppercase());

            // Encrypt the client secret
            let rise_client_secret_encrypted = self
                .encryption_provider
                .encrypt(&rise_client_secret)
                .await
                .context("Failed to encrypt Rise client secret")?;

            // Store client ID as environment variable (plaintext, not secret)
            db_env_vars::upsert_project_env_var(
                db_pool,
                project_id,
                &rise_client_id_ref,
                &rise_client_id,
                false, // not secret - client ID is public
                false, // not protected - managed by Rise
            )
            .await
            .context("Failed to store Rise client ID as environment variable")?;

            // Store client secret as environment variable (encrypted, secret)
            db_env_vars::upsert_project_env_var(
                db_pool,
                project_id,
                &rise_client_secret_ref,
                &rise_client_secret_encrypted,
                true,  // is_secret
                false, // not protected - managed by Rise
            )
            .await
            .context("Failed to store Rise client secret as environment variable")?;

            info!(
                "Stored Rise client credentials as environment variables: {}, {}",
                rise_client_id_ref, rise_client_secret_ref
            );

            // Update spec with Rise credentials
            spec.rise_client_id = Some(rise_client_id);
            spec.rise_client_secret_ref = Some(rise_client_secret_ref);
            spec.rise_client_secret_encrypted = Some(rise_client_secret_encrypted.clone());

            spec_updated = true;
        } else {
            // Restore Rise client ID env var if missing
            if let Some(ref rise_client_id) = spec.rise_client_id {
                let rise_client_id_ref =
                    format!("OAUTH_RISE_CLIENT_ID_{}", extension_name.to_uppercase());

                // Check if env var exists
                let env_vars = db_env_vars::list_project_env_vars(db_pool, project_id)
                    .await
                    .unwrap_or_default();
                let env_var_exists = env_vars.iter().any(|v| v.key == rise_client_id_ref);

                if !env_var_exists {
                    warn!(
                        "Rise client ID env var {} missing, restoring from spec",
                        rise_client_id_ref
                    );

                    db_env_vars::upsert_project_env_var(
                        db_pool,
                        project_id,
                        &rise_client_id_ref,
                        rise_client_id,
                        false, // not secret
                        false, // not protected
                    )
                    .await
                    .context("Failed to restore Rise client ID")?;

                    info!("Restored Rise client ID: {}", rise_client_id_ref);
                }
            }

            // Restore Rise client secret env var if missing
            if let Some(ref rise_client_secret_ref) = spec.rise_client_secret_ref {
                // Check if env var exists
                let env_vars = db_env_vars::list_project_env_vars(db_pool, project_id)
                    .await
                    .unwrap_or_default();
                let env_var_exists = env_vars.iter().any(|v| v.key == *rise_client_secret_ref);

                if !env_var_exists {
                    // Env var is missing, restore from encrypted backup
                    if let Some(ref encrypted) = spec.rise_client_secret_encrypted {
                        warn!(
                            "Rise client secret env var {} missing, restoring from backup",
                            rise_client_secret_ref
                        );

                        db_env_vars::upsert_project_env_var(
                            db_pool,
                            project_id,
                            rise_client_secret_ref,
                            encrypted,
                            true,  // is_secret
                            false, // not protected
                        )
                        .await
                        .context("Failed to restore Rise client secret")?;

                        info!("Restored Rise client secret: {}", rise_client_secret_ref);
                    } else {
                        // Both env var and backup missing - regenerate
                        warn!("Rise client secret and backup both missing, regenerating");
                        let rise_client_secret = generate_rise_client_secret();
                        let rise_client_secret_encrypted = self
                            .encryption_provider
                            .encrypt(&rise_client_secret)
                            .await
                            .context("Failed to encrypt Rise client secret")?;

                        db_env_vars::upsert_project_env_var(
                            db_pool,
                            project_id,
                            rise_client_secret_ref,
                            &rise_client_secret_encrypted,
                            true,  // is_secret
                            false, // not protected
                        )
                        .await
                        .context("Failed to store Rise client secret")?;

                        spec.rise_client_secret_encrypted = Some(rise_client_secret_encrypted);
                        spec_updated = true;
                    }
                }
            }
        }

        // If spec was updated with Rise credentials, persist it
        if spec_updated {
            let updated_spec_value =
                serde_json::to_value(&spec).context("Failed to serialize updated OAuth spec")?;

            db_extensions::upsert(
                db_pool,
                project_id,
                extension_name,
                "oauth",
                &updated_spec_value,
            )
            .await
            .context("Failed to update extension spec with Rise credentials")?;

            info!(
                "Updated extension spec with Rise client credentials for {}/{}",
                project_id, extension_name
            );
        }

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
        let provider = self.clone();

        tokio::spawn(async move {
            info!("Starting OAuth provider reconciliation loop");

            loop {
                match crate::db::extensions::list_by_extension_type(&provider.db_pool, "oauth")
                    .await
                {
                    Ok(extensions) => {
                        for ext in extensions {
                            // Handle deleted extensions
                            if ext.deleted_at.is_some() {
                                if let Err(e) = provider.reconcile_deletion(ext).await {
                                    error!("Failed to reconcile OAuth extension deletion: {:?}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to list OAuth extensions: {:?}", e);
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        });
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

The extension supports RFC 6749-compliant OAuth flows with PKCE support:

### PKCE Flow (Recommended for SPAs)

For single-page applications, use PKCE (RFC 7636) to securely exchange authorization codes
for tokens. This prevents authorization code interception attacks.

**Frontend Integration:**

```javascript
// Generate PKCE verifier and challenge
async function generatePKCE() {
  const verifier = generateRandomString(128);
  const hashed = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier));
  const challenge = base64urlEncode(hashed);
  return { verifier, challenge };
}

// Initiate OAuth login with PKCE
async function login() {
  const { verifier, challenge } = await generatePKCE();
  sessionStorage.setItem('pkce_verifier', verifier);

  const authUrl = 'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-provider/oauth/authorize' +
    `?code_challenge=${challenge}&code_challenge_method=S256`;
  window.location.href = authUrl;
}

// Handle callback and exchange code for tokens
async function handleCallback() {
  const code = new URLSearchParams(window.location.search).get('code');
  const verifier = sessionStorage.getItem('pkce_verifier');
  const clientId = 'OAUTH_RISE_CLIENT_ID_oauth-provider'; // From env vars

  const response = await fetch(
    'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-provider/oauth/token',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({
        grant_type: 'authorization_code',
        code: code,
        client_id: clientId,
        code_verifier: verifier
      })
    }
  );

  const tokens = await response.json();
  sessionStorage.setItem('access_token', tokens.access_token);
  return tokens;
}
```

### Token Endpoint Flow (For Backend Apps)

For server-rendered applications, use the RFC 6749-compliant token endpoint with client credentials.
Rise auto-generates client credentials for each OAuth extension.

**Backend Integration (Node.js/Express example):**

```javascript
// Initiate OAuth login with exchange flow
app.get('/login', (req, res) => {
  const authUrl = 'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-provider/oauth/authorize?flow=exchange';
  res.redirect(authUrl);
});

// Handle OAuth callback
app.get('/oauth/callback', async (req, res) => {
  const code = req.query.code; // Authorization code from callback

  // Get Rise client credentials from environment
  const clientId = process.env.OAUTH_RISE_CLIENT_ID_oauth_provider;
  const clientSecret = process.env.OAUTH_RISE_CLIENT_SECRET_oauth_provider;

  // Exchange authorization code for tokens
  const response = await fetch(
    'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-provider/oauth/token',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({
        grant_type: 'authorization_code',
        code: code,
        client_id: clientId,
        client_secret: clientSecret
      })
    }
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
// PKCE flow (for SPAs)
const authUrl = 'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-provider/oauth/authorize?code_challenge=' + codeChallenge + '&code_challenge_method=S256&redirect_uri=http://localhost:3000/callback';

// Token endpoint flow (for backend apps)
const authUrl = 'https://api.rise.dev/api/v1/projects/my-app/extensions/oauth-provider/oauth/authorize?redirect_uri=http://localhost:3000/oauth/callback';
```

## Security Features

- Client secrets stored encrypted in database
- PKCE (RFC 7636): Prevents authorization code interception for public clients
- Rise client credentials: Auto-generated per extension for token endpoint authentication
- Temporary single-use authorization codes with 5-minute TTL
- Redirect URI validation (localhost or project domains only)
- CSRF protection via state tokens
- Constant-time comparison for client secret and PKCE validation
- Stateless OAuth proxy: clients own their tokens after exchange
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
