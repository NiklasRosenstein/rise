use std::collections::HashMap;

use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Isolation mode for deployment groups
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseIsolation {
    /// All deployment groups share the same database
    #[default]
    Shared,
    /// Each deployment group gets its own empty database
    Isolated,
}

fn default_database_isolation() -> DatabaseIsolation {
    DatabaseIsolation::Shared
}

fn default_true() -> bool {
    true
}

fn default_database_url_env_var() -> Option<String> {
    Some("DATABASE_URL".to_string())
}

/// Spec for the SnowflakePostgres custom resource
#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "rise.dev",
    version = "v1alpha1",
    kind = "SnowflakePostgres",
    namespaced,
    status = "SnowflakePostgresStatus",
    shortname = "sfpg",
    printcolumn = r#"{"name":"State","type":"string","jsonPath":".status.state"}"#,
    printcolumn = r#"{"name":"Endpoint","type":"string","jsonPath":".status.endpoint"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
pub struct SnowflakePostgresSpec {
    /// Rise project name
    pub project_name: String,

    /// Extension instance name
    pub extension_name: String,

    /// Database isolation mode for deployment groups
    #[serde(default = "default_database_isolation")]
    pub database_isolation: DatabaseIsolation,

    /// Environment variable name for the database URL
    /// Set to null to disable DATABASE_URL injection
    #[serde(default = "default_database_url_env_var")]
    pub database_url_env_var: Option<String>,

    /// Whether to inject PG* environment variables (PGHOST, PGPORT, PGDATABASE, PGUSER, PGPASSWORD)
    #[serde(default = "default_true")]
    pub inject_pg_vars: bool,
}

/// Status for the SnowflakePostgres custom resource
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct SnowflakePostgresStatus {
    /// Current provisioning state
    pub state: SnowflakePostgresState,

    /// Snowflake account endpoint (host:port)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,

    /// Encrypted master password (AES-GCM or KMS encrypted)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_password_encrypted: Option<String>,

    /// Master username
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_username: Option<String>,

    /// Per-deployment-group database status and credentials
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub databases: HashMap<String, DatabaseStatus>,

    /// Last error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Provisioning state for the SnowflakePostgres resource
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq, Default)]
#[serde(rename_all = "PascalCase")]
pub enum SnowflakePostgresState {
    /// Waiting to begin provisioning
    #[default]
    Pending,
    /// Currently provisioning the database instance
    Creating,
    /// Database is provisioned and available
    Available,
    /// Marked for deletion; teardown in progress
    Deleting,
    /// Resource has been fully deleted
    Deleted,
    /// Provisioning failed; see `error` field for details
    Failed,
}

/// Status and credentials for an individual database within the instance
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct DatabaseStatus {
    /// Username for this database
    pub user: String,

    /// Encrypted password for this user
    pub password_encrypted: String,

    /// Current provisioning state for this database
    pub status: DatabaseState,

    /// Timestamp when cleanup was scheduled (for inactive deployment groups)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_scheduled_at: Option<DateTime<Utc>>,
}

/// Provisioning state for an individual database within the instance
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum DatabaseState {
    Pending,
    Creating,
    Available,
    Terminating,
}
