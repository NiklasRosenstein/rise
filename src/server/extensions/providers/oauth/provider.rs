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
    /// Internal URL for backend-to-backend calls (used for OIDC issuer env var)
    pub internal_url: String,
}

#[allow(dead_code)]
pub struct OAuthProvider {
    db_pool: PgPool,
    encryption_provider: Arc<dyn EncryptionProvider>,
    http_client: reqwest::Client,
    api_domain: String,
    internal_url: String,
}

impl Clone for OAuthProvider {
    fn clone(&self) -> Self {
        Self {
            db_pool: self.db_pool.clone(),
            encryption_provider: self.encryption_provider.clone(),
            http_client: self.http_client.clone(),
            api_domain: self.api_domain.clone(),
            internal_url: self.internal_url.clone(),
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
            internal_url: config.internal_url,
        }
    }

    /// Compute the redirect URI for this OAuth extension
    #[allow(dead_code)]
    pub fn compute_redirect_uri(&self, project_name: &str, extension_name: &str) -> String {
        format!(
            "{}/oidc/{}/{}/callback",
            self.api_domain.trim_end_matches('/'),
            project_name,
            extension_name
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

        // New naming pattern: {EXT_NAME}_{KEY}
        let normalized_name = ext.extension.to_uppercase().replace('-', "_");
        let rise_client_id_ref = format!("{}_CLIENT_ID", normalized_name);
        let rise_client_secret_ref_name = format!("{}_CLIENT_SECRET", normalized_name);
        let issuer_ref = format!("{}_ISSUER", normalized_name);

        // Delete associated Rise client ID env var
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

        // Delete associated Rise client secret env var (use the new naming pattern)
        if let Err(e) = db_env_vars::delete_project_env_var(
            &self.db_pool,
            ext.project_id,
            &rise_client_secret_ref_name,
        )
        .await
        {
            warn!(
                "Failed to delete Rise client secret env var {} for OAuth extension: {:?}",
                rise_client_secret_ref_name, e
            );
        } else {
            info!(
                "Deleted Rise client secret env var {} for OAuth extension",
                rise_client_secret_ref_name
            );
        }

        // Also try to delete with the spec's stored ref name (for backwards compatibility during migration)
        if let Some(ref rise_client_secret_ref) = spec.rise_client_secret_ref {
            if *rise_client_secret_ref != rise_client_secret_ref_name {
                if let Err(e) = db_env_vars::delete_project_env_var(
                    &self.db_pool,
                    ext.project_id,
                    rise_client_secret_ref,
                )
                .await
                {
                    warn!(
                        "Failed to delete legacy Rise client secret env var {} for OAuth extension: {:?}",
                        rise_client_secret_ref, e
                    );
                }
            }
        }

        // Delete issuer env var
        if let Err(e) =
            db_env_vars::delete_project_env_var(&self.db_pool, ext.project_id, &issuer_ref).await
        {
            warn!(
                "Failed to delete issuer env var {} for OAuth extension: {:?}",
                issuer_ref, e
            );
        } else {
            info!("Deleted issuer env var {} for OAuth extension", issuer_ref);
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

        // Normalize extension name for env var (uppercase, replace hyphens with underscores)
        // New naming pattern: {EXT_NAME}_{KEY} (e.g., OAUTH_DEX_CLIENT_ID)
        let normalized_name = extension_name.to_uppercase().replace('-', "_");
        let rise_client_id_ref = format!("{}_CLIENT_ID", normalized_name);
        let rise_client_secret_ref_name = format!("{}_CLIENT_SECRET", normalized_name);
        let issuer_ref = format!("{}_ISSUER", normalized_name);

        if spec.rise_client_id.is_none() || spec.rise_client_secret_ref.is_none() {
            info!(
                "Generating Rise client credentials for OAuth extension {}/{}",
                project_id, extension_name
            );

            // Generate credentials with deterministic client ID: {project_name}-{extension_name}
            let rise_client_id = format!("{}-{}", project.name, extension_name);
            let rise_client_secret = generate_rise_client_secret();

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
                &rise_client_secret_ref_name,
                &rise_client_secret_encrypted,
                true,  // is_secret
                false, // not protected - managed by Rise
            )
            .await
            .context("Failed to store Rise client secret as environment variable")?;

            info!(
                "Stored Rise client credentials as environment variables: {}, {}",
                rise_client_id_ref, rise_client_secret_ref_name
            );

            // Update spec with Rise credentials
            spec.rise_client_id = Some(rise_client_id);
            spec.rise_client_secret_ref = Some(rise_client_secret_ref_name.clone());
            spec.rise_client_secret_encrypted = Some(rise_client_secret_encrypted.clone());

            spec_updated = true;
        } else {
            // Restore Rise client ID env var if missing (uses new naming pattern)
            if let Some(ref rise_client_id) = spec.rise_client_id {
                // Check if env var exists with new naming
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

        // Inject issuer env var for OIDC discovery (id_token validation)
        // Point to Rise's OIDC proxy using internal URL (for backend-to-backend calls)
        let rise_issuer = format!(
            "{}/oidc/{}/{}",
            self.internal_url.trim_end_matches('/'),
            project.name,
            extension_name
        );

        db_env_vars::upsert_project_env_var(
            db_pool,
            project_id,
            &issuer_ref,
            &rise_issuer,
            false, // not secret
            false, // not protected
        )
        .await
        .context("Failed to store issuer as environment variable")?;

        debug!("Stored issuer env var {}: {}", issuer_ref, rise_issuer);

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

**Frontend Integration (using oauth4webapi):**

```bash
npm install oauth4webapi
```

```javascript
import * as oauth from 'oauth4webapi';

// 1. Initiate OAuth login with PKCE
async function login() {
  const codeVerifier = oauth.generateRandomCodeVerifier();
  const codeChallenge = await oauth.calculatePKCECodeChallenge(codeVerifier);
  sessionStorage.setItem('pkce_verifier', codeVerifier);

  const authUrl = new URL(
    `https://api.rise.dev/oidc/my-app/oauth-provider/authorize`
  );
  authUrl.searchParams.set('code_challenge', codeChallenge);
  authUrl.searchParams.set('code_challenge_method', 'S256');

  window.location.href = authUrl;
}

// 2. Handle callback and exchange code for tokens
async function handleCallback() {
  const code = new URLSearchParams(window.location.search).get('code');
  const codeVerifier = sessionStorage.getItem('pkce_verifier');

  const tokens = await fetch(
    'https://api.rise.dev/oidc/my-app/oauth-provider/token',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({
        grant_type: 'authorization_code',
        code: code,
        client_id: CONFIG.riseClientId,  // From build-time config
        code_verifier: codeVerifier
      })
    }
  ).then(r => r.json());

  sessionStorage.setItem('tokens', JSON.stringify(tokens));
  return tokens;
}
```

### Token Endpoint Flow (For Backend Apps)

For server-rendered applications, use the RFC 6749-compliant token endpoint with client credentials.
Rise auto-generates client credentials for each OAuth extension.

**Backend Integration (TypeScript/Express):**

```typescript
app.get('/oauth/callback', async (req, res) => {
  const { code } = req.query;

  const tokens = await fetch(
    'https://api.rise.dev/oidc/my-app/oauth-provider/token',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({
        grant_type: 'authorization_code',
        code: code as string,
        client_id: process.env.OAUTH_PROVIDER_CLIENT_ID!,
        client_secret: process.env.OAUTH_PROVIDER_CLIENT_SECRET!
      })
    }
  ).then(r => r.json());

  req.session.tokens = tokens;
  res.redirect('/');
});
```

## Local Development

For local development, add `redirect_uri` parameter:

```javascript
// PKCE flow
authUrl.searchParams.set('redirect_uri', 'http://localhost:3000/callback');

// Backend flow
const authUrl = 'https://api.rise.dev/oidc/my-app/oauth-provider/authorize?redirect_uri=http://localhost:3000/oauth/callback';
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
                },
                "issuer": {
                    "type": "string",
                    "format": "uri",
                    "description": "OIDC issuer URL for id_token validation via JWKS discovery. If not provided, derived from token_endpoint.",
                    "example": "https://accounts.google.com"
                }
            }
        })
    }
}
