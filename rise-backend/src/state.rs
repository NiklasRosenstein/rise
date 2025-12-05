use crate::auth::{jwt::JwtValidator, oauth::DexOAuthClient};
use crate::registry::{
    models::{EcrConfig, OciClientAuthConfig},
    providers::{EcrProvider, OciClientAuthProvider},
    RegistryProvider,
};
use crate::settings::{RegistrySettings, Settings};
use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;

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
    pub oauth_client: Arc<DexOAuthClient>,
    pub registry_provider: Option<Arc<dyn RegistryProvider>>,
    pub oci_client: Arc<crate::oci::OciClient>,
    pub admin_users: Arc<Vec<String>>,
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

        // Initialize JWT validator
        let jwt_validator = Arc::new(JwtValidator::new(
            settings.auth.issuer.clone(),
            settings.auth.client_id.clone(),
        ));

        // Fetch JWKS on startup
        jwt_validator
            .init()
            .await
            .context("Failed to initialize JWT validator")?;

        // Initialize OAuth2 client
        let oauth_client = Arc::new(DexOAuthClient::new(
            settings.auth.issuer.clone(),
            settings.auth.client_id.clone(),
            settings.auth.client_secret.clone(),
        )?);

        // Initialize registry provider based on configuration
        let registry_provider: Option<Arc<dyn RegistryProvider>> =
            if let Some(ref registry_config) = settings.registry {
                match registry_config {
                    RegistrySettings::Ecr {
                        region,
                        account_id,
                        access_key_id,
                        secret_access_key,
                    } => {
                        let ecr_config = EcrConfig {
                            region: region.clone(),
                            account_id: account_id.clone(),
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

        Ok(Self {
            db_pool,
            jwt_validator,
            oauth_client,
            registry_provider,
            oci_client,
            admin_users,
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
                        access_key_id,
                        secret_access_key,
                    } => {
                        let ecr_config = EcrConfig {
                            region: region.clone(),
                            account_id: account_id.clone(),
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
        let jwt_validator = Arc::new(JwtValidator::new(
            settings.auth.issuer.clone(),
            settings.auth.client_id.clone(),
        ));
        let oauth_client = Arc::new(DexOAuthClient::new(
            settings.auth.issuer.clone(),
            settings.auth.client_id.clone(),
            settings.auth.client_secret.clone(),
        )?);
        let admin_users = Arc::new(Vec::new());

        Ok(Self {
            db_pool,
            jwt_validator,
            oauth_client,
            registry_provider,
            oci_client,
            admin_users,
        })
    }
}
