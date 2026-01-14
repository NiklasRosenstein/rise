//! Project API request/response types and client functions

use serde::{Deserialize, Serialize};

/// Project status
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum ProjectStatus {
    Running,
    Stopped,
    Deploying,
    Failed,
    Deleting,
}

impl std::fmt::Display for ProjectStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectStatus::Running => write!(f, "Running"),
            ProjectStatus::Stopped => write!(f, "Stopped"),
            ProjectStatus::Deploying => write!(f, "Deploying"),
            ProjectStatus::Failed => write!(f, "Failed"),
            ProjectStatus::Deleting => write!(f, "Deleting"),
        }
    }
}

/// Project resource
#[derive(Debug, Deserialize, Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub status: ProjectStatus,
    pub access_class: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_team: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_user_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_team_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_deployment_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_url: Option<String>,
    #[serde(default)]
    pub custom_domain_urls: Vec<String>,
}

/// User information in owner context
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
}

/// Team information in owner context
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TeamInfo {
    pub id: String,
    pub name: String,
}

/// Owner information (user or team)
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum OwnerInfo {
    User(UserInfo),
    Team(TeamInfo),
}

/// Project with expanded owner information
#[derive(Debug, Deserialize)]
pub struct ProjectWithOwnerInfo {
    pub id: String,
    pub name: String,
    pub status: ProjectStatus,
    #[allow(dead_code)]
    pub access_class: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<OwnerInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_url: Option<String>,
    #[serde(default)]
    pub custom_domain_urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_groups: Option<Vec<String>>,
    #[serde(default)]
    pub finalizers: Vec<String>,
}

/// Error response from project API
#[derive(Debug, Deserialize)]
pub struct ProjectErrorResponse {
    pub error: String,
    pub suggestions: Option<Vec<String>>,
}

/// Response from project create endpoint
#[derive(Debug, Deserialize)]
pub struct CreateProjectResponse {
    pub project: Project,
}

/// Response from project update endpoint
#[derive(Debug, Deserialize)]
pub struct UpdateProjectResponse {
    pub project: Project,
}

/// Domain item in list response
#[derive(Debug, Deserialize)]
pub struct DomainItem {
    pub domain: String,
}

/// Response from domains list endpoint
#[derive(Debug, Deserialize)]
pub struct DomainsResponse {
    pub domains: Vec<DomainItem>,
}

/// Environment variable item
#[derive(Debug, Deserialize)]
pub struct EnvVarItem {
    pub key: String,
    pub is_secret: bool,
}

/// Response from env vars list endpoint
#[derive(Debug, Deserialize)]
pub struct EnvVarsResponse {
    pub env_vars: Vec<EnvVarItem>,
}

/// Current user information
#[derive(Debug, Deserialize)]
pub struct MeResponse {
    #[allow(dead_code)]
    pub id: String,
    #[allow(dead_code)]
    pub email: String,
}
