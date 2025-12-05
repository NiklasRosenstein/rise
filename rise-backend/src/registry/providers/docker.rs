use anyhow::Result;
use async_trait::async_trait;

use crate::registry::{
    models::{OciClientAuthConfig, RegistryCredentials},
    RegistryProvider,
};

/// OCI registry provider that relies on client-side authentication
///
/// This provider assumes the user has already authenticated via `docker login`
/// or equivalent. It simply returns the registry URL - no credential generation.
///
/// Works with any OCI-compliant registry (Docker Hub, Harbor, Quay, etc.)
pub struct OciClientAuthProvider {
    config: OciClientAuthConfig,
    registry_url: String,
}

impl OciClientAuthProvider {
    /// Create a new OCI client-auth registry provider
    pub fn new(config: OciClientAuthConfig) -> Result<Self> {
        let registry_url = format!(
            "{}/{}",
            config.registry_url.trim_end_matches('/'),
            config.namespace
        );
        Ok(Self {
            config,
            registry_url,
        })
    }
}

#[async_trait]
impl RegistryProvider for OciClientAuthProvider {
    async fn get_credentials(&self, repository: &str) -> Result<RegistryCredentials> {
        tracing::info!("Returning OCI registry info for repository: {}", repository);

        // Return registry URL - credentials assumed to be configured via docker login
        Ok(RegistryCredentials {
            registry_url: self.registry_url.clone(),
            username: String::new(), // Empty - docker CLI uses stored credentials
            password: String::new(), // Empty - docker CLI uses stored credentials
            expires_in: None,
        })
    }

    fn registry_type(&self) -> &str {
        "oci-client-auth"
    }

    fn registry_url(&self) -> &str {
        &self.registry_url
    }
}
