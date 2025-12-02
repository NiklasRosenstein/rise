use async_trait::async_trait;
use anyhow::Result;

use crate::registry::{RegistryProvider, models::{RegistryCredentials, DockerConfig}};

/// Generic Docker registry provider
///
/// Assumes the user has already authenticated via `docker login`.
/// This provider simply returns the registry URL - no credential generation.
pub struct DockerProvider {
    config: DockerConfig,
}

impl DockerProvider {
    /// Create a new Docker registry provider
    pub fn new(config: DockerConfig) -> Result<Self> {
        Ok(Self { config })
    }

    /// Get the registry URL
    fn get_registry_url(&self) -> String {
        format!("{}/{}", self.config.registry_url.trim_end_matches('/'), self.config.namespace)
    }
}

#[async_trait]
impl RegistryProvider for DockerProvider {
    async fn get_credentials(&self, repository: &str) -> Result<RegistryCredentials> {
        tracing::info!("Returning Docker registry info for repository: {}", repository);

        // Return registry URL - credentials assumed to be configured via docker login
        Ok(RegistryCredentials {
            registry_url: self.get_registry_url(),
            username: String::new(),  // Empty - docker CLI uses stored credentials
            password: String::new(),  // Empty - docker CLI uses stored credentials
            expires_in: None,
        })
    }

    fn registry_type(&self) -> &str {
        "docker"
    }

    fn registry_url(&self) -> &str {
        "docker"
    }
}
