use anyhow::{Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_ecr::Client as EcrClient;

use crate::server::registry::models::EcrConfig;

/// Extract a clean error message from an AWS SDK error's Debug output
///
/// The AWS SDK errors have verbose Debug output, but we can extract just the
/// meaningful message by parsing for the `message: Some("...")` pattern.
fn format_sdk_error<E: std::fmt::Debug>(err: &E) -> String {
    let debug_str = format!("{:?}", err);

    // Try to extract the message field from the debug output
    // Pattern: message: Some("actual error message")
    if let Some(start) = debug_str.find("message: Some(\"") {
        let start = start + 15; // length of 'message: Some("'
        if let Some(end) = debug_str[start..].find("\")") {
            return debug_str[start..start + end].to_string();
        }
    }

    // Fallback: try to find just a Message field (as in JSON response)
    if let Some(start) = debug_str.find("\"Message\":\"") {
        let start = start + 11; // length of '"Message":"'
        if let Some(end) = debug_str[start..].find("\"") {
            return debug_str[start..start + end].to_string();
        }
    }

    // Last resort: return a truncated debug string
    if debug_str.len() > 200 {
        format!("{}...", &debug_str[..200])
    } else {
        debug_str
    }
}

/// Manages ECR repository lifecycle operations
///
/// This manager handles creating, deleting, and tagging ECR repositories.
/// It uses STS AssumeRole for all operations, with the assumed role having
/// permissions to manage repositories within the configured prefix.
pub struct EcrRepoManager {
    config: EcrConfig,
    ecr_client: EcrClient,
}

impl EcrRepoManager {
    /// Create a new ECR repository manager
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

        let ecr_client = EcrClient::new(&aws_config);

        Ok(Self { config, ecr_client })
    }

    /// Get the full ECR repository name for a project
    ///
    /// # Example
    /// With repo_prefix = "rise/" and project = "hello":
    /// Returns "rise/hello"
    pub fn repo_name(&self, project: &str) -> String {
        format!("{}{}", self.config.repo_prefix, project)
    }

    /// Get the full ECR repository ARN for a project
    pub fn repo_arn(&self, project: &str) -> String {
        format!(
            "arn:aws:ecr:{}:{}:repository/{}",
            self.config.region,
            self.config.account_id,
            self.repo_name(project)
        )
    }

    /// Check if an ECR repository exists for a project
    pub async fn repository_exists(&self, project: &str) -> Result<bool> {
        let repo_name = self.repo_name(project);

        match self
            .ecr_client
            .describe_repositories()
            .repository_names(&repo_name)
            .send()
            .await
        {
            Ok(response) => Ok(!response.repositories().is_empty()),
            Err(err) => {
                // Check if error is RepositoryNotFoundException
                if let Some(service_err) = err.as_service_error() {
                    if service_err.is_repository_not_found_exception() {
                        return Ok(false);
                    }
                }
                Err(anyhow::anyhow!(
                    "Failed to check ECR repository existence for '{}': {}",
                    repo_name,
                    format_sdk_error(&err)
                ))
            }
        }
    }

    /// Create an ECR repository for a project
    ///
    /// Creates the repository with tags for management:
    /// - rise:managed = "true"
    /// - rise:project = "{project_name}"
    ///
    /// Returns true if created, false if already exists.
    pub async fn create_repository(&self, project: &str) -> Result<bool> {
        let repo_name = self.repo_name(project);

        // Check if already exists
        if self.repository_exists(project).await? {
            tracing::debug!("ECR repository {} already exists", repo_name);
            return Ok(false);
        }

        tracing::info!("Creating ECR repository: {}", repo_name);

        // Build tags
        let managed_tag = aws_sdk_ecr::types::Tag::builder()
            .key("rise:managed")
            .value("true")
            .build()
            .context("Failed to build managed tag")?;

        let project_tag = aws_sdk_ecr::types::Tag::builder()
            .key("rise:project")
            .value(project)
            .build()
            .context("Failed to build project tag")?;

        // Create the repository with tags
        self.ecr_client
            .create_repository()
            .repository_name(&repo_name)
            .tags(managed_tag)
            .tags(project_tag)
            .image_scanning_configuration(
                aws_sdk_ecr::types::ImageScanningConfiguration::builder()
                    .scan_on_push(true)
                    .build(),
            )
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to create ECR repository '{}': {}",
                    repo_name,
                    format_sdk_error(&e)
                )
            })?;

        tracing::info!("Created ECR repository: {}", repo_name);
        Ok(true)
    }

    /// Delete an ECR repository for a project
    ///
    /// Deletes the repository and all images within it.
    /// Returns true if deleted, false if it didn't exist.
    pub async fn delete_repository(&self, project: &str) -> Result<bool> {
        let repo_name = self.repo_name(project);

        // Check if exists
        if !self.repository_exists(project).await? {
            tracing::debug!(
                "ECR repository {} does not exist, nothing to delete",
                repo_name
            );
            return Ok(false);
        }

        tracing::info!("Deleting ECR repository: {}", repo_name);

        // Delete the repository (force = true deletes images too)
        self.ecr_client
            .delete_repository()
            .repository_name(&repo_name)
            .force(true)
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to delete ECR repository '{}': {}",
                    repo_name,
                    format_sdk_error(&e)
                )
            })?;

        tracing::info!("Deleted ECR repository: {}", repo_name);
        Ok(true)
    }

    /// Tag a repository as orphaned (when soft-delete is configured)
    ///
    /// Adds the tag rise:orphaned = "true" to the repository.
    /// This marks the repository for manual cleanup instead of automatic deletion.
    ///
    /// Returns true if tagged, false if repository doesn't exist.
    pub async fn tag_as_orphaned(&self, project: &str) -> Result<bool> {
        // Check if repository exists first
        if !self.repository_exists(project).await? {
            tracing::debug!(
                "ECR repository {} does not exist, nothing to tag",
                self.repo_name(project)
            );
            return Ok(false);
        }

        let repo_arn = self.repo_arn(project);

        tracing::info!(
            "Tagging ECR repository as orphaned: {}",
            self.repo_name(project)
        );

        let orphaned_tag = aws_sdk_ecr::types::Tag::builder()
            .key("rise:orphaned")
            .value("true")
            .build()
            .context("Failed to build orphaned tag")?;

        self.ecr_client
            .tag_resource()
            .resource_arn(&repo_arn)
            .tags(orphaned_tag)
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to tag ECR repository '{}' as orphaned: {}",
                    self.repo_name(project),
                    format_sdk_error(&e)
                )
            })?;

        Ok(true)
    }

    /// Get whether auto_remove is enabled
    pub fn auto_remove(&self) -> bool {
        self.config.auto_remove
    }
}
