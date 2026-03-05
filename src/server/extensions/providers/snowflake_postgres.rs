use crate::db::{extensions as db_extensions, projects as db_projects};
use crate::server::crds::snowflake_postgres::{
    DatabaseIsolation, DatabaseState, DatabaseStatus, SnowflakePostgres, SnowflakePostgresSpec,
    SnowflakePostgresState, SnowflakePostgresStatus,
};
use crate::server::encryption::EncryptionProvider;
use crate::server::extensions::{Extension, InjectedEnvVar, InjectedEnvVarValue};
use anyhow::{Context, Result};
use async_trait::async_trait;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{Api, DeleteParams, Patch, PatchParams, PostParams};
use kube::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const SNOWFLAKE_POSTGRES_ADMIN_USER: &str = "riseadmin";
const EXTENSION_TYPE: &str = "snowflake-postgres-provisioner";

fn default_database_isolation() -> DatabaseIsolation {
    DatabaseIsolation::Shared
}

fn default_database_url_env_var() -> Option<String> {
    Some("DATABASE_URL".to_string())
}

fn default_true() -> bool {
    true
}

/// User-facing spec for the Snowflake Postgres extension (stored in Rise DB).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SnowflakePostgresExtensionSpec {
    /// Database isolation mode for deployment groups.
    #[serde(default = "default_database_isolation")]
    pub database_isolation: DatabaseIsolation,

    /// Environment variable name for the database URL.
    /// Set to `null` to disable injection.
    #[serde(default = "default_database_url_env_var")]
    pub database_url_env_var: Option<String>,

    /// Whether to inject `PG*` environment variables.
    #[serde(default = "default_true")]
    pub inject_pg_vars: bool,
}

/// Configuration for creating a [`SnowflakePostgresProvisioner`].
pub struct SnowflakePostgresProvisionerConfig {
    /// Rise database pool (for reading extension records).
    pub db_pool: sqlx::PgPool,
    /// Encryption provider (for encrypting/decrypting credentials).
    pub encryption_provider: Arc<dyn EncryptionProvider>,
    /// Kubernetes client.
    pub kube_client: Client,
    /// Kubernetes namespace where `SnowflakePostgres` CRDs are created.
    pub namespace: String,
    /// Snowflake account identifier (e.g. `"myorg.us-east-1"`).
    pub account: String,
    /// Snowflake user used for provisioning databases.
    pub user: String,
    /// Optional Snowflake role.
    pub role: Option<String>,
    /// Optional Snowflake warehouse.
    pub warehouse: Option<String>,
    /// Snowflake authentication configuration.
    pub auth: crate::server::settings::SnowflakeAuth,
}

/// Extension provider that provisions Snowflake Postgres databases via Kubernetes CRDs.
///
/// This extension bridges the Rise extension system with Kubernetes custom resources.
/// When a user creates a `snowflake-postgres-provisioner` extension for a project,
/// Rise creates a [`SnowflakePostgres`] CRD in the configured Kubernetes namespace.
/// An embedded CRD controller loop watches these resources and performs the actual
/// Snowflake database provisioning.
pub struct SnowflakePostgresProvisioner {
    db_pool: sqlx::PgPool,
    encryption_provider: Arc<dyn EncryptionProvider>,
    kube_client: Client,
    namespace: String,
    account: String,
    user: String,
    role: Option<String>,
    warehouse: Option<String>,
    auth: crate::server::settings::SnowflakeAuth,
}

impl SnowflakePostgresProvisioner {
    pub fn new(config: SnowflakePostgresProvisionerConfig) -> Self {
        Self {
            db_pool: config.db_pool,
            encryption_provider: config.encryption_provider,
            kube_client: config.kube_client,
            namespace: config.namespace,
            account: config.account,
            user: config.user,
            role: config.role,
            warehouse: config.warehouse,
            auth: config.auth,
        }
    }

    /// Return the Kubernetes API handle for `SnowflakePostgres` resources.
    fn crd_api(&self) -> Api<SnowflakePostgres> {
        Api::namespaced(self.kube_client.clone(), &self.namespace)
    }

    /// Compute the Kubernetes resource name for a project/extension pair.
    fn crd_name(project_name: &str, extension_name: &str) -> String {
        // Must be a valid DNS label: lowercase, alphanumeric, hyphens only.
        format!("{}-{}", project_name, extension_name)
    }

    /// Return the Rise finalizer name for this extension instance.
    fn finalizer_name(extension_name: &str) -> String {
        format!("rise.dev/extension/{}/{}", EXTENSION_TYPE, extension_name)
    }

    /// Generate a cryptographically random alphanumeric password (32 chars).
    fn generate_password(&self) -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..32)
            .map(|_| {
                let idx = rng.gen_range(0..62usize);
                match idx {
                    0..=9 => (b'0' + idx as u8) as char,
                    10..=35 => (b'a' + (idx - 10) as u8) as char,
                    _ => (b'A' + (idx - 36) as u8) as char,
                }
            })
            .collect()
    }

    // -------------------------------------------------------------------------
    // CRD lifecycle helpers
    // -------------------------------------------------------------------------

    /// Ensure a `SnowflakePostgres` CRD exists (create or update spec).
    async fn ensure_crd_exists(
        &self,
        project_name: &str,
        extension_name: &str,
        ext_spec: &SnowflakePostgresExtensionSpec,
    ) -> Result<()> {
        let api = self.crd_api();
        let name = Self::crd_name(project_name, extension_name);

        let desired_spec = SnowflakePostgresSpec {
            project_name: project_name.to_string(),
            extension_name: extension_name.to_string(),
            database_isolation: ext_spec.database_isolation.clone(),
            database_url_env_var: ext_spec.database_url_env_var.clone(),
            inject_pg_vars: ext_spec.inject_pg_vars,
        };

        match api.get(&name).await {
            Ok(_existing) => {
                let patch = json!({ "spec": serde_json::to_value(&desired_spec)? });
                api.patch(
                    &name,
                    &PatchParams::apply("rise").force(),
                    &Patch::Merge(&patch),
                )
                .await
                .with_context(|| format!("Failed to patch SnowflakePostgres CRD '{}'", name))?;
                debug!("Patched SnowflakePostgres CRD '{}'", name);
            }
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                let crd = SnowflakePostgres {
                    metadata: ObjectMeta {
                        name: Some(name.clone()),
                        namespace: Some(self.namespace.clone()),
                        ..Default::default()
                    },
                    spec: desired_spec,
                    status: None,
                };
                api.create(&PostParams::default(), &crd)
                    .await
                    .with_context(|| {
                        format!("Failed to create SnowflakePostgres CRD '{}'", name)
                    })?;
                info!(
                    "Created SnowflakePostgres CRD '{}' in namespace '{}'",
                    name, self.namespace
                );
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("Failed to get SnowflakePostgres CRD '{}'", name));
            }
        }
        Ok(())
    }

    /// Patch the CRD status to `Deleting` so the CRD controller tears down resources.
    async fn mark_crd_deleting(&self, project_name: &str, extension_name: &str) -> Result<()> {
        let api = self.crd_api();
        let name = Self::crd_name(project_name, extension_name);

        match api.get(&name).await {
            Ok(_) => {
                let patch = json!({ "status": { "state": "Deleting" } });
                api.patch_status(
                    &name,
                    &PatchParams::apply("rise").force(),
                    &Patch::Merge(&patch),
                )
                .await
                .with_context(|| {
                    format!(
                        "Failed to mark SnowflakePostgres CRD '{}' as Deleting",
                        name
                    )
                })?;
                info!("Marked SnowflakePostgres CRD '{}' as Deleting", name);
            }
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                debug!(
                    "SnowflakePostgres CRD '{}' not found, already deleted",
                    name
                );
            }
            Err(e) => {
                return Err(e).with_context(|| {
                    format!(
                        "Failed to get SnowflakePostgres CRD '{}' for deletion",
                        name
                    )
                });
            }
        }
        Ok(())
    }

    /// Hard-delete the Kubernetes CRD object.
    async fn delete_crd(&self, project_name: &str, extension_name: &str) -> Result<()> {
        let api = self.crd_api();
        let name = Self::crd_name(project_name, extension_name);

        match api.delete(&name, &DeleteParams::default()).await {
            Ok(_) => {
                info!("Deleted SnowflakePostgres CRD '{}'", name);
            }
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                debug!("SnowflakePostgres CRD '{}' already gone", name);
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("Failed to delete SnowflakePostgres CRD '{}'", name));
            }
        }
        Ok(())
    }

    /// Read the status from a `SnowflakePostgres` CRD.
    async fn read_crd_status(
        &self,
        project_name: &str,
        extension_name: &str,
    ) -> Result<Option<SnowflakePostgresStatus>> {
        let api = self.crd_api();
        let name = Self::crd_name(project_name, extension_name);

        match api.get(&name).await {
            Ok(crd) => Ok(crd.status),
            Err(kube::Error::Api(ae)) if ae.code == 404 => Ok(None),
            Err(e) => {
                Err(e).with_context(|| format!("Failed to read SnowflakePostgres CRD '{}'", name))
            }
        }
    }

    /// Patch the status sub-resource of a `SnowflakePostgres` CRD.
    async fn patch_crd_status(&self, name: &str, status: &SnowflakePostgresStatus) -> Result<()> {
        let api = self.crd_api();
        let patch = json!({ "status": serde_json::to_value(status)? });
        api.patch_status(
            name,
            &PatchParams::apply("rise").force(),
            &Patch::Merge(&patch),
        )
        .await
        .with_context(|| format!("Failed to patch status of SnowflakePostgres CRD '{}'", name))?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Extension sync loop (Rise DB → Kubernetes)
    // -------------------------------------------------------------------------

    /// Loop: for every extension record in Rise, ensure the CRD exists in Kubernetes.
    async fn run_extension_sync_loop(&self) {
        info!("Starting SnowflakePostgres extension sync loop");
        loop {
            match db_extensions::list_by_extension_type(&self.db_pool, EXTENSION_TYPE).await {
                Ok(extensions) => {
                    let mut any_transitional = false;
                    for ext in extensions {
                        let status_state =
                            serde_json::from_value::<SnowflakePostgresStatus>(ext.status.clone())
                                .unwrap_or_default()
                                .state;
                        if matches!(
                            status_state,
                            SnowflakePostgresState::Pending
                                | SnowflakePostgresState::Creating
                                | SnowflakePostgresState::Deleting
                        ) || ext.deleted_at.is_some()
                        {
                            any_transitional = true;
                        }
                        if let Err(e) = self.reconcile_single(ext).await {
                            error!("Error reconciling SnowflakePostgres extension: {:?}", e);
                        }
                    }
                    let wait_secs = if any_transitional { 2 } else { 5 };
                    sleep(std::time::Duration::from_secs(wait_secs)).await;
                }
                Err(e) => {
                    error!("Failed to list SnowflakePostgres extensions: {:?}", e);
                    sleep(std::time::Duration::from_secs(10)).await;
                }
            }
        }
    }

    /// Reconcile a single Rise extension record: keep the CRD in sync.
    async fn reconcile_single(
        &self,
        project_extension: crate::db::models::ProjectExtension,
    ) -> Result<bool> {
        let project = db_projects::find_by_id(&self.db_pool, project_extension.project_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Project not found"))?;

        let ext_spec: SnowflakePostgresExtensionSpec =
            serde_json::from_value(project_extension.spec.clone())
                .context("Failed to parse SnowflakePostgres extension spec")?;

        // ---- Handle deletion ----
        if project_extension.deleted_at.is_some() {
            let ext_status: SnowflakePostgresStatus =
                serde_json::from_value(project_extension.status.clone()).unwrap_or_default();

            if ext_status.state == SnowflakePostgresState::Deleted {
                return Ok(false); // already done
            }

            self.mark_crd_deleting(&project.name, &project_extension.extension)
                .await?;

            // Check whether the CRD controller has finished tearing down.
            let crd_status = self
                .read_crd_status(&project.name, &project_extension.extension)
                .await?;

            let crd_deleted = crd_status
                .as_ref()
                .map(|s| s.state == SnowflakePostgresState::Deleted)
                .unwrap_or(false);

            if crd_deleted {
                self.delete_crd(&project.name, &project_extension.extension)
                    .await?;

                let finalizer = Self::finalizer_name(&project_extension.extension);
                if let Err(e) = db_projects::remove_finalizer(
                    &self.db_pool,
                    project_extension.project_id,
                    &finalizer,
                )
                .await
                {
                    error!(
                        "Failed to remove finalizer '{}' from project {}: {:?}",
                        finalizer, project.name, e
                    );
                }

                db_extensions::delete_permanently(
                    &self.db_pool,
                    project_extension.project_id,
                    &project_extension.extension,
                )
                .await?;

                info!(
                    "Permanently deleted SnowflakePostgres extension '{}' for project '{}'",
                    project_extension.extension, project.name
                );
            }

            return Ok(false);
        }

        // ---- Normal lifecycle: ensure CRD exists ----
        self.ensure_crd_exists(&project.name, &project_extension.extension, &ext_spec)
            .await?;

        // Add Rise finalizer so project cannot be deleted while the extension exists.
        let finalizer = Self::finalizer_name(&project_extension.extension);
        if !project.finalizers.contains(&finalizer) {
            if let Err(e) =
                db_projects::add_finalizer(&self.db_pool, project_extension.project_id, &finalizer)
                    .await
            {
                error!(
                    "Failed to add finalizer '{}' to project {}: {:?}",
                    finalizer, project.name, e
                );
            }
        }

        // Sync CRD status back to the Rise extension status field.
        if let Some(crd_status) = self
            .read_crd_status(&project.name, &project_extension.extension)
            .await?
        {
            let current: SnowflakePostgresStatus =
                serde_json::from_value(project_extension.status.clone()).unwrap_or_default();

            if serde_json::to_value(&crd_status)? != serde_json::to_value(&current)? {
                db_extensions::update_status(
                    &self.db_pool,
                    project_extension.project_id,
                    &project_extension.extension,
                    &serde_json::to_value(&crd_status)?,
                )
                .await?;
            }

            let needs_more_work = matches!(
                crd_status.state,
                SnowflakePostgresState::Pending
                    | SnowflakePostgresState::Creating
                    | SnowflakePostgresState::Deleting
            );
            return Ok(needs_more_work);
        }

        Ok(true) // CRD was just created; requeue
    }

    // -------------------------------------------------------------------------
    // CRD controller loop (Kubernetes → Snowflake)
    // -------------------------------------------------------------------------

    /// Loop: watch `SnowflakePostgres` CRDs and reconcile each one.
    async fn run_crd_controller_loop(&self) {
        info!(
            "Starting SnowflakePostgres CRD controller loop in namespace '{}'",
            self.namespace
        );
        loop {
            let api = self.crd_api();
            match api.list(&Default::default()).await {
                Ok(list) => {
                    for crd in list.items {
                        if let Err(e) = self.reconcile_crd(crd).await {
                            error!("Error reconciling SnowflakePostgres CRD: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to list SnowflakePostgres CRDs: {:?}", e);
                }
            }
            sleep(std::time::Duration::from_secs(5)).await;
        }
    }

    /// Reconcile one `SnowflakePostgres` CRD against the actual Snowflake state.
    async fn reconcile_crd(&self, crd: SnowflakePostgres) -> Result<()> {
        let name = crd.metadata.name.as_deref().unwrap_or("<unnamed>");
        let spec = &crd.spec;
        let status = crd.status.clone().unwrap_or_default();

        debug!(
            "Reconciling SnowflakePostgres CRD '{}' (state: {:?})",
            name, status.state
        );

        match status.state {
            SnowflakePostgresState::Pending => {
                self.handle_crd_pending(name, spec).await?;
            }
            SnowflakePostgresState::Creating => {
                self.handle_crd_creating(name, spec, status).await?;
            }
            SnowflakePostgresState::Available => {
                // Stable; health checks could go here in future.
            }
            SnowflakePostgresState::Deleting => {
                self.handle_crd_deleting(name, spec, status).await?;
            }
            SnowflakePostgresState::Failed => {
                info!(
                    "SnowflakePostgres CRD '{}' is Failed, resetting to Pending",
                    name
                );
                self.patch_crd_status(
                    name,
                    &SnowflakePostgresStatus {
                        state: SnowflakePostgresState::Pending,
                        error: None,
                        ..Default::default()
                    },
                )
                .await?;
            }
            SnowflakePostgresState::Deleted => {
                // Terminal; nothing to do.
            }
        }
        Ok(())
    }

    /// Begin provisioning: create the Snowflake database and move to `Creating`.
    async fn handle_crd_pending(&self, name: &str, spec: &SnowflakePostgresSpec) -> Result<()> {
        info!(
            "Provisioning Snowflake database for SnowflakePostgres CRD '{}'",
            name
        );

        let master_password = self.generate_password();
        let encrypted_password = self
            .encryption_provider
            .encrypt(&master_password)
            .await
            .context("Failed to encrypt master password")?;

        let endpoint = format!("{}.snowflakecomputing.com:5432", self.account);

        match self
            .provision_snowflake_database(&spec.project_name, &spec.extension_name)
            .await
        {
            Ok(()) => {
                self.patch_crd_status(
                    name,
                    &SnowflakePostgresStatus {
                        state: SnowflakePostgresState::Creating,
                        endpoint: Some(endpoint),
                        master_username: Some(SNOWFLAKE_POSTGRES_ADMIN_USER.to_string()),
                        master_password_encrypted: Some(encrypted_password),
                        databases: HashMap::new(),
                        error: None,
                    },
                )
                .await?;
            }
            Err(e) => {
                error!(
                    "Failed to provision Snowflake database for CRD '{}': {:?}",
                    name, e
                );
                self.patch_crd_status(
                    name,
                    &SnowflakePostgresStatus {
                        state: SnowflakePostgresState::Failed,
                        error: Some(format!("{:?}", e)),
                        ..Default::default()
                    },
                )
                .await?;
            }
        }
        Ok(())
    }

    /// Finish provisioning: create the default database user and move to `Available`.
    async fn handle_crd_creating(
        &self,
        name: &str,
        spec: &SnowflakePostgresSpec,
        mut status: SnowflakePostgresStatus,
    ) -> Result<()> {
        let default_db_name = format!("{}_db_default", spec.project_name);

        use std::collections::hash_map::Entry;
        if let Entry::Vacant(entry) = status.databases.entry(default_db_name.clone()) {
            let username = format!("{}_db_default_user", spec.project_name);
            match self
                .provision_snowflake_user(&spec.project_name, &default_db_name, &username)
                .await
            {
                Ok(()) => {
                    let password = self.generate_password();
                    let encrypted = self
                        .encryption_provider
                        .encrypt(&password)
                        .await
                        .unwrap_or_else(|_| password.clone());

                    entry.insert(DatabaseStatus {
                        user: username,
                        password_encrypted: encrypted,
                        status: DatabaseState::Available,
                        cleanup_scheduled_at: None,
                    });
                    status.state = SnowflakePostgresState::Available;
                    self.patch_crd_status(name, &status).await?;
                    info!("SnowflakePostgres CRD '{}' is now Available", name);
                }
                Err(e) => {
                    error!(
                        "Failed to provision default database for CRD '{}': {:?}",
                        name, e
                    );
                    status.state = SnowflakePostgresState::Failed;
                    status.error = Some(format!("{:?}", e));
                    self.patch_crd_status(name, &status).await?;
                }
            }
        } else {
            // Default database already set up; transition to Available.
            status.state = SnowflakePostgresState::Available;
            self.patch_crd_status(name, &status).await?;
        }
        Ok(())
    }

    /// Deprovision Snowflake resources and transition to `Deleted`.
    async fn handle_crd_deleting(
        &self,
        name: &str,
        spec: &SnowflakePostgresSpec,
        mut status: SnowflakePostgresStatus,
    ) -> Result<()> {
        info!(
            "Deprovisioning Snowflake database for SnowflakePostgres CRD '{}'",
            name
        );

        match self
            .deprovision_snowflake_database(&spec.project_name, &spec.extension_name)
            .await
        {
            Ok(()) => {
                status.state = SnowflakePostgresState::Deleted;
                self.patch_crd_status(name, &status).await?;
                info!("SnowflakePostgres CRD '{}' resources deleted", name);
            }
            Err(e) => {
                error!(
                    "Failed to deprovision Snowflake database for CRD '{}': {:?}",
                    name, e
                );
                status.error = Some(format!("{:?}", e));
                self.patch_crd_status(name, &status).await?;
            }
        }
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Snowflake provisioning helpers
    // -------------------------------------------------------------------------

    fn snowflake_db_name(project_name: &str, extension_name: &str) -> String {
        // Snowflake identifiers are case-insensitive; uppercase for clarity.
        format!(
            "RISE_{}_{}",
            project_name.to_uppercase().replace('-', "_"),
            extension_name.to_uppercase().replace('-', "_")
        )
    }

    fn escape_identifier(ident: &str) -> Result<String> {
        if ident.contains('"') {
            anyhow::bail!(
                "Snowflake identifier '{}' contains invalid double-quote characters",
                ident
            );
        }
        Ok(format!("\"{}\"", ident))
    }

    async fn provision_snowflake_database(
        &self,
        project_name: &str,
        extension_name: &str,
    ) -> Result<()> {
        let db_name = Self::snowflake_db_name(project_name, extension_name);
        let sql = format!(
            "CREATE DATABASE IF NOT EXISTS {}",
            Self::escape_identifier(&db_name)?
        );
        self.execute_sql(&sql)
            .await
            .with_context(|| format!("Failed to create Snowflake database '{}'", db_name))?;
        info!("Created Snowflake database '{}'", db_name);
        Ok(())
    }

    async fn provision_snowflake_user(
        &self,
        project_name: &str,
        _database_name: &str,
        username: &str,
    ) -> Result<()> {
        let db_name = Self::snowflake_db_name(project_name, username);
        let safe_db = Self::escape_identifier(&db_name)?;
        let safe_user = Self::escape_identifier(username)?;

        let create_user_sql = format!(
            "CREATE USER IF NOT EXISTS {} MUST_CHANGE_PASSWORD = FALSE",
            safe_user
        );
        self.execute_sql(&create_user_sql)
            .await
            .with_context(|| format!("Failed to create Snowflake user '{}'", username))?;

        let grant_sql = format!("GRANT USAGE ON DATABASE {} TO USER {}", safe_db, safe_user);
        self.execute_sql(&grant_sql).await.with_context(|| {
            format!(
                "Failed to grant database access to Snowflake user '{}'",
                username
            )
        })?;

        info!(
            "Provisioned Snowflake user '{}' with access to database '{}'",
            username, project_name
        );
        Ok(())
    }

    async fn deprovision_snowflake_database(
        &self,
        project_name: &str,
        extension_name: &str,
    ) -> Result<()> {
        let db_name = Self::snowflake_db_name(project_name, extension_name);
        let sql = format!(
            "DROP DATABASE IF EXISTS {}",
            Self::escape_identifier(&db_name)?
        );
        self.execute_sql(&sql)
            .await
            .with_context(|| format!("Failed to drop Snowflake database '{}'", db_name))?;
        info!("Dropped Snowflake database '{}'", db_name);
        Ok(())
    }

    /// Execute SQL on Snowflake using the configured credentials.
    async fn execute_sql(&self, sql: &str) -> Result<Vec<Value>> {
        use snowflake_connector_rs::{SnowflakeAuthMethod, SnowflakeClient, SnowflakeClientConfig};

        let auth_method = match &self.auth {
            crate::server::settings::SnowflakeAuth::Password { password } => {
                SnowflakeAuthMethod::Password(password.clone())
            }
            crate::server::settings::SnowflakeAuth::PrivateKey {
                key_source,
                private_key_password,
            } => {
                let key_pem = match key_source {
                    crate::server::settings::PrivateKeySource::Path { private_key_path } => {
                        std::fs::read_to_string(private_key_path).with_context(|| {
                            format!("Failed to read private key from '{}'", private_key_path)
                        })?
                    }
                    crate::server::settings::PrivateKeySource::Inline { private_key } => {
                        private_key.clone()
                    }
                };
                let password_bytes = private_key_password
                    .as_ref()
                    .map(|p| p.as_bytes().to_vec())
                    .unwrap_or_default();
                SnowflakeAuthMethod::KeyPair {
                    encrypted_pem: key_pem,
                    password: password_bytes,
                }
            }
        };

        let account_parts: Vec<&str> = self.account.split('.').collect();
        let account_identifier = account_parts
            .first()
            .ok_or_else(|| anyhow::anyhow!("Invalid Snowflake account format"))?
            .to_string();

        let config = SnowflakeClientConfig {
            account: account_identifier,
            warehouse: self.warehouse.clone(),
            database: None,
            schema: None,
            role: self.role.clone(),
            timeout: Some(std::time::Duration::from_secs(60)),
        };

        let client = SnowflakeClient::new(&self.user, auth_method, config)
            .context("Failed to create Snowflake client")?;

        let session = client
            .create_session()
            .await
            .context("Failed to create Snowflake session")?;

        if let Some(ref warehouse) = self.warehouse {
            let use_wh = format!("USE WAREHOUSE {}", Self::escape_identifier(warehouse)?);
            session
                .query(use_wh.as_str())
                .await
                .context("Failed to set Snowflake warehouse")?;
        }

        let rows = session
            .query(sql)
            .await
            .context("Failed to execute SQL on Snowflake")?;

        // The snowflake-connector-rs row type doesn't easily convert to JSON;
        // return an empty vec since our callers only check for errors, not row data.
        let _ = rows;
        Ok(vec![])
    }

    // -------------------------------------------------------------------------
    // Environment variable injection
    // -------------------------------------------------------------------------

    async fn build_injected_env_vars(
        &self,
        spec: &SnowflakePostgresExtensionSpec,
        endpoint: &str,
        database_name: &str,
        username: &str,
        password: &str,
    ) -> Result<Vec<InjectedEnvVar>> {
        let (host, port) = if let Some(idx) = endpoint.rfind(':') {
            (endpoint[..idx].to_string(), endpoint[idx + 1..].to_string())
        } else {
            (endpoint.to_string(), "5432".to_string())
        };

        let mut result: Vec<InjectedEnvVar> = Vec::new();

        if let Some(ref url_var) = spec.database_url_env_var {
            if !url_var.is_empty() {
                let url = format!(
                    "postgres://{}:{}@{}/{}",
                    username, password, endpoint, database_name
                );
                let encrypted_url = self
                    .encryption_provider
                    .encrypt(&url)
                    .await
                    .with_context(|| format!("Failed to encrypt {}", url_var))?;
                result.push(InjectedEnvVar {
                    key: url_var.clone(),
                    value: InjectedEnvVarValue::Protected {
                        decrypted: url,
                        encrypted: encrypted_url,
                    },
                });
            }
        }

        if spec.inject_pg_vars {
            for (key, value) in [
                ("PGHOST", host.as_str()),
                ("PGPORT", port.as_str()),
                ("PGDATABASE", database_name),
                ("PGUSER", username),
            ] {
                result.push(InjectedEnvVar {
                    key: key.to_string(),
                    value: InjectedEnvVarValue::Plain(value.to_string()),
                });
            }
            let encrypted_password = self
                .encryption_provider
                .encrypt(password)
                .await
                .context("Failed to encrypt PGPASSWORD")?;
            result.push(InjectedEnvVar {
                key: "PGPASSWORD".to_string(),
                value: InjectedEnvVarValue::Protected {
                    decrypted: password.to_string(),
                    encrypted: encrypted_password,
                },
            });
        }

        Ok(result)
    }
}

// =============================================================================
// Extension trait implementation
// =============================================================================

#[async_trait]
impl Extension for SnowflakePostgresProvisioner {
    fn extension_type(&self) -> &str {
        EXTENSION_TYPE
    }

    fn display_name(&self) -> &str {
        "Snowflake Postgres Database"
    }

    fn description(&self) -> &str {
        "Provisions a Postgres-compatible database on Snowflake via Kubernetes CRDs"
    }

    fn documentation(&self) -> &str {
        r#"# Snowflake Postgres Provisioner

Provisions a Postgres-compatible database on Snowflake for your project.

Rise manages the full lifecycle of a `SnowflakePostgres` Kubernetes CRD. The CRD
controller embedded in Rise performs the actual Snowflake provisioning.

## Configuration

```json
{
  "database_isolation": "shared",
  "database_url_env_var": "DATABASE_URL",
  "inject_pg_vars": true
}
```

### Fields

- `database_isolation` (`shared` | `isolated`): Whether deployment groups share
  one database or each receive an isolated database. Default: `shared`.

- `database_url_env_var`: Name of the env var for the connection URL. Set to
  `null` to disable. Default: `"DATABASE_URL"`.

- `inject_pg_vars`: Inject `PGHOST`, `PGPORT`, `PGDATABASE`, `PGUSER`,
  `PGPASSWORD`. Default: `true`.

## Injected Variables

- `DATABASE_URL` (or custom) – full `postgres://` connection URL (protected)
- `PGHOST` – Snowflake account hostname
- `PGPORT` – `5432`
- `PGDATABASE` – database name
- `PGUSER` – database username
- `PGPASSWORD` – database password (protected)
"#
    }

    fn spec_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "database_isolation": {
                    "type": "string",
                    "enum": ["shared", "isolated"],
                    "default": "shared",
                    "description": "Isolation mode for deployment groups"
                },
                "database_url_env_var": {
                    "type": ["string", "null"],
                    "default": "DATABASE_URL",
                    "description": "Env var name for the database URL"
                },
                "inject_pg_vars": {
                    "type": "boolean",
                    "default": true,
                    "description": "Inject PGHOST, PGPORT, PGDATABASE, PGUSER, PGPASSWORD"
                }
            }
        })
    }

    async fn validate_spec(&self, spec: &Value) -> Result<()> {
        serde_json::from_value::<SnowflakePostgresExtensionSpec>(spec.clone())
            .context("Invalid SnowflakePostgres extension spec")?;
        Ok(())
    }

    fn start(&self) {
        // Clone all fields for the two background tasks.
        let sync_provisioner = Self {
            db_pool: self.db_pool.clone(),
            encryption_provider: self.encryption_provider.clone(),
            kube_client: self.kube_client.clone(),
            namespace: self.namespace.clone(),
            account: self.account.clone(),
            user: self.user.clone(),
            role: self.role.clone(),
            warehouse: self.warehouse.clone(),
            auth: self.auth.clone(),
        };
        let crd_provisioner = Self {
            db_pool: self.db_pool.clone(),
            encryption_provider: self.encryption_provider.clone(),
            kube_client: self.kube_client.clone(),
            namespace: self.namespace.clone(),
            account: self.account.clone(),
            user: self.user.clone(),
            role: self.role.clone(),
            warehouse: self.warehouse.clone(),
            auth: self.auth.clone(),
        };

        // Extension sync: Rise DB → Kubernetes CRDs
        tokio::spawn(async move {
            sync_provisioner.run_extension_sync_loop().await;
        });

        // CRD controller: Kubernetes CRDs → Snowflake
        tokio::spawn(async move {
            crd_provisioner.run_crd_controller_loop().await;
        });
    }

    async fn before_deployment(
        &self,
        project_id: Uuid,
        deployment_group: &str,
    ) -> Result<Vec<InjectedEnvVar>> {
        let extensions = db_extensions::list_by_extension_type(&self.db_pool, EXTENSION_TYPE)
            .await?
            .into_iter()
            .filter(|e| e.project_id == project_id && e.deleted_at.is_none())
            .collect::<Vec<_>>();

        if extensions.is_empty() {
            return Ok(vec![]);
        }

        let ext = &extensions[0];
        if extensions.len() > 1 {
            warn!(
                "Multiple SnowflakePostgres extensions for project {}, using '{}'",
                project_id, ext.extension
            );
        }

        let spec: SnowflakePostgresExtensionSpec = serde_json::from_value(ext.spec.clone())
            .context("Failed to parse SnowflakePostgres spec")?;

        let project = db_projects::find_by_id(&self.db_pool, project_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Project not found"))?;

        // Read the CRD status — this is the source of truth for credentials.
        let status = self
            .read_crd_status(&project.name, &ext.extension)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "SnowflakePostgres CRD not found for extension '{}'",
                    ext.extension
                )
            })?;

        if status.state != SnowflakePostgresState::Available {
            anyhow::bail!(
                "SnowflakePostgres extension '{}' is not yet available (state: {:?})",
                ext.extension,
                status.state
            );
        }

        let endpoint = status
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("SnowflakePostgres endpoint not set"))?;

        let safe_group = deployment_group.replace(['/', '-'], "_");
        let database_name = match spec.database_isolation {
            DatabaseIsolation::Shared => format!("{}_db_default", project.name),
            DatabaseIsolation::Isolated => format!("{}_db_{}", project.name, safe_group),
        };

        // Retrieve per-database credentials from CRD status.
        let (username, password) = if let Some(db) = status.databases.get(&database_name) {
            if db.status != DatabaseState::Available {
                anyhow::bail!(
                    "SnowflakePostgres database '{}' is not available (state: {:?})",
                    database_name,
                    db.status
                );
            }
            let pw = self
                .encryption_provider
                .decrypt(&db.password_encrypted)
                .await
                .context("Failed to decrypt database password")?;
            (db.user.clone(), pw)
        } else {
            // Fall back to master credentials.
            let master_user = status
                .master_username
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("Master username not set"))?
                .to_string();
            let enc_pw = status
                .master_password_encrypted
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("Master password not set"))?;
            let pw = self
                .encryption_provider
                .decrypt(enc_pw)
                .await
                .context("Failed to decrypt master password")?;
            (master_user, pw)
        };

        self.build_injected_env_vars(&spec, endpoint, &database_name, &username, &password)
            .await
    }

    fn format_status(&self, status: &Value) -> String {
        let parsed: SnowflakePostgresStatus =
            serde_json::from_value(status.clone()).unwrap_or_default();
        match parsed.state {
            SnowflakePostgresState::Pending => "Pending".to_string(),
            SnowflakePostgresState::Creating => "Creating...".to_string(),
            SnowflakePostgresState::Available => parsed
                .endpoint
                .map(|ep| format!("Available ({})", ep))
                .unwrap_or_else(|| "Available".to_string()),
            SnowflakePostgresState::Deleting => "Deleting...".to_string(),
            SnowflakePostgresState::Deleted => "Deleted".to_string(),
            SnowflakePostgresState::Failed => parsed
                .error
                .map(|err| format!("Failed: {}", err))
                .unwrap_or_else(|| "Failed".to_string()),
        }
    }
}
