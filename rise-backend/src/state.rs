use crate::settings::{Settings, RegistrySettings};
use crate::registry::{RegistryProvider, providers::{EcrProvider, ArtifactoryProvider}, models::{EcrConfig, ArtifactoryConfig}};
use std::sync::Arc;
use pocketbase_sdk::client::Client as PocketbaseClient;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub http_client: Arc<reqwest::Client>,
    pub pocketbase_url: String,
    pub pb_client: Arc<PocketbaseClient>,
    pub registry_provider: Option<Arc<dyn RegistryProvider>>,
}

impl AppState {
    pub async fn new(settings: &Settings) -> Self {
        let http_client = reqwest::Client::new();
        let pb_client = PocketbaseClient::new(&settings.pocketbase.url);

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
                RegistrySettings::Artifactory { base_url, repository, username, password, use_credential_helper } => {
                    let artifactory_config = ArtifactoryConfig {
                        base_url: base_url.clone(),
                        repository: repository.clone(),
                        username: username.clone(),
                        password: password.clone(),
                        use_credential_helper: *use_credential_helper,
                    };
                    match ArtifactoryProvider::new(artifactory_config) {
                        Ok(provider) => {
                            tracing::info!("Initialized Artifactory registry provider");
                            Some(Arc::new(provider))
                        }
                        Err(e) => {
                            tracing::error!("Failed to initialize Artifactory provider: {}", e);
                            None
                        }
                    }
                }
            }
        } else {
            tracing::warn!("No registry configured - registry credentials endpoint will not be available");
            None
        };

        Self {
            settings: Arc::new(settings.clone()),
            http_client: Arc::new(http_client),
            pocketbase_url: settings.pocketbase.url.clone(),
            pb_client: Arc::new(pb_client),
            registry_provider,
        }
    }
}