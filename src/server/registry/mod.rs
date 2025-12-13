pub mod credentials;
pub mod handlers;
pub mod models;
pub mod providers;
pub mod routes;

#[cfg(feature = "docker")]
pub use credentials::{
    CredentialsProvider, OptionalCredentialsProvider, RegistryCredentialsAdapter,
};

use crate::server::registry::models::RegistryCredentials;
use anyhow::Result;
use async_trait::async_trait;

/// Trait for container registry providers
#[async_trait]
pub trait RegistryProvider: Send + Sync {
    /// Get temporary credentials for pushing images (scoped to repository)
    ///
    /// # Arguments
    /// * `repository` - The repository name (e.g., "my-app")
    ///
    /// # Returns
    /// Registry credentials including username, password, and registry URL
    async fn get_credentials(&self, repository: &str) -> Result<RegistryCredentials>;

    /// Get credentials for pulling/reading images (registry-wide)
    ///
    /// Used for resolving image digests. Returns (username, password) tuple.
    /// Returns empty strings if no credentials are available (e.g., anonymous access).
    async fn get_pull_credentials(&self) -> Result<(String, String)>;

    /// Get the registry host (for credentials map key)
    ///
    /// Returns the registry hostname without protocol or path
    /// (e.g., "459109751375.dkr.ecr.eu-west-1.amazonaws.com")
    fn registry_host(&self) -> &str;

    /// Get the base registry URL
    fn registry_url(&self) -> &str;
}
