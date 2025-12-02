use async_trait::async_trait;
use anyhow::{Result, Context};
use aws_config::BehaviorVersion;
use aws_sdk_ecr::Client as EcrClient;
use base64::Engine;

use crate::registry::{RegistryProvider, models::{RegistryCredentials, EcrConfig}};

/// AWS ECR registry provider
pub struct EcrProvider {
    config: EcrConfig,
    client: EcrClient,
}

impl EcrProvider {
    /// Create a new ECR provider
    pub async fn new(config: EcrConfig) -> Result<Self> {
        // Build AWS config
        let aws_config = if let (Some(access_key), Some(secret_key)) =
            (&config.access_key_id, &config.secret_access_key) {
            // Use static credentials if provided
            let creds = aws_sdk_ecr::config::Credentials::new(
                access_key,
                secret_key,
                None,
                None,
                "static",
            );
            aws_config::defaults(BehaviorVersion::latest())
                .credentials_provider(creds)
                .region(aws_config::Region::new(config.region.clone()))
                .load()
                .await
        } else {
            // Use default credential chain (IAM role, env vars, etc.)
            aws_config::defaults(BehaviorVersion::latest())
                .region(aws_config::Region::new(config.region.clone()))
                .load()
                .await
        };

        let client = EcrClient::new(&aws_config);

        Ok(Self { config, client })
    }

    /// Get the ECR registry URL
    fn get_registry_url(&self) -> String {
        format!("{}.dkr.ecr.{}.amazonaws.com", self.config.account_id, self.config.region)
    }
}

#[async_trait]
impl RegistryProvider for EcrProvider {
    async fn get_credentials(&self, repository: &str) -> Result<RegistryCredentials> {
        tracing::info!("Getting ECR credentials for repository: {}", repository);

        // Get authorization token from ECR
        let response = self.client
            .get_authorization_token()
            .send()
            .await
            .context("Failed to get ECR authorization token")?;

        let auth_data = response
            .authorization_data()
            .first()
            .context("No authorization data returned from ECR")?;

        let token = auth_data
            .authorization_token()
            .context("No authorization token in response")?;

        // Decode the base64 token (format is "AWS:password")
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(token)
            .context("Failed to decode ECR token")?;

        let decoded_str = String::from_utf8(decoded)
            .context("ECR token is not valid UTF-8")?;

        let parts: Vec<&str> = decoded_str.splitn(2, ':').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid ECR token format");
        }

        let username = parts[0].to_string();
        let password = parts[1].to_string();

        // ECR tokens are valid for 12 hours
        let expires_in = Some(12 * 60 * 60); // 12 hours in seconds

        Ok(RegistryCredentials {
            registry_url: self.get_registry_url(),
            username,
            password,
            expires_in,
        })
    }

    fn registry_type(&self) -> &str {
        "ecr"
    }

    fn registry_url(&self) -> &str {
        // Note: We can't return a reference to a temporary string, so we'll need to change this
        // For now, returning a static string that will be replaced
        "ecr"
    }
}
