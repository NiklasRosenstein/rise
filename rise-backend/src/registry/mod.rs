pub mod handlers;
pub mod models;
pub mod providers;
pub mod routes;

use crate::registry::models::RegistryCredentials;
use anyhow::Result;
use async_trait::async_trait;

/// Trait for container registry providers
#[async_trait]
pub trait RegistryProvider: Send + Sync {
    /// Get temporary credentials for pushing images
    ///
    /// # Arguments
    /// * `repository` - The repository name (e.g., "my-app")
    ///
    /// # Returns
    /// Registry credentials including username, password, and registry URL
    async fn get_credentials(&self, repository: &str) -> Result<RegistryCredentials>;

    /// Get the registry type identifier
    fn registry_type(&self) -> &str;

    /// Get the base registry URL
    fn registry_url(&self) -> &str;
}
