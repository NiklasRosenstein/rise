use crate::auth::{
    cookie_helpers::CookieSettings,
    jwt::JwtValidator,
    jwt_signer::JwtSigner,
    oauth::OAuthClient,
    token_storage::{InMemoryTokenStore, TokenStore},
};
use crate::registry::{
    models::{EcrConfig, OciClientAuthConfig},
    providers::{EcrProvider, OciClientAuthProvider},
    RegistryProvider,
};
use crate::settings::{AuthSettings, RegistrySettings, ServerSettings, Settings};
use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

/// Minimal state for controllers - just database access
#[derive(Clone)]
pub struct ControllerState {
    pub db_pool: PgPool,
}

/// Full state for HTTP server
#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub jwt_validator: Arc<JwtValidator>,
    pub jwt_signer: Option<Arc<JwtSigner>>,
    pub oauth_client: Arc<OAuthClient>,
    pub registry_provider: Option<Arc<dyn RegistryProvider>>,
    pub oci_client: Arc<crate::oci::OciClient>,
    pub admin_users: Arc<Vec<String>>,
    pub auth_settings: Arc<AuthSettings>,
    pub server_settings: Arc<ServerSettings>,
    pub token_store: Arc<dyn TokenStore>,
    pub cookie_settings: CookieSettings,
    pub public_url: String,
}

impl ControllerState {
    /// Create minimal controller state with database access only
    pub async fn new(database_url: &str, max_connections: u32) -> Result<Self> {
        tracing::info!(
            "Connecting to PostgreSQL with {} max connections...",
            max_connections
        );

        let db_pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await
            .context("Failed to connect to PostgreSQL")?;

        tracing::info!("Successfully connected to PostgreSQL");
        Ok(Self { db_pool })
    }
}

impl AppState {
    /// Run database migrations
    async fn run_migrations(pool: &PgPool) -> Result<()> {
        tracing::info!("Running database migrations...");
        sqlx::migrate!("./migrations")
            .run(pool)
            .await
            .context("Failed to run migrations")?;
        tracing::info!("Migrations completed successfully");
        Ok(())
    }

    /// Initialize full state for HTTP server
    pub async fn new_for_server(settings: &Settings) -> Result<Self> {
        tracing::info!("Initializing AppState for HTTP server");

        // Connect to PostgreSQL with server-optimized pool size
        let db_pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(&settings.database.url)
            .await
            .context("Failed to connect to PostgreSQL")?;

        tracing::info!("Successfully connected to PostgreSQL");

        // Run migrations (server-only)
        Self::run_migrations(&db_pool).await?;

        // Initialize JWT validator (JWKS is fetched on-demand)
        let jwt_validator = Arc::new(JwtValidator::new());

        // Initialize JWT signer for ingress authentication (if secret provided)
        let jwt_signer = if let Some(ref secret) = settings.server.jwt_signing_secret {
            match JwtSigner::new(
                secret,
                settings.server.public_url.clone(),
                3600, // Default 1 hour expiry (matches typical IdP token expiry)
                settings.server.jwt_claims.clone(),
            ) {
                Ok(signer) => {
                    tracing::info!("Initialized JWT signer for ingress authentication");
                    Some(Arc::new(signer))
                }
                Err(e) => {
                    tracing::error!("Failed to initialize JWT signer: {}", e);
                    tracing::warn!(
                        "Ingress authentication will fall back to IdP tokens (less secure)"
                    );
                    None
                }
            }
        } else {
            tracing::warn!(
                "No JWT signing secret configured - ingress authentication will use IdP tokens"
            );
            tracing::warn!("Generate a secret with: openssl rand -base64 32");
            None
        };

        // Initialize OAuth2 client
        let oauth_client = Arc::new(
            OAuthClient::new(
                settings.auth.issuer.clone(),
                settings.auth.client_id.clone(),
                settings.auth.client_secret.clone(),
                settings.auth.authorize_url.clone(),
                settings.auth.token_url.clone(),
            )
            .await?,
        );

        // Initialize registry provider based on configuration
        let registry_provider: Option<Arc<dyn RegistryProvider>> =
            if let Some(ref registry_config) = settings.registry {
                match registry_config {
                    RegistrySettings::Ecr {
                        region,
                        account_id,
                        repo_prefix,
                        role_arn,
                        push_role_arn,
                        auto_remove,
                        access_key_id,
                        secret_access_key,
                    } => {
                        let ecr_config = EcrConfig {
                            region: region.clone(),
                            account_id: account_id.clone(),
                            repo_prefix: repo_prefix.clone(),
                            role_arn: role_arn.clone(),
                            push_role_arn: push_role_arn.clone(),
                            auto_remove: *auto_remove,
                            access_key_id: access_key_id.clone(),
                            secret_access_key: secret_access_key.clone(),
                        };
                        match EcrProvider::new(ecr_config).await {
                            Ok(provider) => {
                                tracing::info!("Initialized ECR registry provider");
                                Some(Arc::new(provider))
                            }
                            Err(e) => {
                                tracing::error!("Failed to initialize ECR provider: {}", e);
                                None
                            }
                        }
                    }
                    RegistrySettings::OciClientAuth {
                        registry_url,
                        namespace,
                    } => {
                        let oci_config = OciClientAuthConfig {
                            registry_url: registry_url.clone(),
                            namespace: namespace.clone(),
                        };
                        match OciClientAuthProvider::new(oci_config) {
                            Ok(provider) => {
                                tracing::info!(
                                    "Initialized OCI client-auth registry provider at {}",
                                    registry_url
                                );
                                Some(Arc::new(provider))
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to initialize OCI client-auth provider: {}",
                                    e
                                );
                                None
                            }
                        }
                    }
                }
            } else {
                tracing::warn!(
                    "No registry configured - registry credentials endpoint will not be available"
                );
                None
            };

        // Initialize OCI client for direct registry interaction
        let oci_client =
            Arc::new(crate::oci::OciClient::new().context("Failed to initialize OCI client")?);
        tracing::info!("Initialized OCI client for registry digest resolution");

        // Store admin users list
        let admin_users = Arc::new(settings.auth.admin_users.clone());
        if !admin_users.is_empty() {
            tracing::info!("Configured {} admin user(s)", admin_users.len());
        }

        // Store auth settings for issuer comparison
        let auth_settings = Arc::new(settings.auth.clone());

        // Store server settings for frontend config injection
        let server_settings = Arc::new(settings.server.clone());

        // Initialize token store for OAuth2 PKCE flow (10 minute TTL)
        let token_store: Arc<dyn TokenStore> =
            Arc::new(InMemoryTokenStore::new(Duration::from_secs(600)));
        tracing::info!("Initialized in-memory token store for OAuth2 state");

        // Initialize cookie settings for session management
        let cookie_settings = CookieSettings {
            domain: settings.server.cookie_domain.clone(),
            secure: settings.server.cookie_secure,
        };
        tracing::info!(
            "Configured session cookies with domain={:?}, secure={}",
            if cookie_settings.domain.is_empty() {
                "current-host-only"
            } else {
                &cookie_settings.domain
            },
            cookie_settings.secure
        );

        let public_url = settings.server.public_url.clone();
        tracing::info!("Public URL: {}", public_url);

        Ok(Self {
            db_pool,
            jwt_validator,
            jwt_signer,
            oauth_client,
            registry_provider,
            oci_client,
            admin_users,
            auth_settings,
            server_settings,
            token_store,
            cookie_settings,
            public_url,
        })
    }

    /// Initialize minimal state for deployment controller
    ///
    /// The deployment controller only needs database and registry access.
    /// We use dummy values for auth components since they're not used.
    pub async fn new_for_controller(settings: &Settings) -> Result<Self> {
        tracing::info!("Initializing AppState for deployment controller");

        // Connect to PostgreSQL with controller-optimized pool size
        let db_pool = PgPoolOptions::new()
            .max_connections(3)
            .connect(&settings.database.url)
            .await
            .context("Failed to connect to PostgreSQL")?;

        tracing::info!("Successfully connected to PostgreSQL");

        // Initialize registry provider based on configuration
        let registry_provider: Option<Arc<dyn RegistryProvider>> =
            if let Some(ref registry_config) = settings.registry {
                match registry_config {
                    RegistrySettings::Ecr {
                        region,
                        account_id,
                        repo_prefix,
                        role_arn,
                        push_role_arn,
                        auto_remove,
                        access_key_id,
                        secret_access_key,
                    } => {
                        let ecr_config = EcrConfig {
                            region: region.clone(),
                            account_id: account_id.clone(),
                            repo_prefix: repo_prefix.clone(),
                            role_arn: role_arn.clone(),
                            push_role_arn: push_role_arn.clone(),
                            auto_remove: *auto_remove,
                            access_key_id: access_key_id.clone(),
                            secret_access_key: secret_access_key.clone(),
                        };
                        match EcrProvider::new(ecr_config).await {
                            Ok(provider) => {
                                tracing::info!("Initialized ECR registry provider");
                                Some(Arc::new(provider))
                            }
                            Err(e) => {
                                tracing::error!("Failed to initialize ECR provider: {}", e);
                                None
                            }
                        }
                    }
                    RegistrySettings::OciClientAuth {
                        registry_url,
                        namespace,
                    } => {
                        let oci_config = OciClientAuthConfig {
                            registry_url: registry_url.clone(),
                            namespace: namespace.clone(),
                        };
                        match OciClientAuthProvider::new(oci_config) {
                            Ok(provider) => {
                                tracing::info!(
                                    "Initialized OCI client-auth registry provider at {}",
                                    registry_url
                                );
                                Some(Arc::new(provider))
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to initialize OCI client-auth provider: {}",
                                    e
                                );
                                None
                            }
                        }
                    }
                }
            } else {
                tracing::warn!("No registry configured - image tag construction will use fallback");
                None
            };

        // Initialize OCI client (needed for pre-built image deployments)
        let oci_client =
            Arc::new(crate::oci::OciClient::new().context("Failed to initialize OCI client")?);

        // Dummy auth components (not used by controller)
        let jwt_validator = Arc::new(JwtValidator::new());
        let jwt_signer = None; // Not used by controller
        let oauth_client = Arc::new(
            OAuthClient::new(
                settings.auth.issuer.clone(),
                settings.auth.client_id.clone(),
                settings.auth.client_secret.clone(),
                settings.auth.authorize_url.clone(),
                settings.auth.token_url.clone(),
            )
            .await?,
        );
        let admin_users = Arc::new(Vec::new());
        let auth_settings = Arc::new(settings.auth.clone());
        let server_settings = Arc::new(settings.server.clone());

        // Dummy OAuth proxy components (not used by controller)
        let token_store: Arc<dyn TokenStore> =
            Arc::new(InMemoryTokenStore::new(Duration::from_secs(600)));
        let cookie_settings = CookieSettings {
            domain: String::new(),
            secure: true,
        };
        let public_url = "http://localhost:3000".to_string(); // Dummy value, not used by controller

        Ok(Self {
            db_pool,
            jwt_validator,
            jwt_signer,
            oauth_client,
            registry_provider,
            oci_client,
            admin_users,
            auth_settings,
            server_settings,
            token_store,
            cookie_settings,
            public_url,
        })
    }
}
