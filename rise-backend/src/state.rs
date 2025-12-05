use crate::settings::{Settings, RegistrySettings};
use crate::registry::{RegistryProvider, providers::{EcrProvider, DockerProvider}, models::{EcrConfig, DockerConfig}};
use crate::auth::{jwt::JwtValidator, oauth::DexOAuthClient};
use std::sync::Arc;
use anyhow::{Result, Context};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub http_client: Arc<reqwest::Client>,
    pub db_pool: PgPool,
    pub jwt_validator: Arc<JwtValidator>,
    pub oauth_client: Arc<DexOAuthClient>,
    pub registry_provider: Option<Arc<dyn RegistryProvider>>,
    pub oci_client: Arc<crate::oci::OciClient>,
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

    /// Wait for PostgreSQL to become available
    async fn wait_for_postgres(database_url: &str) -> Result<PgPool> {
        tracing::info!("Connecting to PostgreSQL...");

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .context("Failed to connect to PostgreSQL")?;

        tracing::info!("Successfully connected to PostgreSQL");
        Ok(pool)
    }

    pub async fn new(settings: &Settings) -> Result<Self> {
        let http_client = reqwest::Client::new();

        // Connect to PostgreSQL
        let db_pool = Self::wait_for_postgres(&settings.database.url).await?;

        // Run migrations
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
        let registry_provider: Option<Arc<dyn RegistryProvider>> = if let Some(ref registry_config) = settings.registry {
            match registry_config {
                RegistrySettings::Ecr { region, account_id, access_key_id, secret_access_key } => {
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
                RegistrySettings::Docker { registry_url, namespace } => {
                    let docker_config = DockerConfig {
                        registry_url: registry_url.clone(),
                        namespace: namespace.clone(),
                    };
                    match DockerProvider::new(docker_config) {
                        Ok(provider) => {
                            tracing::info!("Initialized Docker registry provider at {}", registry_url);
                            Some(Arc::new(provider))
                        }
                        Err(e) => {
                            tracing::error!("Failed to initialize Docker provider: {}", e);
                            None
                        }
                    }
                }
            }
        } else {
            tracing::warn!("No registry configured - registry credentials endpoint will not be available");
            None
        };

        // Initialize OCI client for direct registry interaction
        let oci_client = Arc::new(
            crate::oci::OciClient::new()
                .context("Failed to initialize OCI client")?
        );
        tracing::info!("Initialized OCI client for registry digest resolution");

        Ok(Self {
            settings: Arc::new(settings.clone()),
            http_client: Arc::new(http_client),
            db_pool,
            jwt_validator,
            oauth_client,
            registry_provider,
            oci_client,
        })
    }
}
