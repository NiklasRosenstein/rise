use crate::db::{
    self, deployments as db_deployments, env_vars as db_env_vars, extensions as db_extensions,
    projects as db_projects,
};
use crate::server::encryption::EncryptionProvider;
use crate::server::extensions::Extension;
use crate::server::settings::{S3AccessMode, S3DeletionPolicy, S3EncryptionConfig};
use anyhow::{Context, Result};
use async_trait::async_trait;
use aws_sdk_iam::Client as IamClient;
use aws_sdk_s3::Client as S3Client;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// Constants
const S3_IAM_USERNAME_PREFIX: &str = "rise-s3";

/// User-facing spec structure (configured per extension instance)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AwsS3Spec {
    /// Bucket strategy: Shared (one bucket per project) or Isolated (one per deployment group)
    #[serde(default = "default_bucket_strategy")]
    pub bucket_strategy: BucketStrategy,
    /// Enable bucket versioning
    #[serde(default)]
    pub versioning: bool,
    /// Environment variable names to inject
    #[serde(default = "default_env_vars")]
    pub env_vars: S3EnvVars,
    /// Optional lifecycle rules for bucket objects
    #[serde(default)]
    pub lifecycle_rules: Vec<LifecycleRule>,
    /// Public access block settings (default: all blocked)
    #[serde(default = "default_public_access_block")]
    pub public_access_block: PublicAccessBlockConfig,
    /// Optional CORS configuration
    #[serde(default)]
    pub cors: Option<Vec<CorsRule>>,
}

/// Bucket strategy: Shared or Isolated
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BucketStrategy {
    Shared,
    Isolated,
}

fn default_bucket_strategy() -> BucketStrategy {
    BucketStrategy::Shared
}

/// Environment variable names to inject
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct S3EnvVars {
    #[serde(default = "default_bucket_name_var")]
    pub bucket_name: Option<String>,
    #[serde(default = "default_region_var")]
    pub region: Option<String>,
    #[serde(default = "default_access_key_id_var")]
    pub access_key_id: Option<String>,
    #[serde(default = "default_secret_access_key_var")]
    pub secret_access_key: Option<String>,
    #[serde(default = "default_role_arn_var")]
    pub role_arn: Option<String>,
}

fn default_env_vars() -> S3EnvVars {
    S3EnvVars {
        bucket_name: default_bucket_name_var(),
        region: default_region_var(),
        access_key_id: default_access_key_id_var(),
        secret_access_key: default_secret_access_key_var(),
        role_arn: default_role_arn_var(),
    }
}

fn default_bucket_name_var() -> Option<String> {
    Some("AWS_S3_BUCKET".to_string())
}

fn default_region_var() -> Option<String> {
    Some("AWS_REGION".to_string())
}

fn default_access_key_id_var() -> Option<String> {
    Some("AWS_ACCESS_KEY_ID".to_string())
}

fn default_secret_access_key_var() -> Option<String> {
    Some("AWS_SECRET_ACCESS_KEY".to_string())
}

fn default_role_arn_var() -> Option<String> {
    Some("AWS_ROLE_ARN".to_string())
}

/// Lifecycle rule configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LifecycleRule {
    pub id: String,
    #[serde(default)]
    pub prefix: String,
    pub expiration_days: i32,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Public access block configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PublicAccessBlockConfig {
    #[serde(default = "default_true")]
    pub block_public_acls: bool,
    #[serde(default = "default_true")]
    pub ignore_public_acls: bool,
    #[serde(default = "default_true")]
    pub block_public_policy: bool,
    #[serde(default = "default_true")]
    pub restrict_public_buckets: bool,
}

fn default_public_access_block() -> PublicAccessBlockConfig {
    PublicAccessBlockConfig {
        block_public_acls: true,
        ignore_public_acls: true,
        block_public_policy: true,
        restrict_public_buckets: true,
    }
}

/// CORS rule configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CorsRule {
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    #[serde(default)]
    pub allowed_headers: Vec<String>,
    #[serde(default)]
    pub max_age_seconds: Option<i32>,
}

/// Status structure (tracked by reconciliation loop)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AwsS3Status {
    /// Current state of the S3 provisioner
    pub state: S3State,
    /// Map of bucket names to their status
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub buckets: HashMap<String, BucketStatus>,
    /// IAM user credentials (if using IAM user mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iam_user: Option<IamUserStatus>,
    /// Last error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Default for AwsS3Status {
    fn default() -> Self {
        Self {
            state: S3State::Pending,
            buckets: HashMap::new(),
            iam_user: None,
            error: None,
        }
    }
}

/// S3 provisioner state machine
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum S3State {
    Pending,
    CreatingIamUser,
    CreatingAccessKeys,
    CreatingBuckets,
    ConfiguringBuckets,
    Available,
    Deleting,
    Deleted,
    Failed,
}

/// Bucket status structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BucketStatus {
    pub region: String,
    pub status: BucketState,
    /// Timestamp when cleanup was scheduled (for inactive deployment groups in isolated mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_scheduled_at: Option<DateTime<Utc>>,
}

/// Bucket provisioning state
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum BucketState {
    Pending,
    Creating,
    Configuring,
    Available,
    Deleting,
}

/// IAM user status (encrypted credentials)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IamUserStatus {
    pub username: String,
    pub access_key_id_encrypted: String,
    pub secret_access_key_encrypted: String,
}

pub struct AwsS3ProvisionerConfig {
    pub s3_client: S3Client,
    pub iam_client: IamClient,
    pub db_pool: PgPool,
    pub encryption_provider: Arc<dyn EncryptionProvider>,
    pub default_region: String,
    pub bucket_prefix: String,
    pub encryption: S3EncryptionConfig,
    pub access_mode: S3AccessMode,
    pub deletion_policy: S3DeletionPolicy,
    pub iam_user_boundary_policy_arn: Option<String>,
}

#[derive(Clone)]
pub struct AwsS3Provisioner {
    s3_client: S3Client,
    iam_client: IamClient,
    db_pool: PgPool,
    encryption_provider: Arc<dyn EncryptionProvider>,
    default_region: String,
    bucket_prefix: String,
    encryption: S3EncryptionConfig,
    access_mode: S3AccessMode,
    deletion_policy: S3DeletionPolicy,
    iam_user_boundary_policy_arn: Option<String>,
}

impl AwsS3Provisioner {
    pub async fn new(config: AwsS3ProvisionerConfig) -> Result<Self> {
        Ok(Self {
            s3_client: config.s3_client,
            iam_client: config.iam_client,
            db_pool: config.db_pool,
            encryption_provider: config.encryption_provider,
            default_region: config.default_region,
            bucket_prefix: config.bucket_prefix,
            encryption: config.encryption,
            access_mode: config.access_mode,
            deletion_policy: config.deletion_policy,
            iam_user_boundary_policy_arn: config.iam_user_boundary_policy_arn,
        })
    }

    /// Get the finalizer name for this extension instance
    fn finalizer_name(&self, extension_name: &str) -> String {
        format!(
            "rise.dev/extension/{}/{}",
            self.extension_type(),
            extension_name
        )
    }

    /// Generate bucket name based on strategy
    fn bucket_name_for_deployment_group(
        &self,
        project_name: &str,
        deployment_group: &str,
        strategy: &BucketStrategy,
    ) -> String {
        // Sanitize project name and deployment group for bucket naming
        let safe_project = project_name.replace(['/', '-'], "_").to_lowercase();
        let safe_group = deployment_group.replace(['/', '-'], "_").to_lowercase();

        match strategy {
            BucketStrategy::Shared => {
                format!("{}-{}", self.bucket_prefix, safe_project)
            }
            BucketStrategy::Isolated => {
                format!("{}-{}-{}", self.bucket_prefix, safe_project, safe_group)
            }
        }
    }

    /// Reconcile a single S3 extension
    async fn reconcile_single(
        &self,
        project_extension: db::models::ProjectExtension,
    ) -> Result<bool> {
        debug!("Reconciling AWS S3 extension: {:?}", project_extension);
        let project = db_projects::find_by_id(&self.db_pool, project_extension.project_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Project not found"))?;

        // Parse spec
        let spec: AwsS3Spec = serde_json::from_value(project_extension.spec.clone())
            .context("Failed to parse AWS S3 spec")?;

        // Parse current status or create default
        let mut status: AwsS3Status =
            serde_json::from_value(project_extension.status.clone()).unwrap_or_default();

        // Check if marked for deletion
        if project_extension.deleted_at.is_some() {
            // Handle deletion
            if status.state != S3State::Deleted {
                self.handle_deletion(&mut status, &project.name, &project_extension.extension)
                    .await?;
                // Update status
                db_extensions::update_status(
                    &self.db_pool,
                    project_extension.project_id,
                    &project_extension.extension,
                    &serde_json::to_value(&status)?,
                )
                .await?;

                // If deletion is complete, hard delete the record and remove finalizer
                if status.state == S3State::Deleted {
                    let finalizer = self.finalizer_name(&project_extension.extension);

                    // Remove finalizer so project can be deleted
                    if let Err(e) = db_projects::remove_finalizer(
                        &self.db_pool,
                        project_extension.project_id,
                        &finalizer,
                    )
                    .await
                    {
                        error!(
                            "Failed to remove finalizer '{}' from project {}: {}",
                            finalizer, project.name, e
                        );
                    } else {
                        info!(
                            "Removed finalizer '{}' from project {}",
                            finalizer, project.name
                        );
                    }

                    db_extensions::delete_permanently(
                        &self.db_pool,
                        project_extension.project_id,
                        &project_extension.extension,
                    )
                    .await?;
                    info!(
                        "Permanently deleted extension record for project {}",
                        project.name
                    );
                }
            }
            return Ok(false); // Deletion complete, no more work
        }

        // Track if state changed during this reconciliation
        let initial_state = status.state.clone();

        // Handle normal lifecycle
        match status.state {
            S3State::Pending => {
                self.handle_pending(
                    &spec,
                    &mut status,
                    &project.name,
                    project.id,
                    &project_extension.extension,
                )
                .await?;
            }
            S3State::CreatingIamUser => {
                self.handle_creating_iam_user(&mut status, &project.name)
                    .await?;
            }
            S3State::CreatingAccessKeys => {
                self.handle_creating_access_keys(&mut status, &project.name)
                    .await?;
            }
            S3State::CreatingBuckets => {
                self.handle_creating_buckets(&spec, &mut status, &project.name)
                    .await?;
            }
            S3State::ConfiguringBuckets => {
                self.handle_configuring_buckets(&spec, &mut status).await?;
            }
            S3State::Available => {
                self.handle_available(&spec, &mut status, &project.name, project.id)
                    .await?;
            }
            S3State::Failed => {
                // Retry from Pending
                info!(
                    "S3 extension for project {} is in failed state, retrying from Pending",
                    project.name
                );
                status.state = S3State::Pending;
                status.error = None;
            }
            _ => {}
        }

        // Update status in database
        db_extensions::update_status(
            &self.db_pool,
            project_extension.project_id,
            &project_extension.extension,
            &serde_json::to_value(&status)?,
        )
        .await?;

        // Determine if more work can be done immediately
        let state_changed = status.state != initial_state;
        let needs_more_work = state_changed
            || matches!(
                status.state,
                S3State::CreatingIamUser
                    | S3State::CreatingAccessKeys
                    | S3State::CreatingBuckets
                    | S3State::ConfiguringBuckets
            );

        Ok(needs_more_work)
    }

    // State handlers will be implemented next
    async fn handle_pending(
        &self,
        _spec: &AwsS3Spec,
        status: &mut AwsS3Status,
        project_name: &str,
        project_id: Uuid,
        extension_name: &str,
    ) -> Result<()> {
        info!(
            "Starting S3 provisioning for project {} (extension: {})",
            project_name, extension_name
        );

        // Check if we need to create IAM user based on access_mode
        match &self.access_mode {
            S3AccessMode::IamUser | S3AccessMode::Both { .. } => {
                // Transition to creating IAM user
                status.state = S3State::CreatingIamUser;
            }
            S3AccessMode::IamRole { .. } => {
                // Skip IAM user creation, go directly to creating buckets
                status.state = S3State::CreatingBuckets;
            }
        }

        // Add finalizer immediately to ensure cleanup if project is deleted during provisioning
        let finalizer = self.finalizer_name(extension_name);
        if let Err(e) = db_projects::add_finalizer(&self.db_pool, project_id, &finalizer).await {
            error!(
                "Failed to add finalizer '{}' for project {}: {}",
                finalizer, project_name, e
            );
        } else {
            info!(
                "Added finalizer '{}' to project {}",
                finalizer, project_name
            );
        }

        Ok(())
    }

    async fn handle_creating_iam_user(
        &self,
        status: &mut AwsS3Status,
        project_name: &str,
    ) -> Result<()> {
        // Determine the username
        let username = if let Some(ref iam_user) = status.iam_user {
            iam_user.username.clone()
        } else {
            // Generate username based on extension
            // Note: We don't have access to extension_name here, so we use project name only
            format!(
                "{}-{}",
                S3_IAM_USERNAME_PREFIX,
                project_name.replace(['/', '-'], "_").to_lowercase()
            )
        };

        info!("Creating IAM user '{}' for S3 access", username);

        // Check if user already exists
        match self.iam_client.get_user().user_name(&username).send().await {
            Ok(_) => {
                info!("IAM user '{}' already exists", username);
                // User exists, move to creating access keys
                status.state = S3State::CreatingAccessKeys;
            }
            Err(e) => {
                let error_str = format!("{:?}", e);
                if error_str.contains("NoSuchEntity") {
                    // User doesn't exist, create it
                    let mut create_user_req = self.iam_client.create_user().user_name(&username);

                    // Set permission boundary if configured (prevents privilege escalation)
                    if let Some(ref boundary_arn) = self.iam_user_boundary_policy_arn {
                        info!(
                            "Setting permission boundary '{}' for IAM user '{}'",
                            boundary_arn, username
                        );
                        create_user_req = create_user_req.permissions_boundary(boundary_arn);
                    } else {
                        warn!(
                            "No permission boundary configured for IAM user '{}'. \
                            Consider setting iam_user_boundary_policy_arn to prevent privilege escalation.",
                            username
                        );
                    }

                    match create_user_req.send().await {
                        Ok(_) => {
                            info!("Created IAM user '{}'", username);

                            // Create inline policy for S3 access
                            // We'll create a broad policy now, and can refine it later when buckets are created
                            let policy_document = self.generate_iam_policy_document(project_name);

                            match self
                                .iam_client
                                .put_user_policy()
                                .user_name(&username)
                                .policy_name("S3Access")
                                .policy_document(&policy_document)
                                .send()
                                .await
                            {
                                Ok(_) => {
                                    info!("Attached S3 access policy to user '{}'", username);
                                    status.state = S3State::CreatingAccessKeys;
                                }
                                Err(e) => {
                                    error!(
                                        "Failed to attach policy to IAM user '{}': {:?}",
                                        username, e
                                    );
                                    status.state = S3State::Failed;
                                    status.error =
                                        Some(format!("Failed to attach IAM policy: {:?}", e));
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to create IAM user '{}': {:?}", username, e);
                            status.state = S3State::Failed;
                            status.error = Some(format!("Failed to create IAM user: {:?}", e));
                        }
                    }
                } else {
                    error!("Failed to check IAM user '{}': {:?}", username, e);
                    status.state = S3State::Failed;
                    status.error = Some(format!("Failed to check IAM user: {:?}", e));
                }
            }
        }

        Ok(())
    }

    async fn handle_creating_access_keys(
        &self,
        status: &mut AwsS3Status,
        project_name: &str,
    ) -> Result<()> {
        // Get username
        let username = if let Some(ref iam_user) = status.iam_user {
            iam_user.username.clone()
        } else {
            format!(
                "{}-{}",
                S3_IAM_USERNAME_PREFIX,
                project_name.replace(['/', '-'], "_").to_lowercase()
            )
        };

        // Check if we already have access keys
        if status.iam_user.is_some() {
            info!("Access keys already exist for user '{}'", username);
            status.state = S3State::CreatingBuckets;
            return Ok(());
        }

        info!("Creating access keys for IAM user '{}'", username);

        // Create access key
        match self
            .iam_client
            .create_access_key()
            .user_name(&username)
            .send()
            .await
        {
            Ok(resp) => {
                if let Some(access_key) = resp.access_key() {
                    let access_key_id = access_key.access_key_id();
                    let secret_access_key = access_key.secret_access_key();

                    info!("Created access key for user '{}'", username);

                    // Encrypt credentials
                    let encrypted_access_key_id = self
                        .encryption_provider
                        .encrypt(access_key_id)
                        .await
                        .context("Failed to encrypt access key ID")?;

                    let encrypted_secret_key = self
                        .encryption_provider
                        .encrypt(secret_access_key)
                        .await
                        .context("Failed to encrypt secret access key")?;

                    // Store in status
                    status.iam_user = Some(IamUserStatus {
                        username: username.clone(),
                        access_key_id_encrypted: encrypted_access_key_id,
                        secret_access_key_encrypted: encrypted_secret_key,
                    });

                    status.state = S3State::CreatingBuckets;
                } else {
                    error!("Failed to get access key from response");
                    status.state = S3State::Failed;
                    status.error = Some("No access key in response".to_string());
                }
            }
            Err(e) => {
                error!(
                    "Failed to create access key for user '{}': {:?}",
                    username, e
                );
                status.state = S3State::Failed;
                status.error = Some(format!("Failed to create access key: {:?}", e));
            }
        }

        Ok(())
    }

    async fn handle_creating_buckets(
        &self,
        spec: &AwsS3Spec,
        status: &mut AwsS3Status,
        project_name: &str,
    ) -> Result<()> {
        // For shared mode, create one bucket
        // For isolated mode, we'll create buckets on-demand in before_deployment

        match spec.bucket_strategy {
            BucketStrategy::Shared => {
                // Create the shared bucket
                let bucket_name = self.bucket_name_for_deployment_group(
                    project_name,
                    "default",
                    &BucketStrategy::Shared,
                );

                if !status.buckets.contains_key(&bucket_name) {
                    info!("Creating shared S3 bucket '{}'", bucket_name);

                    match self.create_bucket(&bucket_name, &self.default_region).await {
                        Ok(_) => {
                            info!("Created bucket '{}'", bucket_name);
                            status.buckets.insert(
                                bucket_name.clone(),
                                BucketStatus {
                                    region: self.default_region.clone(),
                                    status: BucketState::Configuring,
                                    cleanup_scheduled_at: None,
                                },
                            );
                            status.state = S3State::ConfiguringBuckets;
                        }
                        Err(e) => {
                            error!("Failed to create bucket '{}': {:?}", bucket_name, e);
                            status.state = S3State::Failed;
                            status.error = Some(format!("Failed to create bucket: {:?}", e));
                        }
                    }
                } else {
                    // Bucket already exists
                    status.state = S3State::ConfiguringBuckets;
                }
            }
            BucketStrategy::Isolated => {
                // No buckets to create upfront - they'll be created on-demand
                info!("Using isolated bucket strategy - buckets will be created on-demand");
                status.state = S3State::Available;
            }
        }

        Ok(())
    }

    async fn handle_configuring_buckets(
        &self,
        spec: &AwsS3Spec,
        status: &mut AwsS3Status,
    ) -> Result<()> {
        // Configure all buckets that are in Configuring state
        let mut all_configured = true;

        for (bucket_name, bucket_status) in status.buckets.iter_mut() {
            if bucket_status.status == BucketState::Configuring {
                info!("Configuring bucket '{}'", bucket_name);

                match self.configure_bucket(bucket_name, spec).await {
                    Ok(_) => {
                        info!("Configured bucket '{}'", bucket_name);
                        bucket_status.status = BucketState::Available;
                    }
                    Err(e) => {
                        error!("Failed to configure bucket '{}': {:?}", bucket_name, e);
                        all_configured = false;
                        // Don't fail completely, just skip this bucket and retry later
                    }
                }
            }
        }

        if all_configured {
            status.state = S3State::Available;
            status.error = None;
        }

        Ok(())
    }

    async fn handle_available(
        &self,
        spec: &AwsS3Spec,
        status: &mut AwsS3Status,
        project_name: &str,
        project_id: Uuid,
    ) -> Result<()> {
        // In isolated mode, cleanup orphaned buckets
        if spec.bucket_strategy == BucketStrategy::Isolated {
            self.cleanup_orphaned_buckets(status, project_id, project_name)
                .await?;
        }

        Ok(())
    }

    async fn handle_deletion(
        &self,
        status: &mut AwsS3Status,
        project_name: &str,
        _extension_name: &str,
    ) -> Result<()> {
        info!(
            "Deleting S3 extension resources for project '{}'",
            project_name
        );

        // Delete all buckets
        let mut all_deleted = true;

        for (bucket_name, bucket_status) in status.buckets.clone().iter() {
            if bucket_status.status != BucketState::Deleting {
                info!("Deleting bucket '{}'", bucket_name);

                match self.deletion_policy {
                    S3DeletionPolicy::Retain => {
                        // Check if bucket is empty
                        match self.is_bucket_empty(bucket_name).await {
                            Ok(true) => {
                                // Empty bucket, can delete
                                match self.delete_bucket(bucket_name).await {
                                    Ok(_) => {
                                        info!("Deleted bucket '{}'", bucket_name);
                                        status.buckets.remove(bucket_name);
                                    }
                                    Err(e) => {
                                        error!(
                                            "Failed to delete bucket '{}': {:?}",
                                            bucket_name, e
                                        );
                                        all_deleted = false;
                                    }
                                }
                            }
                            Ok(false) => {
                                error!("Cannot delete bucket '{}' - not empty (deletion_policy: retain)", bucket_name);
                                status.state = S3State::Failed;
                                status.error = Some(format!(
                                    "Bucket '{}' is not empty. Please empty it manually before deletion.",
                                    bucket_name
                                ));
                                return Ok(());
                            }
                            Err(e) => {
                                error!(
                                    "Failed to check if bucket '{}' is empty: {:?}",
                                    bucket_name, e
                                );
                                all_deleted = false;
                            }
                        }
                    }
                    S3DeletionPolicy::ForceEmpty => {
                        // Empty bucket first, then delete
                        match self.empty_bucket(bucket_name).await {
                            Ok(_) => {
                                info!("Emptied bucket '{}'", bucket_name);
                                match self.delete_bucket(bucket_name).await {
                                    Ok(_) => {
                                        info!("Deleted bucket '{}'", bucket_name);
                                        status.buckets.remove(bucket_name);
                                    }
                                    Err(e) => {
                                        error!(
                                            "Failed to delete bucket '{}': {:?}",
                                            bucket_name, e
                                        );
                                        all_deleted = false;
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to empty bucket '{}': {:?}", bucket_name, e);
                                all_deleted = false;
                            }
                        }
                    }
                    S3DeletionPolicy::Delete => {
                        // Try to delete directly
                        match self.delete_bucket(bucket_name).await {
                            Ok(_) => {
                                info!("Deleted bucket '{}'", bucket_name);
                                status.buckets.remove(bucket_name);
                            }
                            Err(e) => {
                                let error_str = format!("{:?}", e);
                                if error_str.contains("BucketNotEmpty") {
                                    error!("Cannot delete bucket '{}' - not empty", bucket_name);
                                    status.state = S3State::Failed;
                                    status.error = Some(format!(
                                        "Bucket '{}' is not empty. Change deletion_policy to 'force-empty' to allow automatic emptying.",
                                        bucket_name
                                    ));
                                    return Ok(());
                                } else {
                                    error!("Failed to delete bucket '{}': {:?}", bucket_name, e);
                                    all_deleted = false;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Delete IAM resources if we created them
        if matches!(
            &self.access_mode,
            S3AccessMode::IamUser | S3AccessMode::Both { .. }
        ) {
            if let Some(ref iam_user) = status.iam_user {
                let username = &iam_user.username;

                // List and delete all access keys for the user
                match self
                    .iam_client
                    .list_access_keys()
                    .user_name(username)
                    .send()
                    .await
                {
                    Ok(resp) => {
                        for key_metadata in resp.access_key_metadata() {
                            if let Some(key_id) = key_metadata.access_key_id() {
                                match self
                                    .iam_client
                                    .delete_access_key()
                                    .user_name(username)
                                    .access_key_id(key_id)
                                    .send()
                                    .await
                                {
                                    Ok(_) => info!(
                                        "Deleted access key '{}' for user '{}'",
                                        key_id, username
                                    ),
                                    Err(e) => {
                                        error!("Failed to delete access key '{}': {:?}", key_id, e);
                                        all_deleted = false;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to list access keys for user '{}': {:?}",
                            username, e
                        );
                    }
                }

                // Delete inline policies
                match self
                    .iam_client
                    .delete_user_policy()
                    .user_name(username)
                    .policy_name("S3Access")
                    .send()
                    .await
                {
                    Ok(_) => info!("Deleted IAM policy for user '{}'", username),
                    Err(e) => {
                        warn!(
                            "Failed to delete IAM policy for user '{}': {:?}",
                            username, e
                        );
                    }
                }

                // Delete user
                match self
                    .iam_client
                    .delete_user()
                    .user_name(username)
                    .send()
                    .await
                {
                    Ok(_) => {
                        info!("Deleted IAM user '{}'", username);
                        status.iam_user = None;
                    }
                    Err(e) => {
                        error!("Failed to delete IAM user '{}': {:?}", username, e);
                        all_deleted = false;
                    }
                }
            }
        }

        if all_deleted {
            status.state = S3State::Deleted;
            info!("All S3 resources deleted successfully");
        }

        Ok(())
    }

    // Helper functions for S3 operations
    async fn create_bucket(&self, bucket_name: &str, region: &str) -> Result<()> {
        use aws_sdk_s3::types::{BucketLocationConstraint, CreateBucketConfiguration};

        let mut request = self.s3_client.create_bucket().bucket(bucket_name);

        // For regions other than us-east-1, need to specify location constraint
        if region != "us-east-1" {
            let constraint = BucketLocationConstraint::from(region);
            let config = CreateBucketConfiguration::builder()
                .location_constraint(constraint)
                .build();
            request = request.create_bucket_configuration(config);
        }

        match request.send().await {
            Ok(_) => Ok(()),
            Err(e) => {
                let error_str = format!("{:?}", e);
                if error_str.contains("BucketAlreadyOwnedByYou")
                    || error_str.contains("BucketAlreadyExists")
                {
                    // Bucket already exists, that's fine
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Failed to create bucket: {:?}", e))
                }
            }
        }
    }

    async fn configure_bucket(&self, bucket_name: &str, spec: &AwsS3Spec) -> Result<()> {
        // Apply encryption
        self.apply_bucket_encryption(bucket_name).await?;

        // Apply versioning
        if spec.versioning {
            self.enable_bucket_versioning(bucket_name).await?;
        }

        // Apply public access block
        self.apply_public_access_block(bucket_name, &spec.public_access_block)
            .await?;

        // Apply lifecycle rules
        if !spec.lifecycle_rules.is_empty() {
            self.apply_lifecycle_rules(bucket_name, &spec.lifecycle_rules)
                .await?;
        }

        // Apply CORS if specified
        if let Some(ref cors_rules) = spec.cors {
            self.apply_cors_configuration(bucket_name, cors_rules)
                .await?;
        }

        Ok(())
    }

    async fn apply_bucket_encryption(&self, bucket_name: &str) -> Result<()> {
        use aws_sdk_s3::types::{
            ServerSideEncryption, ServerSideEncryptionByDefault, ServerSideEncryptionConfiguration,
            ServerSideEncryptionRule,
        };

        let (sse_algorithm, kms_key_id) = match &self.encryption {
            S3EncryptionConfig::Aes256 => (ServerSideEncryption::Aes256, None),
            S3EncryptionConfig::AwsKms => (ServerSideEncryption::AwsKms, None),
            S3EncryptionConfig::AwsKmsCustomKey { key_id } => {
                (ServerSideEncryption::AwsKms, Some(key_id.clone()))
            }
        };

        let default_encryption = if let Some(key_id) = kms_key_id {
            ServerSideEncryptionByDefault::builder()
                .sse_algorithm(ServerSideEncryption::AwsKms)
                .kms_master_key_id(key_id)
                .build()?
        } else {
            ServerSideEncryptionByDefault::builder()
                .sse_algorithm(sse_algorithm)
                .build()?
        };

        let rule = ServerSideEncryptionRule::builder()
            .apply_server_side_encryption_by_default(default_encryption)
            .build();

        let config = ServerSideEncryptionConfiguration::builder()
            .rules(rule)
            .build()?;

        self.s3_client
            .put_bucket_encryption()
            .bucket(bucket_name)
            .server_side_encryption_configuration(config)
            .send()
            .await?;

        Ok(())
    }

    async fn enable_bucket_versioning(&self, bucket_name: &str) -> Result<()> {
        use aws_sdk_s3::types::{BucketVersioningStatus, VersioningConfiguration};

        let config = VersioningConfiguration::builder()
            .status(BucketVersioningStatus::Enabled)
            .build();

        self.s3_client
            .put_bucket_versioning()
            .bucket(bucket_name)
            .versioning_configuration(config)
            .send()
            .await?;

        Ok(())
    }

    async fn apply_public_access_block(
        &self,
        bucket_name: &str,
        config: &PublicAccessBlockConfig,
    ) -> Result<()> {
        use aws_sdk_s3::types::PublicAccessBlockConfiguration;

        let pab_config = PublicAccessBlockConfiguration::builder()
            .block_public_acls(config.block_public_acls)
            .ignore_public_acls(config.ignore_public_acls)
            .block_public_policy(config.block_public_policy)
            .restrict_public_buckets(config.restrict_public_buckets)
            .build();

        self.s3_client
            .put_public_access_block()
            .bucket(bucket_name)
            .public_access_block_configuration(pab_config)
            .send()
            .await?;

        Ok(())
    }

    async fn apply_lifecycle_rules(
        &self,
        bucket_name: &str,
        rules: &[LifecycleRule],
    ) -> Result<()> {
        use aws_sdk_s3::types::{
            BucketLifecycleConfiguration, ExpirationStatus, LifecycleExpiration,
            LifecycleRule as S3LifecycleRule, LifecycleRuleFilter,
        };

        let mut s3_rules = Vec::new();

        for rule in rules {
            let expiration = LifecycleExpiration::builder()
                .days(rule.expiration_days)
                .build();

            let mut s3_rule_builder = S3LifecycleRule::builder()
                .id(&rule.id)
                .status(if rule.enabled {
                    ExpirationStatus::Enabled
                } else {
                    ExpirationStatus::Disabled
                })
                .expiration(expiration);

            // Set filter with prefix (use empty prefix if not specified)
            let filter = LifecycleRuleFilter::builder().prefix(&rule.prefix).build();
            s3_rule_builder = s3_rule_builder.filter(filter);

            s3_rules.push(s3_rule_builder.build()?);
        }

        let config = BucketLifecycleConfiguration::builder()
            .set_rules(Some(s3_rules))
            .build()?;

        self.s3_client
            .put_bucket_lifecycle_configuration()
            .bucket(bucket_name)
            .lifecycle_configuration(config)
            .send()
            .await?;

        Ok(())
    }

    async fn apply_cors_configuration(
        &self,
        bucket_name: &str,
        cors_rules: &[CorsRule],
    ) -> Result<()> {
        use aws_sdk_s3::types::{CorsConfiguration, CorsRule as S3CorsRule};

        let mut s3_cors_rules = Vec::new();

        for rule in cors_rules {
            let mut s3_rule = S3CorsRule::builder()
                .set_allowed_origins(Some(rule.allowed_origins.clone()))
                .set_allowed_methods(Some(rule.allowed_methods.clone()));

            if !rule.allowed_headers.is_empty() {
                s3_rule = s3_rule.set_allowed_headers(Some(rule.allowed_headers.clone()));
            }

            if let Some(max_age) = rule.max_age_seconds {
                s3_rule = s3_rule.max_age_seconds(max_age);
            }

            s3_cors_rules.push(s3_rule.build()?);
        }

        let config = CorsConfiguration::builder()
            .set_cors_rules(Some(s3_cors_rules))
            .build()?;

        self.s3_client
            .put_bucket_cors()
            .bucket(bucket_name)
            .cors_configuration(config)
            .send()
            .await?;

        Ok(())
    }

    async fn is_bucket_empty(&self, bucket_name: &str) -> Result<bool> {
        let resp = self
            .s3_client
            .list_objects_v2()
            .bucket(bucket_name)
            .max_keys(1)
            .send()
            .await?;

        Ok(resp.key_count().unwrap_or(0) == 0)
    }

    async fn empty_bucket(&self, bucket_name: &str) -> Result<()> {
        // List and delete all objects (including versions if versioning is enabled)
        loop {
            let resp = self
                .s3_client
                .list_objects_v2()
                .bucket(bucket_name)
                .max_keys(1000)
                .send()
                .await?;

            let objects = resp.contents();
            if objects.is_empty() {
                break;
            }

            for obj in objects {
                if let Some(key) = obj.key() {
                    self.s3_client
                        .delete_object()
                        .bucket(bucket_name)
                        .key(key)
                        .send()
                        .await?;
                }
            }
        }

        // Also delete object versions if versioning is enabled
        loop {
            let resp = self
                .s3_client
                .list_object_versions()
                .bucket(bucket_name)
                .max_keys(1000)
                .send()
                .await?;

            let versions = resp.versions();
            let delete_markers = resp.delete_markers();

            if versions.is_empty() && delete_markers.is_empty() {
                break;
            }

            // Delete versions
            for version in versions {
                if let (Some(key), Some(version_id)) = (version.key(), version.version_id()) {
                    self.s3_client
                        .delete_object()
                        .bucket(bucket_name)
                        .key(key)
                        .version_id(version_id)
                        .send()
                        .await?;
                }
            }

            // Delete delete markers
            for marker in delete_markers {
                if let (Some(key), Some(version_id)) = (marker.key(), marker.version_id()) {
                    self.s3_client
                        .delete_object()
                        .bucket(bucket_name)
                        .key(key)
                        .version_id(version_id)
                        .send()
                        .await?;
                }
            }
        }

        Ok(())
    }

    async fn delete_bucket(&self, bucket_name: &str) -> Result<()> {
        self.s3_client
            .delete_bucket()
            .bucket(bucket_name)
            .send()
            .await?;

        Ok(())
    }

    async fn cleanup_orphaned_buckets(
        &self,
        status: &mut AwsS3Status,
        project_id: Uuid,
        project_name: &str,
    ) -> Result<()> {
        use std::collections::HashSet;

        // Get list of active deployment groups for this project
        let active_groups = db_deployments::get_active_deployment_groups(&self.db_pool, project_id)
            .await
            .context("Failed to get active deployment groups")?;

        let now = Utc::now();
        let grace_period = Duration::hours(1);

        // Build set of expected bucket names from active deployment groups
        let expected_buckets: HashSet<String> = active_groups
            .iter()
            .map(|group| {
                self.bucket_name_for_deployment_group(
                    project_name,
                    group,
                    &BucketStrategy::Isolated,
                )
            })
            .collect();

        let mut buckets_to_remove = Vec::new();

        for (bucket_name, bucket_status) in status.buckets.iter_mut() {
            // Check if this bucket corresponds to an active deployment group
            let is_active = expected_buckets.contains(bucket_name);

            if !is_active {
                // Bucket doesn't match any active deployment group
                if let Some(scheduled_at) = bucket_status.cleanup_scheduled_at {
                    // Already scheduled, check if grace period expired
                    if now >= scheduled_at + grace_period {
                        info!(
                            "Grace period expired for bucket '{}', cleaning up",
                            bucket_name
                        );
                        buckets_to_remove.push(bucket_name.clone());
                    } else {
                        debug!(
                            "Bucket '{}' scheduled for cleanup at {} (grace period not expired)",
                            bucket_name,
                            scheduled_at + grace_period
                        );
                    }
                } else {
                    // First time inactive, schedule for cleanup
                    bucket_status.cleanup_scheduled_at = Some(now);
                    info!(
                        "Bucket '{}' has no active deployment group, scheduling for cleanup in 1 hour",
                        bucket_name
                    );
                }
            } else {
                // Bucket has active deployment group, cancel cleanup if scheduled
                if bucket_status.cleanup_scheduled_at.is_some() {
                    info!(
                        "Bucket '{}' has active deployments again, cancelling cleanup",
                        bucket_name
                    );
                    bucket_status.cleanup_scheduled_at = None;
                }
            }
        }

        // Execute cleanup for buckets past grace period
        for bucket_name in buckets_to_remove {
            info!("Cleaning up orphaned bucket '{}'", bucket_name);

            // Follow deletion policy
            match self.deletion_policy {
                S3DeletionPolicy::ForceEmpty => match self.empty_bucket(&bucket_name).await {
                    Ok(_) => match self.delete_bucket(&bucket_name).await {
                        Ok(_) => {
                            info!("Deleted orphaned bucket '{}'", bucket_name);
                            status.buckets.remove(&bucket_name);
                        }
                        Err(e) => {
                            error!("Failed to delete bucket '{}': {:?}", bucket_name, e);
                        }
                    },
                    Err(e) => {
                        error!("Failed to empty bucket '{}': {:?}", bucket_name, e);
                    }
                },
                _ => {
                    // For Retain and Delete policies, just remove from tracking
                    warn!(
                        "Bucket '{}' is orphaned but deletion_policy is {:?}, removing from tracking only",
                        bucket_name, self.deletion_policy
                    );
                    status.buckets.remove(&bucket_name);
                }
            }
        }

        Ok(())
    }

    /// Generate IAM policy document for S3 access
    fn generate_iam_policy_document(&self, project_name: &str) -> String {
        let safe_project = project_name.replace(['/', '-'], "_").to_lowercase();
        let bucket_pattern = format!("{}-{}*", self.bucket_prefix, safe_project);

        serde_json::json!({
            "Version": "2012-10-17",
            "Statement": [
                {
                    "Effect": "Allow",
                    "Action": [
                        "s3:ListBucket",
                        "s3:GetBucketLocation",
                        "s3:GetBucketVersioning",
                        "s3:ListBucketVersions"
                    ],
                    "Resource": [format!("arn:aws:s3:::{}", bucket_pattern)]
                },
                {
                    "Effect": "Allow",
                    "Action": [
                        "s3:GetObject",
                        "s3:GetObjectVersion",
                        "s3:PutObject",
                        "s3:DeleteObject",
                        "s3:DeleteObjectVersion"
                    ],
                    "Resource": [format!("arn:aws:s3:::{}/*", bucket_pattern)]
                }
            ]
        })
        .to_string()
    }
}

#[async_trait]
impl Extension for AwsS3Provisioner {
    fn extension_type(&self) -> &str {
        "aws-s3-provisioner"
    }

    fn display_name(&self) -> &str {
        "AWS S3 Bucket"
    }

    async fn validate_spec(&self, spec: &Value) -> Result<()> {
        let _parsed: AwsS3Spec =
            serde_json::from_value(spec.clone()).context("Failed to parse AWS S3 spec")?;

        // Validate bucket strategy
        // (no specific validation needed for now)

        Ok(())
    }

    fn start(&self) {
        let provisioner = self.clone();

        tokio::spawn(async move {
            info!(
                "Starting AWS S3 extension reconciliation loop for type '{}'",
                provisioner.extension_type()
            );

            // Track error counts and last error times for exponential backoff
            let mut error_state: HashMap<Uuid, (usize, DateTime<Utc>)> = HashMap::new();

            loop {
                // List ALL project extensions of this type (across all projects)
                match db_extensions::list_by_extension_type(
                    &provisioner.db_pool,
                    provisioner.extension_type(),
                )
                .await
                {
                    Ok(extensions) => {
                        if extensions.is_empty() {
                            debug!("No S3 extensions found, waiting for work");
                        }

                        for ext in extensions {
                            // Check if we should skip this extension due to backoff
                            if let Some((error_count, last_error)) =
                                error_state.get(&ext.project_id)
                            {
                                // Exponential backoff: 2^error_count seconds (capped at 5 minutes)
                                let backoff_seconds = 2_i64.pow(*error_count as u32).min(300);
                                let backoff_until =
                                    *last_error + Duration::seconds(backoff_seconds);

                                if Utc::now() < backoff_until {
                                    debug!(
                                        "Skipping extension for project {} due to backoff ({}s remaining)",
                                        ext.project_id,
                                        (backoff_until - Utc::now()).num_seconds()
                                    );
                                    continue;
                                }
                            }

                            match provisioner.reconcile_single(ext.clone()).await {
                                Ok(_) => {
                                    // Success - reset error state
                                    error_state.remove(&ext.project_id);
                                }
                                Err(e) => {
                                    error!("Failed to reconcile AWS S3 extension: {:?}", e);
                                    // Increment error count and update last error time
                                    let entry = error_state
                                        .entry(ext.project_id)
                                        .or_insert((0, Utc::now()));
                                    entry.0 += 1;
                                    entry.1 = Utc::now();
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to list S3 extensions: {:?}", e);
                    }
                }

                // Check if any extension is in a transitional state
                let needs_active_polling = match db_extensions::list_by_extension_type(
                    &provisioner.db_pool,
                    provisioner.extension_type(),
                )
                .await
                {
                    Ok(extensions) => extensions.iter().any(|ext| {
                        if let Ok(status) =
                            serde_json::from_value::<AwsS3Status>(ext.status.clone())
                        {
                            matches!(
                                status.state,
                                S3State::Pending
                                    | S3State::CreatingIamUser
                                    | S3State::CreatingAccessKeys
                                    | S3State::CreatingBuckets
                                    | S3State::ConfiguringBuckets
                                    | S3State::Deleting
                                    | S3State::Failed
                            ) || ext.deleted_at.is_some()
                        } else {
                            false
                        }
                    }),
                    Err(_) => false,
                };

                let wait_time = if needs_active_polling { 2 } else { 5 };
                sleep(std::time::Duration::from_secs(wait_time)).await;
            }
        });
    }

    async fn before_deployment(
        &self,
        deployment_id: Uuid,
        project_id: Uuid,
        deployment_group: &str,
    ) -> Result<()> {
        // Find all extensions of this type for this project
        let extensions =
            db_extensions::list_by_extension_type(&self.db_pool, self.extension_type())
                .await?
                .into_iter()
                .filter(|e| e.project_id == project_id && e.deleted_at.is_none())
                .collect::<Vec<_>>();

        if extensions.is_empty() {
            // Extension not enabled for this project - skip hook
            debug!(
                "Extension type '{}' not enabled for project {}, skipping before_deployment hook",
                self.extension_type(),
                project_id
            );
            return Ok(());
        }

        // For S3, we expect at most one instance per project
        // If there are multiple, use the first one and log a warning
        let ext = &extensions[0];
        if extensions.len() > 1 {
            warn!(
                "Multiple S3 extensions found for project {}, using first instance: {}",
                project_id, ext.extension
            );
        }

        // Parse spec and status
        let spec: AwsS3Spec =
            serde_json::from_value(ext.spec.clone()).context("Failed to parse AWS S3 spec")?;
        let status: AwsS3Status =
            serde_json::from_value(ext.status.clone()).context("Failed to parse AWS S3 status")?;

        // Check if S3 extension is available
        if status.state != S3State::Available {
            anyhow::bail!(
                "S3 extension '{}' is not available (current state: {:?})",
                ext.extension,
                status.state
            );
        }

        // Get project info for bucket naming
        let project = db_projects::find_by_id(&self.db_pool, project_id)
            .await
            .context("Failed to find project")?
            .ok_or_else(|| anyhow::anyhow!("Project not found"))?;

        // Get bucket name for this deployment group
        let bucket_name = self.bucket_name_for_deployment_group(
            &project.name,
            deployment_group,
            &spec.bucket_strategy,
        );

        // Ensure bucket exists and is available. In isolated mode, buckets may need to be
        // created on-demand the first time a deployment group is used.
        let mut status = status; // Make mutable for potential updates
        if let Some(bucket) = status.buckets.get(&bucket_name) {
            if bucket.status != BucketState::Available {
                anyhow::bail!(
                    "Bucket '{}' is not available (current state: {:?})",
                    bucket_name,
                    bucket.status
                );
            }
        } else {
            // Bucket is missing from status; create it on-demand for isolated mode
            if spec.bucket_strategy == BucketStrategy::Isolated {
                info!(
                    "Creating isolated S3 bucket '{}' on-demand for deployment group '{}'",
                    bucket_name, deployment_group
                );

                // Create the bucket
                self.create_bucket(&bucket_name, &self.default_region)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to create isolated S3 bucket '{}' in before_deployment",
                            bucket_name
                        )
                    })?;

                // Configure the bucket (encryption, versioning, etc.)
                self.configure_bucket(&bucket_name, &spec)
                    .await
                    .context("Failed to configure bucket")?;

                // Wait for the bucket to become available by polling HeadBucket with a timeout
                let deadline = chrono::Utc::now() + chrono::Duration::minutes(5);
                loop {
                    if chrono::Utc::now() > deadline {
                        anyhow::bail!(
                            "Timed out waiting for isolated S3 bucket '{}' to become available",
                            bucket_name
                        );
                    }

                    match self
                        .s3_client
                        .head_bucket()
                        .bucket(&bucket_name)
                        .send()
                        .await
                    {
                        Ok(_) => {
                            info!("Isolated S3 bucket '{}' is now available", bucket_name);
                            break;
                        }
                        Err(err) => {
                            // If the bucket is not yet fully propagated, keep retrying; otherwise, fail.
                            let msg = format!("{:?}", err);
                            if msg.contains("NotFound") || msg.contains("NoSuchBucket") {
                                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                continue;
                            }
                            return Err(anyhow::anyhow!(err).context(format!(
                                "Failed while waiting for isolated S3 bucket '{}' to become available",
                                bucket_name
                            )));
                        }
                    }
                }

                // Update status with the new bucket
                status.buckets.insert(
                    bucket_name.clone(),
                    BucketStatus {
                        status: BucketState::Available,
                        region: self.default_region.clone(),
                        cleanup_scheduled_at: None,
                    },
                );

                // Persist updated status
                db_extensions::update_status(
                    &self.db_pool,
                    project_id,
                    &ext.extension,
                    &serde_json::to_value(&status)?,
                )
                .await?;
            } else {
                // In shared mode, bucket should already exist from reconciliation
                anyhow::bail!("Bucket '{}' not found in status", bucket_name);
            }
        }

        // Get bucket reference from status (guaranteed to exist at this point)
        let bucket = status
            .buckets
            .get(&bucket_name)
            .expect("Bucket should exist in status after creation check");

        // Inject environment variables
        let mut injected_vars = Vec::new();

        // Inject bucket name
        if let Some(ref var_name) = spec.env_vars.bucket_name {
            db_env_vars::upsert_deployment_env_var(
                &self.db_pool,
                deployment_id,
                var_name,
                &bucket_name,
                false, // not a secret
            )
            .await?;
            injected_vars.push(var_name.as_str());
        }

        // Inject region
        if let Some(ref var_name) = spec.env_vars.region {
            db_env_vars::upsert_deployment_env_var(
                &self.db_pool,
                deployment_id,
                var_name,
                &bucket.region,
                false,
            )
            .await?;
            injected_vars.push(var_name.as_str());
        }

        // Inject credentials based on access_mode
        match &self.access_mode {
            S3AccessMode::IamUser => {
                // Inject IAM user credentials only
                let iam_user = status
                    .iam_user
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("IAM user not found in status"))?;

                // Decrypt credentials
                let access_key_id = self
                    .encryption_provider
                    .decrypt(&iam_user.access_key_id_encrypted)
                    .await
                    .context("Failed to decrypt access key ID")?;
                let secret_key = self
                    .encryption_provider
                    .decrypt(&iam_user.secret_access_key_encrypted)
                    .await
                    .context("Failed to decrypt secret access key")?;

                // Inject as secrets
                if let Some(ref var_name) = spec.env_vars.access_key_id {
                    db_env_vars::upsert_deployment_env_var(
                        &self.db_pool,
                        deployment_id,
                        var_name,
                        &access_key_id,
                        true, // secret
                    )
                    .await?;
                    injected_vars.push(var_name.as_str());
                }

                if let Some(ref var_name) = spec.env_vars.secret_access_key {
                    db_env_vars::upsert_deployment_env_var(
                        &self.db_pool,
                        deployment_id,
                        var_name,
                        &secret_key,
                        true, // secret
                    )
                    .await?;
                    injected_vars.push(var_name.as_str());
                }
            }
            S3AccessMode::IamRole { role_arn } => {
                // Inject role ARN only
                if let Some(ref var_name) = spec.env_vars.role_arn {
                    db_env_vars::upsert_deployment_env_var(
                        &self.db_pool,
                        deployment_id,
                        var_name,
                        role_arn,
                        false,
                    )
                    .await?;
                    injected_vars.push(var_name.as_str());
                }
            }
            S3AccessMode::Both { role_arn } => {
                // Inject both IAM user credentials and role ARN
                let iam_user = status
                    .iam_user
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("IAM user not found in status"))?;

                // Decrypt credentials
                let access_key_id = self
                    .encryption_provider
                    .decrypt(&iam_user.access_key_id_encrypted)
                    .await
                    .context("Failed to decrypt access key ID")?;
                let secret_key = self
                    .encryption_provider
                    .decrypt(&iam_user.secret_access_key_encrypted)
                    .await
                    .context("Failed to decrypt secret access key")?;

                // Inject IAM user credentials
                if let Some(ref var_name) = spec.env_vars.access_key_id {
                    db_env_vars::upsert_deployment_env_var(
                        &self.db_pool,
                        deployment_id,
                        var_name,
                        &access_key_id,
                        true, // secret
                    )
                    .await?;
                    injected_vars.push(var_name.as_str());
                }

                if let Some(ref var_name) = spec.env_vars.secret_access_key {
                    db_env_vars::upsert_deployment_env_var(
                        &self.db_pool,
                        deployment_id,
                        var_name,
                        &secret_key,
                        true, // secret
                    )
                    .await?;
                    injected_vars.push(var_name.as_str());
                }

                // Inject role ARN
                if let Some(ref var_name) = spec.env_vars.role_arn {
                    db_env_vars::upsert_deployment_env_var(
                        &self.db_pool,
                        deployment_id,
                        var_name,
                        role_arn,
                        false,
                    )
                    .await?;
                    injected_vars.push(var_name.as_str());
                }
            }
        }

        info!(
            "Injected S3 env vars for deployment {} (group: {}, bucket: {}): {:?}",
            deployment_id, deployment_group, bucket_name, injected_vars
        );

        Ok(())
    }

    fn format_status(&self, status: &Value) -> String {
        // Try to parse as AwsS3Status
        let parsed: AwsS3Status = match serde_json::from_value(status.clone()) {
            Ok(s) => s,
            Err(_) => return "Unknown".to_string(),
        };

        // Format based on state
        match parsed.state {
            S3State::Pending => "Pending".to_string(),
            S3State::CreatingIamUser => "Creating IAM user...".to_string(),
            S3State::CreatingAccessKeys => "Creating access keys...".to_string(),
            S3State::CreatingBuckets => "Creating buckets...".to_string(),
            S3State::ConfiguringBuckets => "Configuring buckets...".to_string(),
            S3State::Available => {
                let bucket_count = parsed.buckets.len();
                format!(
                    "Available ({} bucket{})",
                    bucket_count,
                    if bucket_count == 1 { "" } else { "s" }
                )
            }
            S3State::Deleting => "Deleting...".to_string(),
            S3State::Deleted => "Deleted".to_string(),
            S3State::Failed => {
                if let Some(error) = parsed.error {
                    format!("Failed: {}", error)
                } else {
                    "Failed".to_string()
                }
            }
        }
    }

    fn description(&self) -> &str {
        "Provides S3 buckets for object storage on AWS"
    }

    fn documentation(&self) -> &str {
        r#"# AWS S3 Bucket Extension

This extension provisions S3 buckets for your project with configurable settings.

## Features

- Automatic S3 bucket provisioning
- Configurable bucket strategy (shared or isolated per deployment group)
- Optional versioning and lifecycle rules
- IAM user or IAM role (IRSA) access modes
- Automatic credential injection as environment variables
- Encrypted credential storage
- CORS configuration support
- Public access blocking

## Configuration

The extension accepts the following spec fields:

- `bucket_strategy` (optional, default: "shared"): Controls bucket provisioning:
  - `"shared"`: One bucket per project (all deployment groups share)
  - `"isolated"`: One bucket per deployment group (data isolation)
- `versioning` (optional, default: false): Enable S3 versioning
- `env_vars` (optional): Environment variable names to inject
- `lifecycle_rules` (optional): Array of expiration rules
- `public_access_block` (optional): Public access settings (default: all blocked)
- `cors` (optional): CORS configuration

## Example Spec

Minimal configuration (uses all defaults):
```json
{}
```

With versioning and isolated buckets:
```json
{
  "bucket_strategy": "isolated",
  "versioning": true
}
```

With lifecycle rules:
```json
{
  "lifecycle_rules": [
    {
      "id": "cleanup-temp",
      "prefix": "tmp/",
      "expiration_days": 7,
      "enabled": true
    }
  ]
}
```

## Environment Variables

Default environment variables injected:
- `AWS_S3_BUCKET`: Bucket name for this deployment
- `AWS_REGION`: AWS region
- `AWS_ACCESS_KEY_ID`: Access key (IAM user mode)
- `AWS_SECRET_ACCESS_KEY`: Secret key (IAM user mode)
- `AWS_ROLE_ARN`: Role ARN (IAM role mode)

Variable names are configurable via the `env_vars` field in the spec.
"#
    }

    fn spec_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "bucket_strategy": {
                    "type": "string",
                    "enum": ["shared", "isolated"],
                    "default": "shared",
                    "description": "Bucket provisioning strategy: 'shared' (one bucket per project) or 'isolated' (one per deployment group)"
                },
                "versioning": {
                    "type": "boolean",
                    "default": false,
                    "description": "Enable S3 versioning for the bucket"
                },
                "env_vars": {
                    "type": "object",
                    "properties": {
                        "bucket_name": { "type": "string", "default": "AWS_S3_BUCKET" },
                        "region": { "type": "string", "default": "AWS_REGION" },
                        "access_key_id": { "type": "string", "default": "AWS_ACCESS_KEY_ID" },
                        "secret_access_key": { "type": "string", "default": "AWS_SECRET_ACCESS_KEY" },
                        "role_arn": { "type": "string", "default": "AWS_ROLE_ARN" }
                    },
                    "description": "Environment variable names to inject for S3 access"
                },
                "lifecycle_rules": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "prefix": { "type": "string", "default": "" },
                            "expiration_days": { "type": "integer" },
                            "enabled": { "type": "boolean", "default": true }
                        },
                        "required": ["id", "expiration_days"]
                    },
                    "default": [],
                    "description": "Lifecycle rules for automatic object expiration"
                },
                "public_access_block": {
                    "type": "object",
                    "properties": {
                        "block_public_acls": { "type": "boolean", "default": true },
                        "ignore_public_acls": { "type": "boolean", "default": true },
                        "block_public_policy": { "type": "boolean", "default": true },
                        "restrict_public_buckets": { "type": "boolean", "default": true }
                    },
                    "default": {
                        "block_public_acls": true,
                        "ignore_public_acls": true,
                        "block_public_policy": true,
                        "restrict_public_buckets": true
                    },
                    "description": "Public access block configuration (default: all blocked)"
                },
                "cors": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "allowed_origins": {
                                "type": "array",
                                "items": { "type": "string" }
                            },
                            "allowed_methods": {
                                "type": "array",
                                "items": { "type": "string" }
                            },
                            "allowed_headers": {
                                "type": "array",
                                "items": { "type": "string" },
                                "default": []
                            },
                            "max_age_seconds": { "type": "integer" }
                        },
                        "required": ["allowed_origins", "allowed_methods"]
                    },
                    "description": "Optional CORS configuration for the bucket"
                }
            }
        })
    }
}
