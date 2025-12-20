use crate::db::{env_vars as db_env_vars, extensions as db_extensions, projects as db_projects};
use crate::server::encryption::EncryptionProvider;
use crate::server::extensions::Extension;
use crate::server::settings::{PrivateKeySource, SnowflakeAuth};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[cfg(feature = "snowflake")]
use snowflake_connector_rs::{SnowflakeAuthMethod, SnowflakeClient, SnowflakeClientConfig};

/// User-facing extension spec - minimal configuration
/// Backend connection credentials are configured in config/default.yaml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnowflakeOAuthProvisionerSpec {
    /// Additional blocked roles (unioned with backend defaults)
    #[serde(default)]
    pub blocked_roles: Vec<String>,

    /// Additional OAuth scopes (unioned with backend defaults)
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// Extension status tracking Snowflake integration state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnowflakeOAuthProvisionerStatus {
    /// Current state in the provisioning lifecycle
    pub state: SnowflakeOAuthState,

    /// Snowflake SECURITY INTEGRATION name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_name: Option<String>,

    /// Name of the created OAuth extension
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_extension_name: Option<String>,

    /// OAuth client ID from Snowflake
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_client_id: Option<String>,

    /// Encrypted OAuth client secret (stored in status like RDS master_password_encrypted)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_client_secret_encrypted: Option<String>,

    /// Redirect URI configured in the integration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,

    /// Last error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Timestamp when the integration was created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
}

/// State machine for Snowflake OAuth provisioning lifecycle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "PascalCase")]
pub enum SnowflakeOAuthState {
    #[default]
    Pending,
    TestingConnection,
    CreatingIntegration,
    RetrievingCredentials,
    CreatingOAuthExtension,
    Available,
    Deleting,
    Deleted,
    Failed,
}

/// Effective configuration merging user spec with backend defaults
#[derive(Debug, Clone)]
struct EffectiveConfig {
    blocked_roles: Vec<String>,
    scopes: Vec<String>,
}

/// Configuration for SnowflakeOAuthProvisioner
pub struct SnowflakeOAuthProvisionerConfig {
    pub db_pool: PgPool,
    pub encryption_provider: Arc<dyn EncryptionProvider>,
    pub http_client: reqwest::Client,
    pub api_domain: String,
    pub oauth_provider: Option<Arc<dyn Extension>>,

    // Backend configuration (from config/default.yaml)
    pub account: String,
    pub user: String,
    pub auth: SnowflakeAuth,
    pub integration_name_prefix: String,
    pub default_blocked_roles: Vec<String>,
    pub default_scopes: Vec<String>,
    pub refresh_token_validity_seconds: i64,
}

/// Main Snowflake OAuth provisioner implementation
pub struct SnowflakeOAuthProvisioner {
    db_pool: PgPool,
    encryption_provider: Arc<dyn EncryptionProvider>,
    http_client: reqwest::Client,
    api_domain: String,
    oauth_provider: Option<Arc<dyn Extension>>,

    // Backend configuration
    account: String,
    user: String,
    auth: SnowflakeAuth,
    integration_name_prefix: String,
    default_blocked_roles: Vec<String>,
    default_scopes: Vec<String>,
    refresh_token_validity_seconds: i64,
}

impl Clone for SnowflakeOAuthProvisioner {
    fn clone(&self) -> Self {
        Self {
            db_pool: self.db_pool.clone(),
            encryption_provider: self.encryption_provider.clone(),
            http_client: self.http_client.clone(),
            api_domain: self.api_domain.clone(),
            oauth_provider: self.oauth_provider.clone(),
            account: self.account.clone(),
            user: self.user.clone(),
            auth: self.auth.clone(),
            integration_name_prefix: self.integration_name_prefix.clone(),
            default_blocked_roles: self.default_blocked_roles.clone(),
            default_scopes: self.default_scopes.clone(),
            refresh_token_validity_seconds: self.refresh_token_validity_seconds,
        }
    }
}

impl SnowflakeOAuthProvisioner {
    pub fn new(config: SnowflakeOAuthProvisionerConfig) -> Self {
        Self {
            db_pool: config.db_pool,
            encryption_provider: config.encryption_provider,
            http_client: config.http_client,
            api_domain: config.api_domain,
            oauth_provider: config.oauth_provider,
            account: config.account,
            user: config.user,
            auth: config.auth,
            integration_name_prefix: config.integration_name_prefix,
            default_blocked_roles: config.default_blocked_roles,
            default_scopes: config.default_scopes,
            refresh_token_validity_seconds: config.refresh_token_validity_seconds,
        }
    }

    /// Merge user spec with backend defaults (union, not override)
    fn get_effective_config(&self, spec: &SnowflakeOAuthProvisionerSpec) -> EffectiveConfig {
        // Union blocked_roles: backend defaults + user additions (deduplicated)
        let mut blocked_roles = self.default_blocked_roles.clone();
        for role in &spec.blocked_roles {
            if !blocked_roles.contains(role) {
                blocked_roles.push(role.clone());
            }
        }

        // Union scopes: backend defaults + user additions (deduplicated)
        let mut scopes = self.default_scopes.clone();
        for scope in &spec.scopes {
            if !scopes.contains(scope) {
                scopes.push(scope.clone());
            }
        }

        EffectiveConfig {
            blocked_roles,
            scopes,
        }
    }

    /// Get the finalizer name for this extension instance
    fn finalizer_name(&self, extension_name: &str) -> String {
        format!(
            "rise.dev/extension/{}/{}",
            self.extension_type(),
            extension_name
        )
    }

    /// Generate Snowflake integration name: {prefix}_{project_name}_{extension_name}
    fn generate_integration_name(&self, project_name: &str, extension_name: &str) -> String {
        let sanitized_project = project_name.replace('-', "_").to_uppercase();
        let sanitized_extension = extension_name.replace('-', "_").to_uppercase();
        format!(
            "{}_{}_{}",
            self.integration_name_prefix.to_uppercase(),
            sanitized_project,
            sanitized_extension
        )
    }

    /// Generate OAuth extension name: {extension_name}-oauth
    fn generate_oauth_extension_name(&self, extension_name: &str) -> String {
        format!("{}-oauth", extension_name)
    }

    /// Create Snowflake client using configured credentials
    #[cfg(feature = "snowflake")]
    fn create_snowflake_client(&self) -> Result<SnowflakeClient> {
        let auth_method = match &self.auth {
            SnowflakeAuth::Password { password } => SnowflakeAuthMethod::Password(password.clone()),
            SnowflakeAuth::PrivateKey {
                key_source,
                private_key_password,
            } => {
                let private_key_pem = match key_source {
                    PrivateKeySource::Path { private_key_path } => {
                        std::fs::read_to_string(private_key_path)
                            .context("Failed to read private key file")?
                    }
                    PrivateKeySource::Inline { private_key } => private_key.clone(),
                };

                // Convert password to Vec<u8>
                let password_bytes = private_key_password
                    .as_ref()
                    .map(|p| p.as_bytes().to_vec())
                    .unwrap_or_default();

                SnowflakeAuthMethod::KeyPair {
                    encrypted_pem: private_key_pem,
                    password: password_bytes,
                }
            }
            SnowflakeAuth::Jwt { .. } => {
                return Err(anyhow!(
                    "JWT authentication is not supported by snowflake-connector-rs v0.4. Use password or private key authentication."
                ));
            }
        };

        // Parse account to extract account locator and cloud region
        // Account format: "account_locator.region" or just "account_locator"
        let account_parts: Vec<&str> = self.account.split('.').collect();
        let account_identifier = account_parts.first()
            .ok_or_else(|| anyhow!("Invalid account format"))?.to_string();

        let config = SnowflakeClientConfig {
            account: account_identifier,
            ..Default::default()
        };

        let client = SnowflakeClient::new(&self.user, auth_method, config)
            .context("Failed to create Snowflake client")?;

        Ok(client)
    }

    /// Execute SQL statement on Snowflake
    #[cfg(feature = "snowflake")]
    async fn execute_sql(&self, sql: &str) -> Result<Vec<Value>> {
        let client = self.create_snowflake_client()?;
        let session = client
            .create_session()
            .await
            .context("Failed to create Snowflake session")?;

        let rows = session
            .query(sql)
            .await
            .context("Failed to execute SQL on Snowflake")?;

        // Convert SnowflakeRow to serde_json::Value
        // For now, we'll construct a basic JSON representation
        let json_rows: Vec<Value> = rows
            .iter()
            .map(|_row| {
                // TODO: Implement proper row-to-JSON conversion using SnowflakeRow API
                // For now, return empty objects as we primarily use this for DDL statements
                json!({})
            })
            .collect();

        Ok(json_rows)
    }

    /// Stub for when snowflake feature is not enabled
    #[cfg(not(feature = "snowflake"))]
    async fn execute_sql(&self, _sql: &str) -> Result<Vec<Value>> {
        Err(anyhow!(
            "Snowflake feature not enabled. Rebuild with --features snowflake"
        ))
    }

    /// Handle Pending state: generate names and add finalizer
    async fn handle_pending(
        &self,
        _spec: &SnowflakeOAuthProvisionerSpec,
        status: &mut SnowflakeOAuthProvisionerStatus,
        project_name: &str,
        project_id: Uuid,
        extension_name: &str,
    ) -> Result<()> {
        info!(
            "Initializing Snowflake OAuth provisioner for project {} (extension: {})",
            project_name, extension_name
        );

        // Generate integration name if not already set
        let integration_name = if let Some(ref existing_name) = status.integration_name {
            existing_name.clone()
        } else {
            self.generate_integration_name(project_name, extension_name)
        };

        // Generate OAuth extension name
        let oauth_extension_name = self.generate_oauth_extension_name(extension_name);

        // Generate redirect URI
        let redirect_uri = format!("{}/api/oauth/callback", self.api_domain);

        // Update status
        status.integration_name = Some(integration_name.clone());
        status.oauth_extension_name = Some(oauth_extension_name);
        status.redirect_uri = Some(redirect_uri);
        status.created_at = Some(Utc::now());
        status.state = SnowflakeOAuthState::TestingConnection;

        // Add finalizer to prevent project deletion during provisioning
        let finalizer = self.finalizer_name(extension_name);
        if let Err(e) = db_projects::add_finalizer(&self.db_pool, project_id, &finalizer).await {
            error!(
                "Failed to add finalizer '{}' for project {}: {}",
                finalizer, project_name, e
            );
        } else {
            info!(
                "Added finalizer '{}' to project {}",
                finalizer, project_name
            );
        }

        Ok(())
    }

    /// Handle TestingConnection state: verify Snowflake credentials
    async fn handle_testing_connection(
        &self,
        status: &mut SnowflakeOAuthProvisionerStatus,
        project_name: &str,
    ) -> Result<()> {
        info!(
            "Testing Snowflake connection for project {} (account: {})",
            project_name, self.account
        );

        // Test the connection with a simple query
        let test_query = "SELECT CURRENT_VERSION() as version, CURRENT_ACCOUNT() as account";

        match self.execute_sql(test_query).await {
            Ok(rows) => {
                info!(
                    "Successfully connected to Snowflake for project {}",
                    project_name
                );

                // Log Snowflake version info if available (for debugging)
                if let Some(row) = rows.first() {
                    if let Some(version) = row.get("version").or_else(|| row.get("VERSION")) {
                        debug!("Snowflake version: {}", version);
                    }
                }

                status.state = SnowflakeOAuthState::CreatingIntegration;
                status.error = None;
            }
            Err(e) => {
                error!(
                    "Failed to connect to Snowflake for project {}: {:?}",
                    project_name, e
                );
                status.state = SnowflakeOAuthState::Failed;
                status.error = Some(format!(
                    "Connection test failed: {:?}. Please verify Snowflake credentials in backend configuration.",
                    e
                ));
            }
        }

        Ok(())
    }

    /// Handle CreatingIntegration state: create SECURITY INTEGRATION in Snowflake
    async fn handle_creating_integration(
        &self,
        spec: &SnowflakeOAuthProvisionerSpec,
        status: &mut SnowflakeOAuthProvisionerStatus,
        project_name: &str,
    ) -> Result<()> {
        let integration_name = status
            .integration_name
            .as_ref()
            .ok_or_else(|| anyhow!("Integration name not set"))?;
        let redirect_uri = status
            .redirect_uri
            .as_ref()
            .ok_or_else(|| anyhow!("Redirect URI not set"))?;

        info!(
            "Creating Snowflake SECURITY INTEGRATION {} for project {}",
            integration_name, project_name
        );

        // Get effective config (union of backend defaults + user overrides)
        let effective_config = self.get_effective_config(spec);

        // Format blocked roles list for SQL
        let blocked_roles_sql = effective_config
            .blocked_roles
            .iter()
            .map(|r| format!("'{}'", r))
            .collect::<Vec<_>>()
            .join(", ");

        // Create SECURITY INTEGRATION SQL
        let sql = format!(
            r#"CREATE SECURITY INTEGRATION {integration_name}
  TYPE = OAUTH
  ENABLED = TRUE
  OAUTH_CLIENT = CUSTOM
  OAUTH_CLIENT_TYPE = 'CONFIDENTIAL'
  OAUTH_REDIRECT_URI = '{redirect_uri}'
  OAUTH_ISSUE_REFRESH_TOKENS = TRUE
  OAUTH_REFRESH_TOKEN_VALIDITY = {refresh_token_validity}
  BLOCKED_ROLES_LIST = ({blocked_roles})"#,
            integration_name = integration_name,
            redirect_uri = redirect_uri,
            refresh_token_validity = self.refresh_token_validity_seconds,
            blocked_roles = blocked_roles_sql
        );

        match self.execute_sql(&sql).await {
            Ok(_) => {
                info!(
                    "Successfully created SECURITY INTEGRATION {}",
                    integration_name
                );
                status.state = SnowflakeOAuthState::RetrievingCredentials;
                status.error = None;
            }
            Err(e) => {
                error!(
                    "Failed to create SECURITY INTEGRATION {}: {:?}",
                    integration_name, e
                );
                status.state = SnowflakeOAuthState::Failed;
                status.error = Some(format!("Failed to create integration: {:?}", e));
            }
        }

        Ok(())
    }

    /// Handle RetrievingCredentials state: get OAuth client credentials from Snowflake
    async fn handle_retrieving_credentials(
        &self,
        status: &mut SnowflakeOAuthProvisionerStatus,
        project_name: &str,
    ) -> Result<()> {
        let integration_name = status
            .integration_name
            .as_ref()
            .ok_or_else(|| anyhow!("Integration name not set"))?;

        info!(
            "Retrieving OAuth credentials for integration {} (project: {})",
            integration_name, project_name
        );

        // Query for OAuth credentials
        let sql = format!(
            "SELECT SYSTEM$SHOW_OAUTH_CLIENT_SECRETS('{}') as credentials",
            integration_name
        );

        match self.execute_sql(&sql).await {
            Ok(rows) => {
                if let Some(row) = rows.first() {
                    // Parse the JSON response
                    let credentials_json = row
                        .get("credentials")
                        .or_else(|| row.get("CREDENTIALS"))
                        .ok_or_else(|| anyhow!("Credentials field not found in response"))?;

                    let credentials_str = credentials_json
                        .as_str()
                        .ok_or_else(|| anyhow!("Credentials is not a string"))?;

                    let credentials: Value = serde_json::from_str(credentials_str)
                        .context("Failed to parse credentials JSON")?;

                    let client_id = credentials["OAUTH_CLIENT_ID"]
                        .as_str()
                        .ok_or_else(|| anyhow!("OAUTH_CLIENT_ID not found"))?
                        .to_string();

                    let client_secret = credentials["OAUTH_CLIENT_SECRET"]
                        .as_str()
                        .ok_or_else(|| anyhow!("OAUTH_CLIENT_SECRET not found"))?
                        .to_string();

                    // Encrypt client secret
                    let client_secret_encrypted = self
                        .encryption_provider
                        .encrypt(&client_secret)
                        .await
                        .context("Failed to encrypt client secret")?;

                    // Update status
                    status.oauth_client_id = Some(client_id);
                    status.oauth_client_secret_encrypted = Some(client_secret_encrypted);
                    status.state = SnowflakeOAuthState::CreatingOAuthExtension;
                    status.error = None;

                    info!(
                        "Successfully retrieved credentials for integration {}",
                        integration_name
                    );
                } else {
                    let error_msg = "No credentials returned from Snowflake";
                    error!("{}", error_msg);
                    status.state = SnowflakeOAuthState::Failed;
                    status.error = Some(error_msg.to_string());
                }
            }
            Err(e) => {
                error!(
                    "Failed to retrieve credentials for integration {}: {:?}",
                    integration_name, e
                );
                status.state = SnowflakeOAuthState::Failed;
                status.error = Some(format!("Failed to retrieve credentials: {:?}", e));
            }
        }

        Ok(())
    }

    /// Handle CreatingOAuthExtension state: create Generic OAuth extension
    async fn handle_creating_oauth_extension(
        &self,
        spec: &SnowflakeOAuthProvisionerSpec,
        status: &mut SnowflakeOAuthProvisionerStatus,
        project_id: Uuid,
        project_name: &str,
        extension_name: &str,
    ) -> Result<()> {
        let oauth_extension_name = status
            .oauth_extension_name
            .as_ref()
            .ok_or_else(|| anyhow!("OAuth extension name not set"))?;

        info!(
            "Creating OAuth extension {} for project {}",
            oauth_extension_name, project_name
        );

        // Check if OAuth extension already exists
        if let Ok(Some(_)) = db_extensions::find_by_project_and_name(
            &self.db_pool,
            project_id,
            oauth_extension_name,
        )
        .await
        {
            info!(
                "OAuth extension {} already exists, skipping creation",
                oauth_extension_name
            );
            status.state = SnowflakeOAuthState::Available;
            return Ok(());
        }

        // Decrypt client secret from status
        let client_secret = self
            .encryption_provider
            .decrypt(
                status
                    .oauth_client_secret_encrypted
                    .as_ref()
                    .ok_or_else(|| anyhow!("Client secret not set"))?,
            )
            .await
            .context("Failed to decrypt client secret")?;

        // Create encrypted environment variable for OAuth extension to use
        let env_var_name = format!(
            "SNOWFLAKE_OAUTH_{}_SECRET",
            extension_name.to_uppercase().replace('-', "_")
        );

        let client_secret_encrypted = self
            .encryption_provider
            .encrypt(&client_secret)
            .await
            .context("Failed to encrypt client secret for env var")?;

        db_env_vars::upsert_project_env_var(
            &self.db_pool,
            project_id,
            &env_var_name,
            &client_secret_encrypted,
            true, // is_secret
        )
        .await
        .context("Failed to create environment variable")?;

        info!(
            "Created environment variable {} for project {}",
            env_var_name, project_name
        );

        // Get effective config for scopes
        let effective_config = self.get_effective_config(spec);

        // Create OAuth extension spec
        let oauth_spec = json!({
            "provider_name": format!("Snowflake ({})", project_name),
            "description": format!("Auto-provisioned Snowflake OAuth for {}", project_name),
            "client_id": status.oauth_client_id.as_ref().ok_or_else(|| anyhow!("Client ID not set"))?,
            "client_secret_ref": env_var_name,
            "authorization_endpoint": format!("https://{}.snowflakecomputing.com/oauth/authorize", self.account),
            "token_endpoint": format!("https://{}.snowflakecomputing.com/oauth/token-request", self.account),
            "scopes": effective_config.scopes,
        });

        // Create OAuth extension
        db_extensions::create(
            &self.db_pool,
            project_id,
            oauth_extension_name,
            "oauth",
            &oauth_spec,
        )
        .await
        .context("Failed to create OAuth extension")?;

        info!(
            "Created OAuth extension {} for project {}",
            oauth_extension_name, project_name
        );

        // Initialize OAuth provider if available
        if let Some(oauth_provider) = &self.oauth_provider {
            oauth_provider
                .on_spec_updated(
                    &json!({}),
                    &oauth_spec,
                    project_id,
                    oauth_extension_name,
                    &self.db_pool,
                )
                .await
                .context("Failed to initialize OAuth provider")?;

            info!(
                "Initialized OAuth provider for extension {}",
                oauth_extension_name
            );
        }

        status.state = SnowflakeOAuthState::Available;
        status.error = None;

        Ok(())
    }

    /// Verify integration is still available (health check)
    async fn verify_integration_available(
        &self,
        status: &mut SnowflakeOAuthProvisionerStatus,
        project_name: &str,
    ) -> Result<()> {
        let integration_name = status
            .integration_name
            .as_ref()
            .ok_or_else(|| anyhow!("Integration name not set"))?;

        // Check if integration still exists
        let sql = format!("SHOW INTEGRATIONS LIKE '{}'", integration_name);

        match self.execute_sql(&sql).await {
            Ok(rows) => {
                if rows.is_empty() {
                    warn!(
                        "Integration {} not found for project {}, marking as failed",
                        integration_name, project_name
                    );
                    status.state = SnowflakeOAuthState::Failed;
                    status.error = Some("Integration no longer exists in Snowflake".to_string());
                }
            }
            Err(e) => {
                warn!(
                    "Failed to verify integration {} for project {}: {:?}",
                    integration_name, project_name, e
                );
                // Don't mark as failed on verification errors, just log
            }
        }

        Ok(())
    }

    /// Handle deletion: cleanup all resources
    async fn handle_deletion(
        &self,
        status: &mut SnowflakeOAuthProvisionerStatus,
        project_id: Uuid,
        project_name: &str,
        extension_name: &str,
    ) -> Result<()> {
        info!(
            "Deleting Snowflake OAuth resources for project {} (extension: {})",
            project_name, extension_name
        );

        // 1. Drop Snowflake integration (best effort)
        if let Some(integration_name) = &status.integration_name {
            let sql = format!("DROP INTEGRATION IF EXISTS {}", integration_name);
            match self.execute_sql(&sql).await {
                Ok(_) => {
                    info!("Dropped Snowflake integration {}", integration_name);
                }
                Err(e) => {
                    warn!("Failed to drop integration {}: {:?}", integration_name, e);
                }
            }
        }

        // 2. Delete OAuth extension (marks for deletion, OAuth provider handles cleanup)
        if let Some(oauth_ext_name) = &status.oauth_extension_name {
            if let Err(e) =
                db_extensions::mark_deleted(&self.db_pool, project_id, oauth_ext_name).await
            {
                warn!(
                    "Failed to mark OAuth extension {} for deletion: {:?}",
                    oauth_ext_name, e
                );
            } else {
                info!("Marked OAuth extension {} for deletion", oauth_ext_name);
            }
        }

        // 3. Delete environment variable
        let env_var_name = format!(
            "SNOWFLAKE_OAUTH_{}_SECRET",
            extension_name.to_uppercase().replace('-', "_")
        );
        if let Err(e) =
            db_env_vars::delete_project_env_var(&self.db_pool, project_id, &env_var_name).await
        {
            warn!(
                "Failed to delete environment variable {}: {:?}",
                env_var_name, e
            );
        } else {
            info!("Deleted environment variable {}", env_var_name);
        }

        // 4. Remove finalizer
        let finalizer = self.finalizer_name(extension_name);
        if let Err(e) = db_projects::remove_finalizer(&self.db_pool, project_id, &finalizer).await {
            error!(
                "Failed to remove finalizer '{}' from project {}: {}",
                finalizer, project_name, e
            );
        } else {
            info!(
                "Removed finalizer '{}' from project {}",
                finalizer, project_name
            );
        }

        status.state = SnowflakeOAuthState::Deleted;
        Ok(())
    }

    /// Reconcile a single Snowflake OAuth extension
    async fn reconcile_single(
        &self,
        project_extension: crate::db::models::ProjectExtension,
    ) -> Result<bool> {
        debug!(
            "Reconciling Snowflake OAuth extension: {:?}",
            project_extension
        );

        let project = db_projects::find_by_id(&self.db_pool, project_extension.project_id)
            .await?
            .ok_or_else(|| anyhow!("Project not found"))?;

        // Parse spec
        let spec: SnowflakeOAuthProvisionerSpec =
            serde_json::from_value(project_extension.spec.clone())
                .context("Failed to parse Snowflake OAuth provisioner spec")?;

        // Parse current status or create default
        let mut status: SnowflakeOAuthProvisionerStatus =
            serde_json::from_value(project_extension.status.clone()).unwrap_or_default();

        // Check if marked for deletion
        if project_extension.deleted_at.is_some() {
            if status.state != SnowflakeOAuthState::Deleted {
                self.handle_deletion(
                    &mut status,
                    project_extension.project_id,
                    &project.name,
                    &project_extension.extension,
                )
                .await?;

                // Update status
                db_extensions::update_status(
                    &self.db_pool,
                    project_extension.project_id,
                    &project_extension.extension,
                    &serde_json::to_value(&status)?,
                )
                .await?;

                // Hard delete if deletion is complete
                if status.state == SnowflakeOAuthState::Deleted {
                    db_extensions::delete_permanently(
                        &self.db_pool,
                        project_extension.project_id,
                        &project_extension.extension,
                    )
                    .await?;
                    info!(
                        "Permanently deleted extension record for project {}",
                        project.name
                    );
                }
            }
            return Ok(false); // Deletion complete
        }

        // Track initial state for change detection
        let initial_state = status.state.clone();

        // Handle normal lifecycle
        match status.state {
            SnowflakeOAuthState::Pending => {
                self.handle_pending(
                    &spec,
                    &mut status,
                    &project.name,
                    project.id,
                    &project_extension.extension,
                )
                .await?;
            }
            SnowflakeOAuthState::TestingConnection => {
                self.handle_testing_connection(&mut status, &project.name)
                    .await?;
            }
            SnowflakeOAuthState::CreatingIntegration => {
                self.handle_creating_integration(&spec, &mut status, &project.name)
                    .await?;
            }
            SnowflakeOAuthState::RetrievingCredentials => {
                self.handle_retrieving_credentials(&mut status, &project.name)
                    .await?;
            }
            SnowflakeOAuthState::CreatingOAuthExtension => {
                self.handle_creating_oauth_extension(
                    &spec,
                    &mut status,
                    project.id,
                    &project.name,
                    &project_extension.extension,
                )
                .await?;
            }
            SnowflakeOAuthState::Available => {
                self.verify_integration_available(&mut status, &project.name)
                    .await?;
            }
            SnowflakeOAuthState::Failed => {
                // Retry creation immediately
                info!(
                    "Snowflake OAuth provisioner for project {} is in failed state, retrying",
                    project.name
                );
                status.state = SnowflakeOAuthState::Pending;
                status.error = None;
            }
            _ => {}
        }

        // Update status in database
        db_extensions::update_status(
            &self.db_pool,
            project_extension.project_id,
            &project_extension.extension,
            &serde_json::to_value(&status)?,
        )
        .await?;

        // Determine if more work can be done immediately
        let state_changed = status.state != initial_state;

        Ok(state_changed)
    }
}

#[async_trait]
impl Extension for SnowflakeOAuthProvisioner {
    fn extension_type(&self) -> &str {
        "snowflake-oauth-provisioner"
    }

    fn display_name(&self) -> &str {
        "Snowflake OAuth"
    }

    fn description(&self) -> &str {
        "Provisions Snowflake SECURITY INTEGRATIONs and configures Generic OAuth extensions for Snowflake authentication"
    }

    fn documentation(&self) -> &str {
        r#"# Snowflake OAuth Provisioner

Automatically provisions Snowflake SECURITY INTEGRATIONs and creates Generic OAuth extensions for end-user authentication.

## Configuration

Backend credentials are configured in `config/default.yaml`:

```yaml
extensions:
  providers:
  - type: snowflake-oauth-provisioner
    account: "myorg.us-east-1"
    user: "admin_user"
    auth_type: password
    password: "${SNOWFLAKE_PASSWORD}"
    integration_name_prefix: "rise"
    default_blocked_roles: ["ACCOUNTADMIN", "SECURITYADMIN"]
    default_scopes: ["refresh_token"]
    refresh_token_validity_seconds: 7776000  # 90 days
```

## User Spec

Users can optionally configure additional blocked roles and scopes (unioned with backend defaults):

```yaml
blocked_roles:
  - SYSADMIN
scopes:
  - session:role:ANALYST
```

## Lifecycle

1. Pending → TestingConnection (verify Snowflake credentials)
2. TestingConnection → CreatingIntegration (CREATE SECURITY INTEGRATION)
3. CreatingIntegration → RetrievingCredentials (fetch OAuth credentials)
4. RetrievingCredentials → CreatingOAuthExtension (create OAuth extension)
5. CreatingOAuthExtension → Available (ready for use)

The provisioner tests the Snowflake connection before creating the integration to catch
credential issues early. If the test fails, the extension will transition to Failed state
with an error message.

Deletion removes all resources: Snowflake integration, OAuth extension, and environment variables.
"#
    }

    fn spec_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "blocked_roles": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Additional blocked roles (unioned with backend defaults)",
                    "default": []
                },
                "scopes": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Additional OAuth scopes (unioned with backend defaults)",
                    "default": []
                }
            },
            "additionalProperties": false
        })
    }

    async fn validate_spec(&self, spec: &Value) -> Result<()> {
        let _parsed: SnowflakeOAuthProvisionerSpec = serde_json::from_value(spec.clone())
            .context("Invalid Snowflake OAuth provisioner spec")?;
        Ok(())
    }

    fn format_status(&self, status: &Value) -> String {
        match serde_json::from_value::<SnowflakeOAuthProvisionerStatus>(status.clone()) {
            Ok(status) => {
                let state = format!("{:?}", status.state);
                if let Some(error) = &status.error {
                    format!("{} (error: {})", state, error)
                } else if let Some(integration_name) = &status.integration_name {
                    format!("{} (integration: {})", state, integration_name)
                } else {
                    state
                }
            }
            Err(_) => "Unknown".to_string(),
        }
    }

    async fn before_deployment(
        &self,
        _deployment_id: Uuid,
        _project_id: Uuid,
        _deployment_group: &str,
    ) -> Result<()> {
        // No-op: this extension doesn't inject deployment-specific resources
        Ok(())
    }

    fn start(&self) {
        let provisioner = self.clone();

        tokio::spawn(async move {
            info!("Starting Snowflake OAuth provisioner reconciliation loop");

            let mut error_state: HashMap<Uuid, (usize, DateTime<Utc>)> = HashMap::new();

            loop {
                match db_extensions::list_by_extension_type(
                    &provisioner.db_pool,
                    "snowflake-oauth-provisioner",
                )
                .await
                {
                    Ok(extensions) => {
                        for ext in extensions {
                            // Apply exponential backoff for errors
                            if let Some((error_count, last_error)) =
                                error_state.get(&ext.project_id)
                            {
                                let backoff_seconds = 2_i64.pow(*error_count as u32).min(300);
                                let backoff_until =
                                    *last_error + Duration::seconds(backoff_seconds);

                                if Utc::now() < backoff_until {
                                    continue;
                                }
                            }

                            match provisioner.reconcile_single(ext.clone()).await {
                                Ok(_) => {
                                    error_state.remove(&ext.project_id);
                                }
                                Err(e) => {
                                    error!("Reconciliation failed: {:?}", e);
                                    let entry = error_state
                                        .entry(ext.project_id)
                                        .or_insert((0, Utc::now()));
                                    entry.0 += 1;
                                    entry.1 = Utc::now();
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to list Snowflake OAuth extensions: {:?}", e);
                    }
                }

                sleep(std::time::Duration::from_secs(5)).await;
            }
        });
    }
}
