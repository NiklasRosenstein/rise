use anyhow::{Context, Result};
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_ecr::Client as EcrClient;
use aws_sdk_sts::Client as StsClient;
use base64::Engine;

use crate::registry::{
    models::{EcrConfig, RegistryCredentials},
    RegistryProvider,
};

/// AWS ECR registry provider with scoped credentials via STS AssumeRole
pub struct EcrProvider {
    config: EcrConfig,
    sts_client: StsClient,
    registry_url: String,
}

impl EcrProvider {
    /// Create a new ECR provider
    pub async fn new(config: EcrConfig) -> Result<Self> {
        // Build AWS config
        let aws_config = if let (Some(access_key), Some(secret_key)) =
            (&config.access_key_id, &config.secret_access_key)
        {
            // Use static credentials if provided
            let creds =
                aws_sdk_ecr::config::Credentials::new(access_key, secret_key, None, None, "static");
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

        let sts_client = StsClient::new(&aws_config);

        // Build registry_url: {account}.dkr.ecr.{region}.amazonaws.com/{repository}[/{prefix}]
        let base_url = format!(
            "{}.dkr.ecr.{}.amazonaws.com/{}",
            config.account_id, config.region, config.repository
        );
        let registry_url = if config.prefix.is_empty() {
            base_url
        } else {
            format!("{}/{}", base_url, config.prefix)
        };

        Ok(Self {
            config,
            sts_client,
            registry_url,
        })
    }

    /// Get ECR authorization token using the provided ECR client
    async fn get_ecr_auth_token(&self, client: &EcrClient) -> Result<RegistryCredentials> {
        let response = client
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

        let decoded_str = String::from_utf8(decoded).context("ECR token is not valid UTF-8")?;

        let parts: Vec<&str> = decoded_str.splitn(2, ':').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid ECR token format");
        }

        let username = parts[0].to_string();
        let password = parts[1].to_string();

        // ECR tokens are valid for 12 hours
        let expires_in = Some(12 * 60 * 60); // 12 hours in seconds

        Ok(RegistryCredentials {
            registry_url: self.registry_url.clone(),
            username,
            password,
            expires_in,
        })
    }
}

#[async_trait]
impl RegistryProvider for EcrProvider {
    async fn get_credentials(&self, repository: &str) -> Result<RegistryCredentials> {
        tracing::info!(
            "Getting scoped ECR credentials for repository: {}",
            repository
        );

        // Determine the full image path: {repository}/{prefix}/{project} or {repository}/{project}
        let full_path = if self.config.prefix.is_empty() {
            format!("{}/{}", self.config.repository, repository)
        } else {
            format!(
                "{}/{}/{}",
                self.config.repository, self.config.prefix, repository
            )
        };

        // Generate scoped credentials via AssumeRole with inline session policy
        let repo_arn = format!(
            "arn:aws:ecr:{}:{}:repository/{}*",
            self.config.region, self.config.account_id, full_path
        );

        let inline_policy = serde_json::json!({
            "Version": "2012-10-17",
            "Statement": [{
                "Effect": "Allow",
                "Action": [
                    "ecr:GetAuthorizationToken"
                ],
                "Resource": "*"
            }, {
                "Effect": "Allow",
                "Action": [
                    "ecr:BatchCheckLayerAvailability",
                    "ecr:InitiateLayerUpload",
                    "ecr:UploadLayerPart",
                    "ecr:CompleteLayerUpload",
                    "ecr:PutImage",
                    "ecr:BatchGetImage",
                    "ecr:GetDownloadUrlForLayer"
                ],
                "Resource": repo_arn
            }]
        });

        let assumed_role = self
            .sts_client
            .assume_role()
            .role_arn(&self.config.push_role_arn)
            .role_session_name(format!("rise-push-{}", repository))
            .policy(inline_policy.to_string())
            .send()
            .await
            .context("Failed to assume ECR push role")?;

        let creds = assumed_role
            .credentials()
            .context("No credentials in AssumeRole response")?;

        // Create ECR client with scoped credentials
        // Convert AWS DateTime to SystemTime
        let expiration: Option<std::time::SystemTime> =
            std::time::SystemTime::try_from(creds.expiration().clone()).ok();

        let scoped_creds = aws_sdk_ecr::config::Credentials::new(
            creds.access_key_id(),
            creds.secret_access_key(),
            Some(creds.session_token().to_string()),
            expiration,
            "assume_role",
        );

        let scoped_aws_config = aws_config::defaults(BehaviorVersion::latest())
            .credentials_provider(scoped_creds)
            .region(aws_config::Region::new(self.config.region.clone()))
            .load()
            .await;

        let scoped_ecr_client = EcrClient::new(&scoped_aws_config);

        // Get ECR auth token with scoped credentials
        self.get_ecr_auth_token(&scoped_ecr_client).await
    }

    fn registry_type(&self) -> &str {
        "ecr"
    }

    fn registry_url(&self) -> &str {
        &self.registry_url
    }
}
