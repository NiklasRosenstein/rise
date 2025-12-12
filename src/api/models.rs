//! Shared API request/response types
//!
//! These types are used by both the CLI (client) and server for API communication.
//! They are always available regardless of feature flags.

use serde::{Deserialize, Serialize};

// Re-export from server deployment models when server feature is enabled
#[cfg(feature = "server")]
pub use crate::server::deployment::models::*;

// When server feature is NOT enabled, define the types here for CLI use
#[cfg(not(feature = "server"))]
pub use self::client_models::*;

#[cfg(not(feature = "server"))]
mod client_models {
    use super::*;

    #[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
    pub enum DeploymentStatus {
        // Build/Deploy states
        #[default]
        Pending,
        Building,
        Pushing,
        Pushed,
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
        Expired,
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
                DeploymentStatus::Expired => write!(f, "Expired"),
            }
        }
    }

    #[derive(Debug, Deserialize, Serialize, Clone, Default)]
    pub struct Deployment {
        #[serde(default)]
        pub id: String,
        pub deployment_id: String,
        pub project: String,
        pub created_by: String,
        pub created_by_email: String,
        #[serde(default)]
        pub status: DeploymentStatus,
        #[serde(default = "default_group")]
        pub deployment_group: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub expires_at: Option<String>,
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

    fn default_group() -> String {
        DEFAULT_DEPLOYMENT_GROUP.to_string()
    }

    pub const DEFAULT_DEPLOYMENT_GROUP: &str = "default";
}
