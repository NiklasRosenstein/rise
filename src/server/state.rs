use crate::server::auth::{
    cookie_helpers::CookieSettings,
    jwt::JwtValidator,
    jwt_signer::JwtSigner,
    oauth::OAuthClient,
    token_storage::{InMemoryTokenStore, TokenStore},
};
use crate::server::encryption::EncryptionProvider;
use crate::server::registry::{
    models::{EcrConfig, OciClientAuthConfig},
    providers::{EcrProvider, OciClientAuthProvider},
    RegistryProvider,
};
use crate::server::settings::{
    AuthSettings, EncryptionSettings, RegistrySettings, ServerSettings, Settings,
};
use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

/// Minimal state for controllers - database access and encryption
#[derive(Clone)]
pub struct ControllerState {
    pub db_pool: PgPool,
    pub encryption_provider: Option<Arc<dyn EncryptionProvider>>,
}

/// Full state for HTTP server
#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub jwt_validator: Arc<JwtValidator>,
    pub jwt_signer: Arc<JwtSigner>,
    pub oauth_client: Arc<OAuthClient>,
    pub registry_provider: Option<Arc<dyn RegistryProvider>>,
    pub oci_client: Arc<crate::server::oci::OciClient>,
    pub admin_users: Arc<Vec<String>>,
    pub auth_settings: Arc<AuthSettings>,
    pub server_settings: Arc<ServerSettings>,
    pub token_store: Arc<dyn TokenStore>,
    pub cookie_settings: CookieSettings,
    pub public_url: String,
    pub encryption_provider: Option<Arc<dyn EncryptionProvider>>,
}

/// Initialize encryption provider from settings
async fn init_encryption_provider(
    encryption_settings: Option<&EncryptionSettings>,
) -> Result<Option<Arc<dyn EncryptionProvider>>> {
    if let Some(encryption_config) = encryption_settings {
        match encryption_config {
            EncryptionSettings::Local { key } => {
                use crate::server::encryption::providers::local::LocalEncryptionProvider;
                let provider = LocalEncryptionProvider::new(key)
                    .context("Failed to initialize local encryption provider")?;

                // Test encryption/decryption at startup
                tracing::info!("Testing local encryption provider...");
                test_encryption_provider(&provider).await?;
                tracing::info!("✓ Local AES-256-GCM encryption provider initialized and validated");

                Ok(Some(Arc::new(provider)))
            }
            EncryptionSettings::AwsKms {
                region,
                key_id,
                access_key_id,
                secret_access_key,
            } => {
                use crate::server::encryption::providers::aws_kms::AwsKmsEncryptionProvider;
                let provider = AwsKmsEncryptionProvider::new(
                    region,
                    key_id.clone(),
                    access_key_id.clone(),
                    secret_access_key.clone(),
                )
                .await
                .context("Failed to initialize AWS KMS encryption provider")?;

                // Test encryption/decryption at startup
                tracing::info!("Testing AWS KMS encryption provider with key {}...", key_id);
                test_encryption_provider(&provider).await.with_context(|| {
                    format!(
                        "KMS provider initialized but encryption test failed. \
                         Please verify: 1) Key ARN/ID '{}' is valid, \
                         2) AWS credentials are available, \
                         3) IAM permissions include kms:Encrypt and kms:Decrypt, \
                         4) Key is enabled and not pending deletion",
                        key_id
                    )
                })?;
                tracing::info!("✓ AWS KMS encryption provider initialized and validated");

                Ok(Some(Arc::new(provider)))
            }
        }
    } else {
        tracing::info!("No encryption provider configured - secret environment variables will not be available");
        Ok(None)
    }
}

/// Test an encryption provider with a sample encrypt/decrypt round-trip
async fn test_encryption_provider(provider: &dyn EncryptionProvider) -> Result<()> {
    const TEST_PLAINTEXT: &str = "rise-encryption-test-12345";

    let ciphertext = provider
        .encrypt(TEST_PLAINTEXT)
        .await
        .context("Encryption test failed")?;

    let decrypted = provider
        .decrypt(&ciphertext)
        .await
        .context("Decryption test failed")?;

    if decrypted != TEST_PLAINTEXT {
        anyhow::bail!("Encryption round-trip test failed: decrypted value does not match original");
    }

    Ok(())
}

impl ControllerState {
    /// Create minimal controller state with database access and encryption
    pub async fn new(
        database_url: &str,
        max_connections: u32,
        encryption_settings: Option<&EncryptionSettings>,
    ) -> Result<Self> {
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

        let encryption_provider = init_encryption_provider(encryption_settings).await?;

        Ok(Self {
            db_pool,
            encryption_provider,
        })
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

        // Initialize JWT signer for ingress authentication (required)
        let jwt_signer = Arc::new(
            JwtSigner::new(
                &settings.server.jwt_signing_secret,
                settings.server.public_url.clone(),
                3600, // Default 1 hour expiry (matches typical IdP token expiry)
                settings.server.jwt_claims.clone(),
            )
            .context("Failed to initialize JWT signer for ingress authentication")?,
        );
        tracing::info!("Initialized JWT signer for ingress authentication");

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
            Arc::new(crate::server::oci::OciClient::new().context("Failed to initialize OCI client")?);
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

        // Initialize encryption provider
        let encryption_provider = init_encryption_provider(settings.encryption.as_ref()).await?;

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
            encryption_provider,
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
            Arc::new(crate::server::oci::OciClient::new().context("Failed to initialize OCI client")?);

        // Dummy auth components (not used by controller)
        let jwt_validator = Arc::new(JwtValidator::new());

        // Initialize dummy JWT signer (not used by controller, but required by AppState)
        let jwt_signer = Arc::new(
            JwtSigner::new(
                &settings.server.jwt_signing_secret,
                settings.server.public_url.clone(),
                3600,
                settings.server.jwt_claims.clone(),
            )
            .context("Failed to initialize JWT signer")?,
        );

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

        // Initialize encryption provider
        let encryption_provider = init_encryption_provider(settings.encryption.as_ref()).await?;

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
            encryption_provider,
        })
    }
}
