use crate::server::auth::{
    cookie_helpers::CookieSettings,
    jwt::JwtValidator,
    jwt_signer::JwtSigner,
    oauth::OAuthClient,
    token_storage::{InMemoryTokenStore, TokenStore},
};
use crate::server::encryption::EncryptionProvider;
use crate::server::registry::{
    models::OciClientAuthConfig, providers::OciClientAuthProvider, RegistryProvider,
};

#[cfg(feature = "aws")]
use crate::server::registry::{models::EcrConfig, providers::EcrProvider};
use crate::server::settings::{
    AuthSettings, EncryptionSettings, RegistrySettings, ServerSettings, Settings,
};
use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "k8s")]
use crate::server::deployment::controller::{
    DeploymentBackend, KubernetesController, KubernetesControllerConfig,
};

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
    pub jwks_json: String,
    pub oauth_client: Arc<OAuthClient>,
    pub registry_provider: Arc<dyn RegistryProvider>,
    pub oci_client: Arc<crate::server::oci::OciClient>,
    pub admin_users: Arc<Vec<String>>,
    pub auth_settings: Arc<AuthSettings>,
    pub server_settings: Arc<ServerSettings>,
    pub token_store: Arc<dyn TokenStore>,
    pub cookie_settings: CookieSettings,
    pub public_url: String,
    /// Internal URL for cluster-internal access (defaults to public_url if not configured)
    pub internal_url: String,
    pub encryption_provider: Option<Arc<dyn EncryptionProvider>>,
    pub deployment_backend: Arc<dyn crate::server::deployment::controller::DeploymentBackend>,
    pub extension_registry: Arc<crate::server::extensions::registry::ExtensionRegistry>,
    pub oauth_state_store:
        Arc<moka::future::Cache<String, crate::server::extensions::providers::oauth::OAuthState>>,
    pub oauth_code_store: Arc<
        moka::future::Cache<String, crate::server::extensions::providers::oauth::OAuthCodeState>,
    >,
    pub access_classes:
        Arc<std::collections::HashMap<String, crate::server::settings::AccessClass>>,
    /// Production ingress URL template (for custom domain validation)
    pub production_ingress_url_template: Option<String>,
    /// Staging ingress URL template (for custom domain validation)
    pub staging_ingress_url_template: Option<String>,
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
            #[cfg(feature = "aws")]
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
            #[cfg(not(feature = "aws"))]
            EncryptionSettings::AwsKms { key_id, .. } => {
                anyhow::bail!(
                    "AWS KMS encryption is configured (key: {}) but the 'aws' feature is not enabled. \
                     Please rebuild with --features aws or use a pre-built binary with AWS support.",
                    key_id
                )
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

/// Initialize Kubernetes deployment backend from settings
#[cfg(feature = "k8s")]
async fn init_kubernetes_backend(
    settings: &Settings,
    controller_state: Arc<ControllerState>,
    registry_provider: Arc<dyn RegistryProvider>,
    jwks_json: String,
) -> Result<Arc<dyn DeploymentBackend>> {
    use crate::server::settings::DeploymentControllerSettings;

    if let Some(DeploymentControllerSettings::Kubernetes {
        kubeconfig,
        production_ingress_url_template,
        staging_ingress_url_template,
        ingress_port,
        ingress_schema,
        auth_backend_url,
        auth_signin_url,
        namespace_labels,
        namespace_annotations,
        ingress_annotations,
        ingress_tls_secret_name,
        custom_domain_tls_mode,
        custom_domain_ingress_annotations,
        node_selector,
        image_pull_secret_name,
        access_classes,
        host_aliases,
        ..
    }) = &settings.deployment_controller
    {
        // Install default CryptoProvider for rustls (required for kube-rs HTTPS connections)
        rustls::crypto::ring::default_provider()
            .install_default()
            .ok();

        // Create kube client
        let kube_config = if kubeconfig.is_some() {
            // Use explicit kubeconfig if provided
            kube::Config::from_kubeconfig(&kube::config::KubeConfigOptions {
                context: None,
                cluster: None,
                user: None,
            })
            .await?
        } else {
            kube::Config::infer().await? // In-cluster or ~/.kube/config
        };
        let kube_client = kube::Client::try_from(kube_config)?;

        // Extract backend_address from auth_backend_url
        let parsed_backend_address =
            crate::server::settings::BackendAddress::from_url(auth_backend_url)?;

        // Filter out null access classes (used to remove inherited entries)
        let filtered_access_classes: std::collections::HashMap<_, _> = access_classes
            .iter()
            .filter_map(|(k, v)| v.as_ref().map(|ac| (k.clone(), ac.clone())))
            .collect();

        let k8s_backend = KubernetesController::new(
            (*controller_state).clone(),
            kube_client,
            KubernetesControllerConfig {
                production_ingress_url_template: production_ingress_url_template.clone(),
                staging_ingress_url_template: staging_ingress_url_template.clone(),
                ingress_port: *ingress_port,
                ingress_schema: ingress_schema.clone(),
                registry_provider,
                auth_backend_url: auth_backend_url.clone(),
                auth_signin_url: auth_signin_url.clone(),
                backend_address: Some(parsed_backend_address),
                namespace_labels: namespace_labels.clone(),
                namespace_annotations: namespace_annotations.clone(),
                ingress_annotations: ingress_annotations.clone(),
                ingress_tls_secret_name: ingress_tls_secret_name.clone(),
                custom_domain_tls_mode: custom_domain_tls_mode.clone(),
                custom_domain_ingress_annotations: custom_domain_ingress_annotations.clone(),
                node_selector: node_selector.clone(),
                image_pull_secret_name: image_pull_secret_name.clone(),
                access_classes: filtered_access_classes,
                rise_jwks_json: jwks_json,
                rise_issuer: settings.server.public_url.clone(),
                host_aliases: host_aliases.clone(),
            },
        )?;

        // Test Kubernetes API connection
        k8s_backend.test_connection().await?;
        tracing::info!("✓ Kubernetes deployment backend initialized and connection tested");

        Ok(Arc::new(k8s_backend) as Arc<dyn DeploymentBackend>)
    } else {
        anyhow::bail!("Deployment controller not configured. Please add deployment_controller configuration with type: kubernetes")
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
    pub async fn new(settings: &Settings) -> Result<Self> {
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
                settings.server.jwt_expiry_seconds,
                settings.server.jwt_claims.clone(),
                settings.server.rs256_private_key_pem.as_deref(),
                settings.server.rs256_public_key_pem.as_deref(),
            )
            .context("Failed to initialize JWT signer for ingress authentication")?,
        );
        tracing::info!(
            "Initialized JWT signer for ingress authentication (expiry: {}s)",
            settings.server.jwt_expiry_seconds
        );

        // Generate JWKS JSON for RS256 public keys to pass to deployed applications
        let jwks = jwt_signer
            .generate_jwks()
            .context("Failed to generate JWKS for deployment controller")?;
        let jwks_json = serde_json::to_string(&jwks).context("Failed to serialize JWKS to JSON")?;
        tracing::info!("Generated JWKS for RS256 JWT verification in deployed apps");

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

        // Initialize registry provider (required for server operation)
        let registry_provider: Arc<dyn RegistryProvider> = match &settings.registry {
            Some(registry_config) => match registry_config {
                #[cfg(feature = "aws")]
                RegistrySettings::Ecr {
                    region,
                    account_id,
                    repo_prefix,
                    push_role_arn,
                    auto_remove,
                    access_key_id,
                    secret_access_key,
                } => {
                    let ecr_config = EcrConfig {
                        region: region.clone(),
                        account_id: account_id.clone(),
                        repo_prefix: repo_prefix.clone(),
                        push_role_arn: push_role_arn.clone(),
                        auto_remove: *auto_remove,
                        access_key_id: access_key_id.clone(),
                        secret_access_key: secret_access_key.clone(),
                    };
                    let provider = EcrProvider::new(ecr_config)
                        .await
                        .context("Failed to initialize ECR registry provider")?;
                    tracing::info!("Initialized ECR registry provider");
                    Arc::new(provider)
                }
                #[cfg(not(feature = "aws"))]
                RegistrySettings::Ecr { account_id, .. } => {
                    anyhow::bail!(
                        "AWS ECR registry is configured (account: {}) but the 'aws' feature is not enabled. \
                         Please rebuild with --features aws or use a pre-built binary with AWS support.",
                        account_id
                    )
                }
                RegistrySettings::OciClientAuth {
                    registry_url,
                    namespace,
                    client_registry_url,
                } => {
                    let oci_config = OciClientAuthConfig {
                        registry_url: registry_url.clone(),
                        namespace: namespace.clone(),
                        client_registry_url: client_registry_url.clone(),
                    };
                    let provider = OciClientAuthProvider::new(oci_config)
                        .context("Failed to initialize OCI client-auth registry provider")?;
                    tracing::info!(
                        "Initialized OCI client-auth registry provider at {}",
                        registry_url
                    );
                    Arc::new(provider)
                }
            },
            None => {
                anyhow::bail!(
                    "Registry provider is required for server operation. \
                     Please configure a registry in settings (ECR or OCI client-auth)"
                )
            }
        };

        // Initialize OCI client for direct registry interaction
        let oci_client = Arc::new(
            crate::server::oci::OciClient::new().context("Failed to initialize OCI client")?,
        );
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

        // Validate cookie configuration at startup
        if !cookie_settings.domain.is_empty() {
            #[cfg(feature = "k8s")]
            if let Some(crate::server::settings::DeploymentControllerSettings::Kubernetes {
                auth_signin_url: signin_url,
                ..
            }) = &settings.deployment_controller
            {
                if let Ok(parsed) = url::Url::parse(signin_url) {
                    if let Some(host) = parsed.host_str() {
                        let cookie_domain_normalized =
                            cookie_settings.domain.trim_start_matches('.');

                        if !host.ends_with(cookie_domain_normalized)
                            && host != cookie_domain_normalized
                        {
                            tracing::warn!(
                                "⚠ Cookie domain mismatch: cookie_domain='{}' but auth_signin_url host='{}'. \
                                 Cookies may not work correctly. Consider setting cookie_domain to '.{}' or \
                                 ensure signin URL uses a matching domain.",
                                cookie_settings.domain,
                                host,
                                host.split('.').skip(1).collect::<Vec<_>>().join(".")
                            );
                        }
                    }
                }

                // Warn if using localhost with cookie domain
                if signin_url.contains("localhost") || signin_url.contains("127.0.0.1") {
                    tracing::warn!(
                        "⚠ Cookie domain '{}' set but auth_signin_url uses localhost. \
                         Use a proper domain name (e.g., rise.local) instead of localhost for cookie sharing.",
                        cookie_settings.domain
                    );
                }
            }
        }

        let public_url = settings.server.public_url.clone();
        let internal_url = settings
            .server
            .internal_url
            .clone()
            .unwrap_or_else(|| public_url.clone());
        tracing::info!("Public URL: {}", public_url);
        tracing::info!("Internal URL: {}", internal_url);

        // Initialize encryption provider
        let encryption_provider = init_encryption_provider(settings.encryption.as_ref()).await?;

        // Initialize deployment backend
        #[cfg(not(feature = "k8s"))]
        compile_error!(
            "At least one deployment backend must be enabled. Please build with --features k8s"
        );

        #[cfg(feature = "k8s")]
        let deployment_backend = {
            let controller_state = Arc::new(ControllerState {
                db_pool: db_pool.clone(),
                encryption_provider: encryption_provider.clone(),
            });
            init_kubernetes_backend(
                settings,
                controller_state,
                registry_provider.clone(),
                jwks_json.clone(),
            )
            .await?
        };

        // Initialize extension registry
        #[allow(unused_mut)]
        let mut extension_registry = crate::server::extensions::registry::ExtensionRegistry::new();

        // Register extensions from configuration
        if let Some(ref extensions_config) = settings.extensions {
            #[allow(clippy::never_loop)]
            for provider_config in &extensions_config.providers {
                match provider_config {
                    #[cfg(feature = "aws")]
                    crate::server::settings::ExtensionProviderConfig::AwsRdsProvisioner {
                        region,
                        instance_size,
                        disk_size,
                        instance_id_template,
                        instance_id_prefix,
                        default_engine_version,
                        vpc_security_group_ids,
                        db_subnet_group_name,
                        backup_retention_days,
                        backup_window,
                        maintenance_window,
                        access_key_id,
                        secret_access_key,
                    } => {
                        tracing::info!("Initializing AWS RDS extension provider");

                        // Create AWS config
                        let mut aws_config_builder =
                            aws_config::defaults(aws_config::BehaviorVersion::latest())
                                .region(aws_config::Region::new(region.clone()));

                        // Use explicit credentials if provided
                        if let (Some(key_id), Some(secret_key)) = (access_key_id, secret_access_key)
                        {
                            aws_config_builder = aws_config_builder.credentials_provider(
                                aws_sdk_sts::config::Credentials::new(
                                    key_id,
                                    secret_key,
                                    None,
                                    None,
                                    "static-credentials",
                                ),
                            );
                        }

                        let aws_config = aws_config_builder.load().await;
                        let rds_client = aws_sdk_rds::Client::new(&aws_config);

                        // Get encryption provider (required for RDS)
                        let encryption_provider = encryption_provider.clone().ok_or_else(|| {
                            anyhow::anyhow!("Encryption provider required for AWS RDS extension")
                        })?;

                        // Create and register the extension
                        let aws_rds_provisioner =
                            crate::server::extensions::providers::aws_rds::AwsRdsProvisioner::new(
                                crate::server::extensions::providers::aws_rds::AwsRdsProvisionerConfig {
                                    rds_client,
                                    db_pool: db_pool.clone(),
                                    encryption_provider,
                                    region: region.clone(),
                                    instance_size: instance_size.clone(),
                                    disk_size: *disk_size,
                                    instance_id_template: instance_id_template.clone(),
                                    instance_id_prefix: instance_id_prefix.clone(),
                                    default_engine_version: default_engine_version.clone(),
                                    vpc_security_group_ids: vpc_security_group_ids.clone(),
                                    db_subnet_group_name: db_subnet_group_name.clone(),
                                    backup_retention_days: *backup_retention_days,
                                    backup_window: backup_window.clone(),
                                    maintenance_window: maintenance_window.clone(),
                                }
                            )
                            .await?;

                        let aws_rds_arc: Arc<dyn crate::server::extensions::Extension> =
                            Arc::new(aws_rds_provisioner);
                        extension_registry.register_type(aws_rds_arc.clone());

                        // Start the extension's reconciliation loop
                        aws_rds_arc.start();

                        tracing::info!("AWS RDS extension provider initialized and started");
                    }
                    // When no extension provider features are enabled, this ensures the match is exhaustive
                    #[allow(unreachable_patterns)]
                    _ => {
                        // This pattern is only reachable when no extension features are enabled
                        // In that case, we skip unknown provider types silently
                    }
                }
            }
        }

        // Register OAuth provider (always enabled)
        tracing::info!("Initializing OAuth extension provider");
        let encryption_provider_for_oauth = encryption_provider
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Encryption provider required for OAuth extension"))?;

        let oauth_provider = crate::server::extensions::providers::oauth::OAuthProvider::new(
            crate::server::extensions::providers::oauth::OAuthProviderConfig {
                db_pool: db_pool.clone(),
                encryption_provider: encryption_provider_for_oauth,
                http_client: reqwest::Client::new(),
                api_domain: public_url.clone(),
                internal_url: internal_url.clone(),
            },
        );

        let oauth_provider_arc: Arc<dyn crate::server::extensions::Extension> =
            Arc::new(oauth_provider);
        extension_registry.register_type(oauth_provider_arc.clone());
        oauth_provider_arc.start();
        tracing::info!("OAuth extension provider initialized and started");

        // Register Snowflake OAuth provisioner (if configured)
        #[cfg(feature = "snowflake")]
        if let Some(ref extensions_config) = settings.extensions {
            for provider_config in &extensions_config.providers {
                #[allow(irrefutable_let_patterns)]
                if let crate::server::settings::ExtensionProviderConfig::SnowflakeOAuthProvisioner {
                    account,
                    user,
                    role,
                    warehouse,
                    auth,
                    integration_name_prefix,
                    default_blocked_roles,
                    default_scopes,
                    refresh_token_validity_seconds,
                } = provider_config
                {
                    tracing::info!("Initializing Snowflake OAuth provisioner");

                    let snowflake_oauth_provisioner =
                        crate::server::extensions::providers::snowflake_oauth::SnowflakeOAuthProvisioner::new(
                            crate::server::extensions::providers::snowflake_oauth::SnowflakeOAuthProvisionerConfig {
                                db_pool: db_pool.clone(),
                                encryption_provider: encryption_provider.clone()
                                    .ok_or_else(|| anyhow::anyhow!("Encryption provider required for Snowflake OAuth provisioner"))?,
                                http_client: reqwest::Client::new(),
                                api_domain: public_url.clone(),
                                oauth_provider: Some(oauth_provider_arc.clone()),
                                account: account.clone(),
                                user: user.clone(),
                                role: role.clone(),
                                warehouse: warehouse.clone(),
                                auth: auth.clone(),
                                integration_name_prefix: integration_name_prefix.clone(),
                                default_blocked_roles: default_blocked_roles.clone(),
                                default_scopes: default_scopes.clone(),
                                refresh_token_validity_seconds: *refresh_token_validity_seconds,
                            },
                        );

                    // Validate credentials during startup - fail fast if invalid
                    snowflake_oauth_provisioner
                        .validate_credentials()
                        .await
                        .context("Failed to validate Snowflake credentials during startup")?;

                    let snowflake_oauth_arc: Arc<dyn crate::server::extensions::Extension> =
                        Arc::new(snowflake_oauth_provisioner);
                    extension_registry.register_type(snowflake_oauth_arc.clone());
                    snowflake_oauth_arc.start();
                    tracing::info!("Snowflake OAuth provisioner initialized and started");
                }
            }
        }

        let extension_registry = Arc::new(extension_registry);

        // Initialize OAuth state store for OAuth extension (10 minute TTL)
        let oauth_state_store = Arc::new(
            moka::future::Cache::builder()
                .time_to_live(Duration::from_secs(600))
                .max_capacity(10_000) // Prevent memory exhaustion
                .build(),
        );
        tracing::info!("Initialized OAuth state store for OAuth extensions");

        // Initialize OAuth authorization code store (5 minute TTL, single-use)
        let oauth_code_store = Arc::new(
            moka::future::Cache::builder()
                .time_to_live(Duration::from_secs(300))
                .max_capacity(10_000) // Prevent memory exhaustion
                .build(),
        );
        tracing::info!("Initialized OAuth authorization code store for secure backend flow");

        // Extract access_classes from deployment controller settings
        // Filter out null values (used to remove inherited access classes)
        let (access_classes, production_ingress_url_template, staging_ingress_url_template) =
            if let Some(crate::server::settings::DeploymentControllerSettings::Kubernetes {
                access_classes,
                production_ingress_url_template,
                staging_ingress_url_template,
                ..
            }) = &settings.deployment_controller
            {
                let filtered: std::collections::HashMap<_, _> = access_classes
                    .iter()
                    .filter_map(|(k, v)| v.as_ref().map(|ac| (k.clone(), ac.clone())))
                    .collect();
                (
                    Arc::new(filtered),
                    Some(production_ingress_url_template.clone()),
                    staging_ingress_url_template.clone(),
                )
            } else {
                (Arc::new(std::collections::HashMap::new()), None, None)
            };

        Ok(Self {
            db_pool,
            jwt_validator,
            jwt_signer,
            jwks_json,
            oauth_client,
            registry_provider,
            oci_client,
            admin_users,
            auth_settings,
            server_settings,
            token_store,
            cookie_settings,
            public_url,
            internal_url,
            encryption_provider,
            deployment_backend,
            extension_registry,
            oauth_state_store,
            oauth_code_store,
            access_classes,
            production_ingress_url_template,
            staging_ingress_url_template,
        })
    }
}
