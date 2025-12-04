use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum DeploymentStatus {
    Pending,
    Building,
    Pushing,
    Pushed,     // Handoff point between CLI and controller
    Deploying,
    Completed,
    Failed,
}

impl Default for DeploymentStatus {
    fn default() -> Self {
        DeploymentStatus::Pending
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Deployment {
    #[serde(default)]
    pub id: String,
    pub deployment_id: String,
    pub project: String,  // Project ID
    pub created_by: String,     // User ID
    #[serde(default)]
    pub status: DeploymentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_logs: Option<String>,
    #[serde(default)]
    pub controller_metadata: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_url: Option<String>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
}

// Request to create a deployment
#[derive(Debug, Deserialize)]
pub struct CreateDeploymentRequest {
    pub project: String,  // Project name
}

// Response from creating a deployment
#[derive(Debug, Serialize)]
pub struct CreateDeploymentResponse {
    pub deployment_id: String,
    pub image_tag: String,  // Full tag: registry_url/namespace/project:deployment_id
    pub credentials: crate::registry::models::RegistryCredentials,
}

// Request to update deployment status
#[derive(Debug, Deserialize)]
pub struct UpdateDeploymentStatusRequest {
    pub status: DeploymentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}
