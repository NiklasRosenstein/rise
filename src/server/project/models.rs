use serde::{Deserialize, Serialize};

/// Access class information for API responses
#[derive(Debug, Serialize, Clone)]
pub struct AccessClassInfo {
    pub id: String,
    pub display_name: String,
    pub description: String,
}

/// Response for listing available access classes
#[derive(Debug, Serialize, Clone)]
pub struct ListAccessClassesResponse {
    pub access_classes: Vec<AccessClassInfo>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub enum ProjectStatus {
    Running,
    #[default]
    Stopped,
    Deploying,
    Failed,
    Deleting,
    Terminated,
}

impl From<crate::db::models::ProjectStatus> for ProjectStatus {
    fn from(status: crate::db::models::ProjectStatus) -> Self {
        match status {
            crate::db::models::ProjectStatus::Running => ProjectStatus::Running,
            crate::db::models::ProjectStatus::Stopped => ProjectStatus::Stopped,
            crate::db::models::ProjectStatus::Deploying => ProjectStatus::Deploying,
            crate::db::models::ProjectStatus::Failed => ProjectStatus::Failed,
            crate::db::models::ProjectStatus::Deleting => ProjectStatus::Deleting,
            crate::db::models::ProjectStatus::Terminated => ProjectStatus::Terminated,
        }
    }
}

impl From<ProjectStatus> for crate::db::models::ProjectStatus {
    fn from(status: ProjectStatus) -> Self {
        match status {
            ProjectStatus::Running => crate::db::models::ProjectStatus::Running,
            ProjectStatus::Stopped => crate::db::models::ProjectStatus::Stopped,
            ProjectStatus::Deploying => crate::db::models::ProjectStatus::Deploying,
            ProjectStatus::Failed => crate::db::models::ProjectStatus::Failed,
            ProjectStatus::Deleting => crate::db::models::ProjectStatus::Deleting,
            ProjectStatus::Terminated => crate::db::models::ProjectStatus::Terminated,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ProjectOwner {
    User(String), // User ID
    Team(String), // Team ID
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Project {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: ProjectStatus,
    #[serde(default)]
    pub access_class: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<OwnerInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_deployment_status: Option<String>, // Status of the active deployment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_url: Option<String>, // Default URL from ingress template
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_url: Option<String>, // Primary URL (starred custom domain, or default URL)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_domain_urls: Vec<String>, // Additional custom domain URLs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_groups: Option<Vec<String>>, // Active deployment groups
    #[serde(default)]
    pub finalizers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub app_users: Vec<UserInfo>, // Users who can access the deployed app
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub app_teams: Vec<TeamInfo>, // Teams whose members can access the deployed app
    // Timestamps
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateProjectRequest {
    pub name: String,
    pub access_class: String,
    pub owner: ProjectOwner,
    #[serde(default)]
    pub app_users: Vec<String>, // User emails or IDs
    #[serde(default)]
    pub app_teams: Vec<String>, // Team names or IDs
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateProjectResponse {
    pub project: Project,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub access_class: Option<String>,
    pub status: Option<ProjectStatus>,
    pub owner: Option<ProjectOwner>,
    pub app_users: Option<Vec<String>>, // User emails or IDs
    pub app_teams: Option<Vec<String>>, // Team names or IDs
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpdateProjectResponse {
    pub project: Project,
}

// User information for expanded responses
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
}

// Team information for expanded responses
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TeamInfo {
    pub id: String,
    pub name: String,
}

// Owner information enum
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum OwnerInfo {
    User(UserInfo),
    Team(TeamInfo),
}

// Error response with optional fuzzy match suggestions
#[derive(Debug, Serialize, Clone)]
pub struct ProjectErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<Vec<String>>,
}

// Query parameters for project lookup
#[derive(Debug, Deserialize, Clone)]
pub struct GetProjectParams {
    #[serde(default)]
    pub by_id: bool,
}
