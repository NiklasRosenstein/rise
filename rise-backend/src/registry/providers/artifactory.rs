use async_trait::async_trait;
use anyhow::{Result, Context, bail};
use std::process::Command;

use crate::registry::{RegistryProvider, models::{RegistryCredentials, ArtifactoryConfig}};

/// JFrog Artifactory registry provider
pub struct ArtifactoryProvider {
    config: ArtifactoryConfig,
}

impl ArtifactoryProvider {
    /// Create a new Artifactory provider
    pub fn new(config: ArtifactoryConfig) -> Result<Self> {
        // Validate config
        if !config.use_credential_helper && (config.username.is_none() || config.password.is_none()) {
            bail!("Artifactory config must provide username/password or enable credential helper");
        }

        Ok(Self { config })
    }

    /// Get the full registry URL
    fn get_registry_url(&self) -> String {
        // Artifactory Docker registry URL format: baseurl/repository
        format!("{}/{}", self.config.base_url.trim_end_matches('/'), self.config.repository)
    }

    /// Get credentials from Docker credential helper
    async fn get_credentials_from_helper(&self, registry_url: &str) -> Result<(String, String)> {
        tracing::info!("Getting Artifactory credentials from Docker credential helper");

        // Try to use docker-credential-helper
        // This is a simplified version - in production you'd want to properly detect which helper to use
        let output = Command::new("docker-credential-desktop")
            .arg("get")
            .arg(registry_url)
            .output()
            .context("Failed to execute docker-credential-desktop")?;

        if !output.status.success() {
            bail!("docker-credential-desktop failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let creds_json = String::from_utf8(output.stdout)
            .context("Failed to parse credential helper output")?;

        // Parse JSON response from credential helper
        #[derive(serde::Deserialize)]
        struct DockerCredentials {
            #[serde(rename = "Username")]
            username: String,
            #[serde(rename = "Secret")]
            secret: String,
        }

        let creds: DockerCredentials = serde_json::from_str(&creds_json)
            .context("Failed to parse Docker credentials JSON")?;

        Ok((creds.username, creds.secret))
    }
}

#[async_trait]
impl RegistryProvider for ArtifactoryProvider {
    async fn get_credentials(&self, repository: &str) -> Result<RegistryCredentials> {
        tracing::info!("Getting Artifactory credentials for repository: {}", repository);

        let registry_url = self.get_registry_url();

        let (username, password) = if self.config.use_credential_helper {
            // Get credentials from Docker credential helper
            self.get_credentials_from_helper(&registry_url).await?
        } else {
            // Use static credentials from config
            let username = self.config.username.clone()
                .context("Username not configured")?;
            let password = self.config.password.clone()
                .context("Password not configured")?;
            (username, password)
        };

        // Artifactory tokens don't expire (unless using temporary tokens, which we can add later)
        Ok(RegistryCredentials {
            registry_url,
            username,
            password,
            expires_in: None,
        })
    }

    fn registry_type(&self) -> &str {
        "artifactory"
    }

    fn registry_url(&self) -> &str {
        // Note: We can't return a reference to a temporary string
        // This is a limitation we'll need to address in the trait design
        "artifactory"
    }
}
