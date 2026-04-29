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

#[derive(Debug, Deserialize)]
pub struct UpdateEnvironmentRequest {
    pub name: Option<String>,
    /// Use `Some(None)` to unset, `Some(Some(group))` to set, `None` to leave unchanged.
    #[serde(default)]
    pub primary_deployment_group: Option<Option<String>>,
    pub is_production: Option<bool>,
    pub color: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EnvironmentResponse {
    pub name: String,
    pub primary_deployment_group: Option<String>,
    pub is_production: bool,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<crate::db::models::Environment> for EnvironmentResponse {
    fn from(env: crate::db::models::Environment) -> Self {
        Self {
            name: env.name,
            primary_deployment_group: env.primary_deployment_group,
            is_production: env.is_production,
            color: env.color,
            created_at: env.created_at.to_rfc3339(),
            updated_at: env.updated_at.to_rfc3339(),
        }
    }
}
