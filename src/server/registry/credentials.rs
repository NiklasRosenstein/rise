#[cfg(feature = "docker")]
use async_trait::async_trait;
#[cfg(feature = "docker")]
use std::sync::Arc;

/// Provider for registry authentication credentials
///
/// This trait allows components like Docker controller and OciClient
/// to fetch credentials without being tightly coupled to RegistryProvider
/// or AppState.
#[cfg(feature = "docker")]
#[async_trait]
pub trait CredentialsProvider: Send + Sync {
    /// Get credentials for a specific registry host
    ///
    /// Returns Some((username, password)) if credentials are available,
    /// None if the registry doesn't match or no provider is configured
    async fn get_credentials(
        &self,
        registry_host: &str,
    ) -> anyhow::Result<Option<(String, String)>>;
}

/// Optional credentials provider wrapper
#[cfg(feature = "docker")]
pub type OptionalCredentialsProvider = Option<Arc<dyn CredentialsProvider>>;

/// Adapter that wraps a RegistryProvider to implement CredentialsProvider
#[cfg(feature = "docker")]
pub struct RegistryCredentialsAdapter {
    provider: Arc<dyn super::RegistryProvider>,
}

#[cfg(feature = "docker")]
impl RegistryCredentialsAdapter {
    pub fn new(provider: Arc<dyn super::RegistryProvider>) -> Self {
        Self { provider }
    }
}

#[cfg(feature = "docker")]
#[async_trait]
impl CredentialsProvider for RegistryCredentialsAdapter {
    async fn get_credentials(
        &self,
        registry_host: &str,
    ) -> anyhow::Result<Option<(String, String)>> {
        // Check if this registry matches our provider's registry
        if registry_host != self.provider.registry_host() {
            return Ok(None);
        }

        // Get credentials from the provider
        let (username, password) = self.provider.get_pull_credentials().await?;
        Ok(Some((username, password)))
    }
}
