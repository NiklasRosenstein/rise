use crate::db::{self, extensions as db_extensions, projects};
use crate::server::encryption::EncryptionProvider;
use crate::server::extensions::Extension;
use anyhow::{Context, Result};
use async_trait::async_trait;
use aws_sdk_rds::Client as RdsClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};
use uuid::Uuid;

const EXTENSION_NAME: &str = "aws-rds-postgres";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AwsRdsSpec {
    /// Database engine (currently only "postgres" is supported)
    #[serde(default = "default_engine")]
    pub engine: String,
    /// Engine version (e.g., "16.2")
    #[serde(default)]
    pub engine_version: Option<String>,
}

fn default_engine() -> String {
    "postgres".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AwsRdsStatus {
    /// Current state of the RDS instance
    pub state: RdsState,
    /// RDS instance identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    /// Database endpoint (host:port)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Master username
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_username: Option<String>,
    /// Encrypted master password
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_password_encrypted: Option<String>,
    /// Last error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RdsState {
    Pending,
    Creating,
    Available,
    Deleting,
    Deleted,
    Failed,
}

pub struct AwsRdsProvisioner {
    rds_client: RdsClient,
    db_pool: sqlx::PgPool,
    encryption_provider: Arc<dyn EncryptionProvider>,
    region: String,
    instance_size: String,
    disk_size: i32,
    instance_id_template: String,
}

impl AwsRdsProvisioner {
    pub fn new(
        rds_client: RdsClient,
        db_pool: sqlx::PgPool,
        encryption_provider: Arc<dyn EncryptionProvider>,
        region: String,
        instance_size: String,
        disk_size: i32,
        instance_id_template: String,
    ) -> Self {
        Self {
            rds_client,
            db_pool,
            encryption_provider,
            region,
            instance_size,
            disk_size,
            instance_id_template,
        }
    }

    fn instance_id_for_project(&self, project_name: &str) -> String {
        self.instance_id_template
            .replace("{project_name}", project_name)
    }

    async fn reconcile_single(
        &self,
        project_extension: db::models::ProjectExtension,
    ) -> Result<()> {
        let project = projects::find_by_id(&self.db_pool, project_extension.project_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Project not found"))?;

        // Parse spec
        let spec: AwsRdsSpec = serde_json::from_value(project_extension.spec.clone())
            .context("Failed to parse AWS RDS spec")?;

        // Parse current status or create default
        let mut status: AwsRdsStatus = serde_json::from_value(project_extension.status.clone())
            .unwrap_or(AwsRdsStatus {
                state: RdsState::Pending,
                instance_id: None,
                endpoint: None,
                master_username: None,
                master_password_encrypted: None,
                error: None,
            });

        // Check if marked for deletion
        if project_extension.deleted_at.is_some() {
            // Handle deletion
            if status.state != RdsState::Deleted {
                self.handle_deletion(&mut status, &project.name).await?;
                // Update status
                db_extensions::update_status(
                    &self.db_pool,
                    project_extension.project_id,
                    EXTENSION_NAME,
                    &serde_json::to_value(&status)?,
                )
                .await?;

                // If deletion is complete, hard delete the record
                if status.state == RdsState::Deleted {
                    db_extensions::delete_permanently(
                        &self.db_pool,
                        project_extension.project_id,
                        EXTENSION_NAME,
                    )
                    .await?;
                    info!(
                        "Permanently deleted extension record for project {}",
                        project.name
                    );
                }
            }
            return Ok(());
        }

        // Handle normal lifecycle
        match status.state {
            RdsState::Pending => {
                self.handle_pending(&spec, &mut status, &project.name)
                    .await?;
            }
            RdsState::Creating => {
                self.handle_creating(&mut status, &project.name).await?;
            }
            RdsState::Available => {
                // Check if instance still exists
                self.verify_instance_available(&mut status, &project.name)
                    .await?;
            }
            RdsState::Failed => {
                // Retry creation after a delay
                warn!(
                    "RDS instance for project {} is in failed state, will retry",
                    project.name
                );
                status.state = RdsState::Pending;
                status.error = None;
            }
            _ => {}
        }

        // Update status in database
        db_extensions::update_status(
            &self.db_pool,
            project_extension.project_id,
            EXTENSION_NAME,
            &serde_json::to_value(&status)?,
        )
        .await?;

        Ok(())
    }

    async fn handle_pending(
        &self,
        spec: &AwsRdsSpec,
        status: &mut AwsRdsStatus,
        project_name: &str,
    ) -> Result<()> {
        let instance_id = self.instance_id_for_project(project_name);
        info!(
            "Creating RDS instance {} for project {}",
            instance_id, project_name
        );

        // Generate master credentials
        let master_username = "riseadmin".to_string();
        let master_password = self.generate_password();

        // Encrypt password
        let encrypted_password = self
            .encryption_provider
            .encrypt(&master_password)
            .await
            .context("Failed to encrypt master password")?;

        // Create RDS instance
        let engine_version = spec
            .engine_version
            .clone()
            .unwrap_or_else(|| "16.2".to_string());

        match self
            .rds_client
            .create_db_instance()
            .db_instance_identifier(&instance_id)
            .db_instance_class(&self.instance_size)
            .engine("postgres")
            .engine_version(&engine_version)
            .master_username(&master_username)
            .master_user_password(&master_password)
            .allocated_storage(self.disk_size)
            .publicly_accessible(false)
            .storage_encrypted(true)
            .send()
            .await
        {
            Ok(_) => {
                info!("RDS create request sent for instance {}", instance_id);
                status.state = RdsState::Creating;
                status.instance_id = Some(instance_id);
                status.master_username = Some(master_username);
                status.master_password_encrypted = Some(encrypted_password);
                status.error = None;
            }
            Err(e) => {
                error!("Failed to create RDS instance {}: {}", instance_id, e);
                status.state = RdsState::Failed;
                status.error = Some(format!("Failed to create instance: {}", e));
            }
        }

        Ok(())
    }

    async fn handle_creating(&self, status: &mut AwsRdsStatus, _project_name: &str) -> Result<()> {
        let instance_id = status
            .instance_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Instance ID not set in creating state"))?;

        // Check instance status
        match self
            .rds_client
            .describe_db_instances()
            .db_instance_identifier(instance_id)
            .send()
            .await
        {
            Ok(resp) => {
                let instances = resp.db_instances();
                if let Some(instance) = instances.first() {
                    if let Some(instance_status) = instance.db_instance_status() {
                        match instance_status {
                            "available" => {
                                info!("RDS instance {} is now available", instance_id);
                                status.state = RdsState::Available;

                                // Extract endpoint
                                if let Some(endpoint) = instance.endpoint() {
                                    if let (Some(address), Some(port)) =
                                        (endpoint.address(), endpoint.port())
                                    {
                                        status.endpoint = Some(format!("{}:{}", address, port));
                                    }
                                }
                                status.error = None;
                            }
                            "creating" | "backing-up" | "modifying" => {
                                info!(
                                    "RDS instance {} is still creating (status: {})",
                                    instance_id, instance_status
                                );
                            }
                            "failed" => {
                                error!("RDS instance {} failed to create", instance_id);
                                status.state = RdsState::Failed;
                                status.error = Some("Instance creation failed".to_string());
                            }
                            _ => {
                                warn!(
                                    "RDS instance {} has unexpected status: {}",
                                    instance_id, instance_status
                                );
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to describe RDS instance {}: {}", instance_id, e);
                // Don't fail immediately, will retry on next reconcile
            }
        }

        Ok(())
    }

    async fn verify_instance_available(
        &self,
        status: &mut AwsRdsStatus,
        _project_name: &str,
    ) -> Result<()> {
        let instance_id = status
            .instance_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Instance ID not set in available state"))?;

        // Check if instance still exists
        match self
            .rds_client
            .describe_db_instances()
            .db_instance_identifier(instance_id)
            .send()
            .await
        {
            Ok(resp) => {
                let instances = resp.db_instances();
                if let Some(instance) = instances.first() {
                    if let Some(instance_status) = instance.db_instance_status() {
                        if instance_status != "available" {
                            warn!(
                                "RDS instance {} status changed from available to {}",
                                instance_id, instance_status
                            );
                            status.state = RdsState::Creating; // Will check again on next reconcile
                        }
                    }
                } else {
                    error!("RDS instance {} no longer exists", instance_id);
                    status.state = RdsState::Failed;
                    status.error = Some("Instance no longer exists".to_string());
                }
            }
            Err(e) => {
                warn!("Failed to verify RDS instance {}: {}", instance_id, e);
                // Don't fail immediately
            }
        }

        Ok(())
    }

    async fn handle_deletion(&self, status: &mut AwsRdsStatus, project_name: &str) -> Result<()> {
        let instance_id = match &status.instance_id {
            Some(id) => id.clone(),
            None => {
                // No instance ID, mark as deleted
                status.state = RdsState::Deleted;
                return Ok(());
            }
        };

        info!(
            "Deleting RDS instance {} for project {}",
            instance_id, project_name
        );
        status.state = RdsState::Deleting;

        // Delete the RDS instance
        match self
            .rds_client
            .delete_db_instance()
            .db_instance_identifier(&instance_id)
            .skip_final_snapshot(true)
            .send()
            .await
        {
            Ok(_) => {
                info!("RDS delete request sent for instance {}", instance_id);
                // Wait briefly for deletion to start
                sleep(Duration::from_secs(5)).await;

                // Verify deletion
                let mut retries = 0;
                while retries < 30 {
                    match self
                        .rds_client
                        .describe_db_instances()
                        .db_instance_identifier(&instance_id)
                        .send()
                        .await
                    {
                        Ok(resp) => {
                            let instances = resp.db_instances();
                            if instances.is_empty() {
                                info!("RDS instance {} successfully deleted", instance_id);
                                status.state = RdsState::Deleted;
                                return Ok(());
                            }
                            // Instance still exists, check status
                            if let Some(instance) = instances.first() {
                                if let Some(instance_status) = instance.db_instance_status() {
                                    info!(
                                        "RDS instance {} deletion in progress (status: {})",
                                        instance_id, instance_status
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            let error_str = format!("{}", e);
                            if error_str.contains("DBInstanceNotFound") {
                                info!("RDS instance {} successfully deleted", instance_id);
                                status.state = RdsState::Deleted;
                                return Ok(());
                            }
                            error!("Error checking RDS instance deletion: {}", e);
                        }
                    }
                    retries += 1;
                    sleep(Duration::from_secs(10)).await;
                }

                warn!(
                    "RDS instance {} deletion timeout, marking as deleted anyway",
                    instance_id
                );
                status.state = RdsState::Deleted;
            }
            Err(e) => {
                let error_str = format!("{}", e);
                if error_str.contains("DBInstanceNotFound") {
                    info!("RDS instance {} already deleted", instance_id);
                    status.state = RdsState::Deleted;
                } else {
                    error!("Failed to delete RDS instance {}: {}", instance_id, e);
                    status.error = Some(format!("Failed to delete instance: {}", e));
                }
            }
        }

        Ok(())
    }

    fn generate_password(&self) -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
        let mut rng = rand::thread_rng();
        (0..32)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }
}

#[async_trait]
impl Extension for AwsRdsProvisioner {
    fn name(&self) -> &str {
        EXTENSION_NAME
    }

    async fn validate_spec(&self, spec: &Value) -> Result<()> {
        let parsed: AwsRdsSpec =
            serde_json::from_value(spec.clone()).context("Failed to parse AWS RDS spec")?;

        if parsed.engine != "postgres" {
            anyhow::bail!("Only 'postgres' engine is currently supported");
        }

        Ok(())
    }

    fn start(&self) {
        let db_pool = self.db_pool.clone();
        let rds_client = self.rds_client.clone();
        let encryption_provider = self.encryption_provider.clone();
        let region = self.region.clone();
        let instance_size = self.instance_size.clone();
        let disk_size = self.disk_size;
        let instance_id_template = self.instance_id_template.clone();

        tokio::spawn(async move {
            info!("Starting AWS RDS extension reconciliation loop");
            loop {
                // List ALL project extensions (not filtered by project)
                match sqlx::query_as::<_, db::models::ProjectExtension>(
                    r#"
                    SELECT project_id, extension,
                           spec as "spec: serde_json::Value",
                           status as "status: serde_json::Value",
                           created_at, updated_at, deleted_at
                    FROM project_extensions
                    WHERE extension = $1
                    "#,
                )
                .bind(EXTENSION_NAME)
                .fetch_all(&db_pool)
                .await
                {
                    Ok(extensions) => {
                        for ext in extensions {
                            let provisioner = AwsRdsProvisioner::new(
                                rds_client.clone(),
                                db_pool.clone(),
                                encryption_provider.clone(),
                                region.clone(),
                                instance_size.clone(),
                                disk_size,
                                instance_id_template.clone(),
                            );

                            if let Err(e) = provisioner.reconcile_single(ext).await {
                                error!("Failed to reconcile AWS RDS extension: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to list extensions: {}", e);
                    }
                }

                // Wait before next reconcile
                sleep(Duration::from_secs(30)).await;
            }
        });
    }

    async fn before_deployment(
        &self,
        _deployment_id: Uuid,
        project_id: Uuid,
        deployment_group: &str,
    ) -> Result<()> {
        // Find the extension for this project
        let ext =
            db_extensions::find_by_project_and_name(&self.db_pool, project_id, EXTENSION_NAME)
                .await?
                .ok_or_else(|| anyhow::anyhow!("AWS RDS extension not found for project"))?;

        // Parse status
        let status: AwsRdsStatus =
            serde_json::from_value(ext.status.clone()).context("Failed to parse AWS RDS status")?;

        // Check if instance is available
        if status.state != RdsState::Available {
            anyhow::bail!(
                "RDS instance is not available (current state: {:?})",
                status.state
            );
        }

        let endpoint = status
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("RDS endpoint not set"))?;

        let master_username = status
            .master_username
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Master username not set"))?;

        let encrypted_password = status
            .master_password_encrypted
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Master password not set"))?;

        // Decrypt password
        let master_password = self
            .encryption_provider
            .decrypt(encrypted_password)
            .await
            .context("Failed to decrypt master password")?;

        // Set environment variables for the deployment
        let _database_url = format!(
            "postgres://{}:{}@{}/postgres",
            master_username, master_password, endpoint
        );

        // Write env vars to deployment_env_vars table
        // This is a simplified version - in reality you'd use the deployment env vars module
        // For now, we'll just log what would be set
        info!(
            "Setting DATABASE_URL for deployment group: {}",
            deployment_group
        );

        // TODO: If deployment_group != "default", create a copy of the default database
        // using PostgreSQL's CREATE DATABASE ... WITH TEMPLATE command
        // TODO: Write env vars to deployment_env_vars table

        Ok(())
    }
}
