use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateEnvironmentRequest {
    pub name: String,
    #[serde(default)]
    pub primary_deployment_group: Option<String>,
    #[serde(default)]
    pub is_production: bool,
    #[serde(default = "default_color")]
    pub color: String,
}

fn default_color() -> String {
    "green".to_string()
}

/// Per-environment deployment constraints (admin-settable)
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct EnvironmentDeploymentConstraints {
    pub min_replicas: Option<u32>,
    pub max_replicas: Option<u32>,
    pub min_cpu: Option<String>,
    pub max_cpu: Option<String>,
    pub min_memory: Option<String>,
    pub max_memory: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEnvironmentRequest {
    pub name: Option<String>,
    /// Use `Some(None)` to unset, `Some(Some(group))` to set, `None` to leave unchanged.
    #[serde(default)]
    pub primary_deployment_group: Option<Option<String>>,
    pub is_production: Option<bool>,
    pub color: Option<String>,
    /// Per-environment deployment constraints (admin-only)
    #[serde(default)]
    pub deployment_constraints: Option<EnvironmentDeploymentConstraints>,
}

#[derive(Debug, Serialize)]
pub struct EnvironmentResponse {
    pub name: String,
    pub primary_deployment_group: Option<String>,
    pub is_production: bool,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_constraints: Option<EnvironmentDeploymentConstraints>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<crate::db::models::Environment> for EnvironmentResponse {
    fn from(env: crate::db::models::Environment) -> Self {
        // Only include constraints if at least one field is set
        let deployment_constraints = if env.min_replicas.is_some()
            || env.max_replicas.is_some()
            || env.min_cpu.is_some()
            || env.max_cpu.is_some()
            || env.min_memory.is_some()
            || env.max_memory.is_some()
        {
            Some(EnvironmentDeploymentConstraints {
                min_replicas: env.min_replicas.and_then(|v| u32::try_from(v).ok()),
                max_replicas: env.max_replicas.and_then(|v| u32::try_from(v).ok()),
                min_cpu: env.min_cpu,
                max_cpu: env.max_cpu,
                min_memory: env.min_memory,
                max_memory: env.max_memory,
            })
        } else {
            None
        };

        Self {
            name: env.name,
            primary_deployment_group: env.primary_deployment_group,
            is_production: env.is_production,
            color: env.color,
            deployment_constraints,
            created_at: env.created_at.to_rfc3339(),
            updated_at: env.updated_at.to_rfc3339(),
        }
    }
}
