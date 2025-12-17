use crate::db::{
    self, env_vars as db_env_vars, extensions as db_extensions, postgres_admin,
    projects as db_projects,
};
use crate::server::encryption::EncryptionProvider;
use crate::server::extensions::Extension;
use anyhow::{Context, Result};
use async_trait::async_trait;
use aws_sdk_rds::Client as RdsClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};
use uuid::Uuid;

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

/// Credentials for a database user
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseUserCredentials {
    /// Database name
    pub database_name: String,
    /// Username for this database
    pub username: String,
    /// Encrypted password for this user
    pub password_encrypted: String,
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
    /// Map of database names to their user credentials (deployment_group -> credentials)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub database_users: HashMap<String, DatabaseUserCredentials>,
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

pub struct AwsRdsProvisionerConfig {
    pub name: String,
    pub rds_client: RdsClient,
    pub db_pool: sqlx::PgPool,
    pub encryption_provider: Arc<dyn EncryptionProvider>,
    pub region: String,
    pub instance_size: String,
    pub disk_size: i32,
    pub instance_id_template: String,
    pub vpc_security_group_ids: Option<Vec<String>>,
    pub db_subnet_group_name: Option<String>,
}

pub struct AwsRdsProvisioner {
    name: String,
    rds_client: RdsClient,
    db_pool: sqlx::PgPool,
    encryption_provider: Arc<dyn EncryptionProvider>,
    region: String,
    instance_size: String,
    disk_size: i32,
    instance_id_template: String,
    vpc_security_group_ids: Option<Vec<String>>,
    db_subnet_group_name: Option<String>,
}

impl AwsRdsProvisioner {
    pub fn new(config: AwsRdsProvisionerConfig) -> Self {
        Self {
            name: config.name,
            rds_client: config.rds_client,
            db_pool: config.db_pool,
            encryption_provider: config.encryption_provider,
            region: config.region,
            instance_size: config.instance_size,
            disk_size: config.disk_size,
            instance_id_template: config.instance_id_template,
            vpc_security_group_ids: config.vpc_security_group_ids,
            db_subnet_group_name: config.db_subnet_group_name,
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
        let project = db_projects::find_by_id(&self.db_pool, project_extension.project_id)
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
                database_users: HashMap::new(),
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
                    &self.name,
                    &serde_json::to_value(&status)?,
                )
                .await?;

                // If deletion is complete, hard delete the record and remove finalizer
                if status.state == RdsState::Deleted {
                    // Remove finalizer so project can be deleted
                    if let Err(e) = db_projects::remove_finalizer(
                        &self.db_pool,
                        project_extension.project_id,
                        &self.name,
                    )
                    .await
                    {
                        error!(
                            "Failed to remove finalizer from project {}: {}",
                            project.name, e
                        );
                    } else {
                        info!(
                            "Removed finalizer '{}' from project {}",
                            self.name, project.name
                        );
                    }

                    db_extensions::delete_permanently(
                        &self.db_pool,
                        project_extension.project_id,
                        &self.name,
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
                self.handle_creating(&mut status, &project.name, project.id)
                    .await?;
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
            &self.name,
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

        let mut create_request = self
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
            .storage_encrypted(true);

        // Add VPC security groups if configured
        if let Some(ref security_groups) = self.vpc_security_group_ids {
            for sg in security_groups {
                create_request = create_request.vpc_security_group_ids(sg);
            }
        }

        // Add DB subnet group if configured
        if let Some(ref subnet_group) = self.db_subnet_group_name {
            create_request = create_request.db_subnet_group_name(subnet_group);
        }

        match create_request.send().await {
            Ok(_) => {
                info!("RDS create request sent for instance {}", instance_id);
                status.state = RdsState::Creating;
                status.instance_id = Some(instance_id);
                status.master_username = Some(master_username);
                status.master_password_encrypted = Some(encrypted_password);
                status.error = None;
            }
            Err(e) => {
                error!("Failed to create RDS instance {}: {:?}", instance_id, e);
                status.state = RdsState::Failed;
                status.error = Some(format!("Failed to create instance: {:?}", e));
            }
        }

        Ok(())
    }

    async fn handle_creating(
        &self,
        status: &mut AwsRdsStatus,
        project_name: &str,
        project_id: Uuid,
    ) -> Result<()> {
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

                                // Add finalizer to ensure cleanup before project deletion
                                if let Err(e) = db_projects::add_finalizer(
                                    &self.db_pool,
                                    project_id,
                                    &self.name,
                                )
                                .await
                                {
                                    error!(
                                        "Failed to add finalizer for project {}: {}",
                                        project_name, e
                                    );
                                } else {
                                    info!(
                                        "Added finalizer '{}' to project {}",
                                        self.name, project_name
                                    );
                                }

                                // Extract endpoint
                                if let Some(endpoint) = instance.endpoint() {
                                    if let (Some(address), Some(port)) =
                                        (endpoint.address(), endpoint.port())
                                    {
                                        status.endpoint = Some(format!("{}:{}", address, port));

                                        // Create the default database for the project
                                        if let (Some(username), Some(encrypted_pass)) = (
                                            status.master_username.as_ref(),
                                            status.master_password_encrypted.as_ref(),
                                        ) {
                                            match self
                                                .encryption_provider
                                                .decrypt(encrypted_pass)
                                                .await
                                            {
                                                Ok(password) => {
                                                    let admin_db_url = format!(
                                                        "postgres://{}:{}@{}:{}/postgres",
                                                        username, password, address, port
                                                    );

                                                    if let Err(e) = self
                                                        .create_default_database(
                                                            &admin_db_url,
                                                            project_name,
                                                        )
                                                        .await
                                                    {
                                                        error!(
                                                            "Failed to create default database for project '{}': {}",
                                                            project_name, e
                                                        );
                                                    }
                                                }
                                                Err(e) => {
                                                    error!(
                                                        "Failed to decrypt master password: {}",
                                                        e
                                                    );
                                                }
                                            }
                                        }
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
                error!("Failed to describe RDS instance {}: {:?}", instance_id, e);
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
                warn!("Failed to verify RDS instance {}: {:?}", instance_id, e);
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
                            let error_str = format!("{:?}", e);
                            if error_str.contains("DBInstanceNotFound") {
                                info!("RDS instance {} successfully deleted", instance_id);
                                status.state = RdsState::Deleted;
                                return Ok(());
                            }
                            error!("Error checking RDS instance deletion: {:?}", e);
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
                let error_str = format!("{:?}", e);
                if error_str.contains("DBInstanceNotFound") {
                    info!("RDS instance {} already deleted", instance_id);
                    status.state = RdsState::Deleted;
                } else {
                    error!("Failed to delete RDS instance {}: {:?}", instance_id, e);
                    status.error = Some(format!("Failed to delete instance: {:?}", e));
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

    /// Create the default database for a project
    async fn create_default_database(&self, admin_db_url: &str, database_name: &str) -> Result<()> {
        // Connect to the postgres database to run administrative commands
        let pool = PgPool::connect(admin_db_url)
            .await
            .context("Failed to connect to RDS instance")?;

        // Check if the database already exists
        let exists = postgres_admin::database_exists(&pool, database_name).await?;

        if exists {
            info!(
                "Database '{}' already exists, skipping creation",
                database_name
            );
            pool.close().await;
            return Ok(());
        }

        // Create the database
        let sanitized_db = sanitize_identifier(database_name)?;
        let sanitized_owner = sanitize_identifier("postgres")?;

        postgres_admin::create_database(&pool, &sanitized_db, &sanitized_owner).await?;

        info!("Successfully created default database '{}'", database_name);

        pool.close().await;
        Ok(())
    }

    /// Create a copy of a database using PostgreSQL's CREATE DATABASE ... WITH TEMPLATE
    /// Falls back to creating an empty database if the template doesn't exist
    async fn create_database_copy(
        &self,
        admin_db_url: &str,
        new_database: &str,
        template_database: &str,
    ) -> Result<()> {
        // Connect to the postgres database to run administrative commands
        let pool = PgPool::connect(admin_db_url)
            .await
            .context("Failed to connect to RDS instance")?;

        // Check if the new database already exists
        let exists = postgres_admin::database_exists(&pool, new_database).await?;

        if exists {
            info!(
                "Database '{}' already exists, skipping creation",
                new_database
            );
            pool.close().await;
            return Ok(());
        }

        // Check if the template database exists
        let template_exists = postgres_admin::database_exists(&pool, template_database).await?;

        let sanitized_new_db = sanitize_identifier(new_database)?;
        let sanitized_owner = sanitize_identifier("postgres")?;

        if template_exists {
            // Create from template if it exists
            let sanitized_template_db = sanitize_identifier(template_database)?;

            postgres_admin::create_database_from_template(
                &pool,
                &sanitized_new_db,
                &sanitized_template_db,
                &sanitized_owner,
            )
            .await?;

            info!(
                "Successfully created database '{}' from template '{}'",
                new_database, template_database
            );
        } else {
            // Fall back to creating an empty database
            warn!(
                "Template database '{}' does not exist, creating empty database '{}' instead",
                template_database, new_database
            );

            postgres_admin::create_database(&pool, &sanitized_new_db, &sanitized_owner).await?;

            info!("Successfully created empty database '{}'", new_database);
        }

        pool.close().await;
        Ok(())
    }
}

/// Sanitize a PostgreSQL identifier (database name, user name, etc.)
/// to prevent SQL injection
fn sanitize_identifier(identifier: &str) -> Result<String> {
    // Only allow alphanumeric characters, underscores, and hyphens
    if !identifier
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        anyhow::bail!("Invalid identifier: contains illegal characters");
    }

    // Quote the identifier to handle reserved words and special characters
    Ok(format!("\"{}\"", identifier))
}

#[async_trait]
impl Extension for AwsRdsProvisioner {
    fn name(&self) -> &str {
        &self.name
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
        let name = self.name.clone();
        let db_pool = self.db_pool.clone();
        let rds_client = self.rds_client.clone();
        let encryption_provider = self.encryption_provider.clone();
        let region = self.region.clone();
        let instance_size = self.instance_size.clone();
        let disk_size = self.disk_size;
        let instance_id_template = self.instance_id_template.clone();
        let vpc_security_group_ids = self.vpc_security_group_ids.clone();
        let db_subnet_group_name = self.db_subnet_group_name.clone();

        tokio::spawn(async move {
            info!(
                "Starting AWS RDS extension reconciliation loop for '{}'",
                name
            );
            loop {
                // List ALL project extensions (not filtered by project)
                match db_extensions::list_by_extension_name(&db_pool, &name).await {
                    Ok(extensions) => {
                        for ext in extensions {
                            let provisioner = AwsRdsProvisioner::new(AwsRdsProvisionerConfig {
                                name: name.clone(),
                                rds_client: rds_client.clone(),
                                db_pool: db_pool.clone(),
                                encryption_provider: encryption_provider.clone(),
                                region: region.clone(),
                                instance_size: instance_size.clone(),
                                disk_size,
                                instance_id_template: instance_id_template.clone(),
                                vpc_security_group_ids: vpc_security_group_ids.clone(),
                                db_subnet_group_name: db_subnet_group_name.clone(),
                            });

                            if let Err(e) = provisioner.reconcile_single(ext).await {
                                error!("Failed to reconcile AWS RDS extension: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to list extensions: {:?}", e);
                    }
                }

                // Wait before next reconcile
                sleep(Duration::from_secs(30)).await;
            }
        });
    }

    async fn before_deployment(
        &self,
        deployment_id: Uuid,
        project_id: Uuid,
        deployment_group: &str,
    ) -> Result<()> {
        // Find the extension for this project
        let ext = db_extensions::find_by_project_and_name(&self.db_pool, project_id, &self.name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Extension '{}' not found for project", self.name))?;

        // Get project info for database naming
        let project = db_projects::find_by_id(&self.db_pool, project_id)
            .await
            .context("Failed to find project")?
            .ok_or_else(|| anyhow::anyhow!("Project not found"))?;

        // Parse status (mutable so we can update database_users)
        let mut status: AwsRdsStatus =
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

        // Determine database name based on deployment group
        // Sanitize deployment_group for use in database/user names (replace slashes and special chars)
        let safe_deployment_group = deployment_group.replace(['/', '-'], "_");

        let database_name = if deployment_group == "default" {
            project.name.clone()
        } else {
            format!("{}_{}", project.name, safe_deployment_group)
        };

        // Connect to the RDS instance to manage databases and users
        let admin_db_url = format!(
            "postgres://{}:{}@{}/postgres",
            master_username, master_password, endpoint
        );

        // If deployment_group is not "default", create a copy of the default database
        if deployment_group != "default" {
            info!(
                "Creating database copy '{}' from template '{}'",
                database_name, project.name
            );

            self.create_database_copy(&admin_db_url, &database_name, &project.name)
                .await
                .context("Failed to create database copy for deployment group")?;
        }

        // Check if we already have credentials for this deployment group
        let (db_username, db_password) =
            if let Some(creds) = status.database_users.get(deployment_group) {
                // Reuse existing credentials
                info!(
                    "Reusing existing database user '{}' for deployment group '{}'",
                    creds.username, deployment_group
                );

                let password = self
                    .encryption_provider
                    .decrypt(&creds.password_encrypted)
                    .await
                    .context("Failed to decrypt database user password")?;

                (creds.username.clone(), password)
            } else {
                // Create new database user credentials
                let username = if deployment_group == "default" {
                    format!("{}_user", project.name)
                } else {
                    format!("{}_{}_user", project.name, safe_deployment_group)
                };
                let password = self.generate_password();

                info!(
                    "Creating new database user '{}' for deployment group '{}'",
                    username, deployment_group
                );

                let pool = PgPool::connect(&admin_db_url)
                    .await
                    .context("Failed to connect to RDS instance for user creation")?;

                // Check if user already exists (shouldn't happen, but handle it)
                let user_exists = postgres_admin::user_exists(&pool, &username)
                    .await
                    .context("Failed to check if user exists")?;

                if !user_exists {
                    // Sanitize username for CREATE USER
                    let sanitized_username = sanitize_identifier(&username)?;

                    postgres_admin::create_user(&pool, &sanitized_username, &password)
                        .await
                        .context("Failed to create database user")?;

                    info!("Created database user '{}'", username);
                } else {
                    warn!(
                    "Database user '{}' already exists in PostgreSQL but not in status, reusing it",
                    username
                );
                }

                // Grant privileges on the database to the user
                let sanitized_username = sanitize_identifier(&username)?;
                let sanitized_database = sanitize_identifier(&database_name)?;

                postgres_admin::grant_database_privileges(
                    &pool,
                    &sanitized_database,
                    &sanitized_username,
                )
                .await
                .context("Failed to grant database privileges")?;

                info!(
                    "Granted privileges on database '{}' to user '{}'",
                    database_name, username
                );

                pool.close().await;

                // Store credentials in status
                let encrypted_password = self
                    .encryption_provider
                    .encrypt(&password)
                    .await
                    .context("Failed to encrypt database user password")?;

                status.database_users.insert(
                    deployment_group.to_string(),
                    DatabaseUserCredentials {
                        database_name: database_name.clone(),
                        username: username.clone(),
                        password_encrypted: encrypted_password,
                    },
                );

                // Update extension status in database
                db_extensions::update_status(
                    &self.db_pool,
                    project_id,
                    &self.name,
                    &serde_json::to_value(&status)?,
                )
                .await
                .context("Failed to update extension status with new database user")?;

                info!(
                    "Stored credentials for deployment group '{}' in extension status",
                    deployment_group
                );

                (username, password)
            };

        // Build DATABASE_URL for the deployment using the dedicated user
        let database_url = format!(
            "postgres://{}:{}@{}/{}",
            db_username, db_password, endpoint, database_name
        );

        // Encrypt the DATABASE_URL before storing
        let encrypted_database_url = self
            .encryption_provider
            .encrypt(&database_url)
            .await
            .context("Failed to encrypt DATABASE_URL")?;

        // Write env var to deployment_env_vars table
        db_env_vars::upsert_deployment_env_var(
            &self.db_pool,
            deployment_id,
            "DATABASE_URL",
            &encrypted_database_url,
            true, // is_secret
        )
        .await
        .context("Failed to write DATABASE_URL to deployment_env_vars")?;

        info!(
            "Set DATABASE_URL for deployment {} (group: {}, database: {})",
            deployment_id, deployment_group, database_name
        );

        Ok(())
    }
}
