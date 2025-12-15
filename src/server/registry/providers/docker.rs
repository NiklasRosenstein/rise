use anyhow::Result;
use async_trait::async_trait;

use crate::server::registry::{
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
    #[allow(dead_code)]
    config: OciClientAuthConfig,
    registry_url: String,
    registry_host: String,
    client_registry_url: String,
}

impl OciClientAuthProvider {
    /// Create a new OCI client-auth registry provider
    pub fn new(config: OciClientAuthConfig) -> Result<Self> {
        // Extract host from registry_url (remove protocol and path)
        let registry_host = config
            .registry_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or(&config.registry_url)
            .to_string();

        let registry_url = format!(
            "{}/{}",
            config.registry_url.trim_end_matches('/'),
            config.namespace
        );

        // Calculate client-facing registry URL (use client_registry_url if provided, otherwise use registry_url)
        let client_base = config
            .client_registry_url
            .as_ref()
            .unwrap_or(&config.registry_url);
        let client_registry_url =
            format!("{}/{}", client_base.trim_end_matches('/'), config.namespace);

        Ok(Self {
            config,
            registry_url,
            registry_host,
            client_registry_url,
        })
    }
}

#[async_trait]
impl RegistryProvider for OciClientAuthProvider {
    async fn get_credentials(&self, repository: &str) -> Result<RegistryCredentials> {
        tracing::info!("Returning OCI registry info for repository: {}", repository);

        // Return client-facing registry URL - credentials assumed to be configured via docker login
        // The client_registry_url is used for push operations, while registry_url is used by deployment controllers
        Ok(RegistryCredentials {
            registry_url: self.client_registry_url.clone(),
            username: String::new(), // Empty - docker CLI uses stored credentials
            password: String::new(), // Empty - docker CLI uses stored credentials
            expires_in: None,
        })
    }

    async fn get_pull_credentials(&self) -> Result<(String, String)> {
        // Client-auth provider assumes docker login was used
        // Return empty credentials - the docker CLI will use stored credentials
        Ok((String::new(), String::new()))
    }

    fn registry_host(&self) -> &str {
        &self.registry_host
    }

    fn registry_url(&self) -> &str {
        &self.registry_url
    }
}
