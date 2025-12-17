use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

pub mod registry;

#[cfg(feature = "server")]
pub mod handlers;
#[cfg(feature = "server")]
pub mod models;
#[cfg(feature = "server")]
pub mod providers;
#[cfg(feature = "server")]
pub mod routes;

/// Extension trait for project resource provisioning
#[async_trait]
pub trait Extension: Send + Sync {
    /// Unique identifier for this extension instance (configurable name)
    fn name(&self) -> &str;

    /// Extension type identifier (constant, used for UI registry lookup)
    /// Examples: "aws-rds-postgres", "aws-s3-bucket"
    /// This should be a constant string that doesn't change based on configuration
    fn extension_type(&self) -> &str;

    /// Validate extension spec on create/update
    ///
    /// Should check that the spec is valid JSONB and contains required fields.
    async fn validate_spec(&self, spec: &Value) -> Result<()>;

    /// Start the extension's background reconciliation loop(s)
    ///
    /// This method should spawn background tasks via `tokio::spawn` that run
    /// indefinitely until the process exits. The tasks will be automatically
    /// cleaned up when the process receives SIGTERM/SIGINT.
    ///
    /// Example implementation:
    /// ```ignore
    /// fn start(&self) {
    ///     let self_clone = Arc::clone(self);
    ///     tokio::spawn(async move {
    ///         let mut interval = tokio::time::interval(Duration::from_secs(30));
    ///         loop {
    ///             interval.tick().await;
    ///             self_clone.reconcile().await;
    ///         }
    ///     });
    /// }
    /// ```
    fn start(&self);

    /// Hook called before deployment creation
    ///
    /// This is a synchronous hook that must complete before the deployment
    /// proceeds. Extensions should use this to provision per-deployment resources
    /// (e.g., create database for deployment group) and inject environment variables.
    ///
    /// Extensions write environment variables directly to the deployment_env_vars
    /// table using the provided deployment_id.
    ///
    /// # Arguments
    /// * `deployment_id` - UUID of the deployment being created
    /// * `project_id` - Project UUID
    /// * `deployment_group` - Deployment group name (e.g., "default", "staging")
    ///
    /// # Returns
    /// Ok(()) if successful, Err if the deployment should be failed
    async fn before_deployment(
        &self,
        deployment_id: Uuid,
        project_id: Uuid,
        deployment_group: &str,
    ) -> Result<()>;

    /// Format the extension status for human-readable display
    ///
    /// This allows each extension provider to control how its status is
    /// displayed in the CLI without the CLI needing to know about provider-specific
    /// status structures.
    ///
    /// # Arguments
    /// * `status` - The status JSONB value from the database
    ///
    /// # Returns
    /// A human-readable string summarizing the extension status
    ///
    /// # Example
    /// For an RDS extension, this might return:
    /// - "Available (db.t4g.micro)"
    /// - "Creating..."
    /// - "Failed: Invalid subnet group"
    fn format_status(&self, status: &Value) -> String;

    /// Get a human-readable description of the extension
    ///
    /// This should be a concise one-line summary of what the extension does.
    ///
    /// # Returns
    /// A short description string (e.g., "Provisions a PostgreSQL database on AWS RDS")
    fn description(&self) -> &str;

    /// Get documentation for the extension
    ///
    /// This should provide comprehensive documentation about:
    /// - What the extension does
    /// - How to configure it
    /// - What environment variables it injects
    /// - Example configurations
    ///
    /// The documentation can be in markdown format.
    ///
    /// # Returns
    /// Documentation string (markdown supported)
    fn documentation(&self) -> &str;

    /// Get the JSON schema or example spec structure
    ///
    /// This should return a JSON Schema or example JSON structure that shows
    /// what fields are valid in the extension spec.
    ///
    /// # Returns
    /// JSON value representing the schema or example spec
    fn spec_schema(&self) -> Value;
}
