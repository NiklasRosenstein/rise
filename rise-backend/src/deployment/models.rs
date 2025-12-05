use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[derive(Default)]
pub enum DeploymentStatus {
    // Build/Deploy states
    #[default]
    Pending,
    Building,
    Pushing,
    Pushed, // Handoff point between CLI and controller
    Deploying,

    // Running states
    Healthy,
    Unhealthy,

    // Cancellation states
    Cancelling,
    Cancelled,

    // Termination states
    Terminating,
    Stopped,
    Superseded,

    // Terminal states
    Failed,
}


impl std::fmt::Display for DeploymentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeploymentStatus::Pending => write!(f, "Pending"),
            DeploymentStatus::Building => write!(f, "Building"),
            DeploymentStatus::Pushing => write!(f, "Pushing"),
            DeploymentStatus::Pushed => write!(f, "Pushed"),
            DeploymentStatus::Deploying => write!(f, "Deploying"),
            DeploymentStatus::Healthy => write!(f, "Healthy"),
            DeploymentStatus::Unhealthy => write!(f, "Unhealthy"),
            DeploymentStatus::Cancelling => write!(f, "Cancelling"),
            DeploymentStatus::Cancelled => write!(f, "Cancelled"),
            DeploymentStatus::Terminating => write!(f, "Terminating"),
            DeploymentStatus::Stopped => write!(f, "Stopped"),
            DeploymentStatus::Superseded => write!(f, "Superseded"),
            DeploymentStatus::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Deployment {
    #[serde(default)]
    pub id: String,
    pub deployment_id: String,
    pub project: String,    // Project ID
    pub created_by: String, // User ID
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_digest: Option<String>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
}

// Request to create a deployment
#[derive(Debug, Deserialize)]
pub struct CreateDeploymentRequest {
    pub project: String, // Project name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>, // Optional pre-built image reference
}

// Response from creating a deployment
#[derive(Debug, Serialize)]
pub struct CreateDeploymentResponse {
    pub deployment_id: String,
    pub image_tag: String, // Full tag: registry_url/namespace/project:deployment_id
    pub credentials: crate::registry::models::RegistryCredentials,
}

// Request to update deployment status
#[derive(Debug, Deserialize)]
pub struct UpdateDeploymentStatusRequest {
    pub status: DeploymentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

// Response from rolling back a deployment
#[derive(Debug, Serialize)]
pub struct RollbackDeploymentResponse {
    pub new_deployment_id: String,
    pub rolled_back_from: String,
    pub image_tag: String,
}
